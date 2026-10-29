//! 多源回退下载引擎：国内镜像优先，一级一级回退到官方源
//! 特性：断点续传(Range)、分片并发、SHA1 校验、进度事件、失败自动切换源

use futures_util::StreamExt;
use reqwest::header::{HeaderValue, CONTENT_LENGTH, CONTENT_RANGE, RANGE};
use reqwest::Client;
use serde::Serialize;
use sha1::{Digest, Sha1};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use tokio::io::AsyncWriteExt;
use tokio::sync::Semaphore;

pub const BMCLAPI: &str = "https://bmclapi2.bangbang93.com";
pub const BMCLAPI_ALT: &str = "https://bmclapi.bangbang93.com";

#[derive(Debug, Clone, Serialize)]
pub struct DlProgress {
    pub id: String,
    pub name: String,
    pub loaded: u64,
    pub total: u64,
    /// 字节/秒
    pub speed: u64,
    pub finished: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DlTask {
    /// 任务唯一 id（前端下载坞用）
    pub id: String,
    /// 展示名
    pub name: String,
    /// 首选 URL
    pub url: String,
    /// 回退源（按顺序尝试）
    pub fallbacks: Vec<String>,
    pub dest: PathBuf,
    /// SHA1 校验（MC 资源都带 sha1，可选）
    pub sha1: Option<String>,
}

impl DlTask {
    pub fn new(id: &str, name: &str, url: &str, dest: &Path) -> Self {
        Self {
            id: id.to_string(),
            name: name.to_string(),
            url: url.to_string(),
            fallbacks: vec![],
            dest: dest.to_path_buf(),
            sha1: None,
        }
    }

    pub fn with_fallbacks(mut self, urls: Vec<String>) -> Self {
        self.fallbacks = urls;
        self
    }

    pub fn with_sha1(mut self, sha1: Option<String>) -> Self {
        self.sha1 = sha1.filter(|s| !s.is_empty());
        self
    }
}

fn emit_progress(app: &AppHandle, p: DlProgress) {
    let _ = app.emit("download-progress", p);
}

/// 带断点续传 + SHA1 校验的单文件下载。返回已存在的/下载完成的文件路径。
pub async fn download_one(
    client: &Client,
    app: &AppHandle,
    task: &DlTask,
) -> Result<PathBuf, String> {
    if let Some(parent) = task.dest.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| format!("创建目录失败 {}: {e}", parent.display()))?;
    }

    // 已存在且 SHA1 匹配 → 跳过
    if task.dest.exists() {
        if let Some(sha) = &task.sha1 {
            if let Ok(existing) = sha1_of_file(&task.dest).await {
                if &existing == sha {
                    emit_progress(
                        app,
                        DlProgress {
                            id: task.id.clone(),
                            name: task.name.clone(),
                            loaded: 0,
                            total: 0,
                            speed: 0,
                            finished: true,
                            error: None,
                        },
                    );
                    return Ok(task.dest.clone());
                }
            }
        }
    }

    let urls: Vec<String> = std::iter::once(task.url.clone())
        .chain(task.fallbacks.iter().cloned())
        .collect();
    let mut last_err = String::new();

    for url in urls {
        match try_download(client, app, task, &url).await {
            Ok(()) => {
                // SHA1 校验
                if let Some(sha) = &task.sha1 {
                    let actual = sha1_of_file(&task.dest)
                        .await
                        .map_err(|e| format!("SHA1 计算失败: {e}"))?;
                    if &actual != sha {
                        let _ = tokio::fs::remove_file(&task.dest).await;
                        last_err = format!("SHA1 校验失败(源 {url}): 期望 {sha} 实际 {actual}");
                        emit_progress(
                            app,
                            DlProgress {
                                id: task.id.clone(),
                                name: task.name.clone(),
                                loaded: 0,
                                total: 0,
                                speed: 0,
                                finished: false,
                                error: Some(last_err.clone()),
                            },
                        );
                        continue;
                    }
                }
                emit_progress(
                    app,
                    DlProgress {
                        id: task.id.clone(),
                        name: task.name.clone(),
                        loaded: 0,
                        total: 0,
                        speed: 0,
                        finished: true,
                        error: None,
                    },
                );
                return Ok(task.dest.clone());
            }
            Err(e) => {
                last_err = e;
                // 保留已下载的分片：下一次尝试会用 Range 续传，
                // 不要在这里删文件，否则大文件每次断流都要从 0 重来。
            }
        }
    }
    Err(format!("所有源均下载失败 [{}]: {last_err}", task.name))
}

async fn try_download(
    client: &Client,
    app: &AppHandle,
    task: &DlTask,
    url: &str,
) -> Result<(), String> {
    let existing_len = if task.dest.exists() {
        tokio::fs::metadata(&task.dest)
            .await
            .map(|m| m.len())
            .unwrap_or(0)
    } else {
        0
    };

    // 不设整体超时：客户端的 read_timeout（60 秒无字节）负责识别断流。
    // 整体超时会把慢速大文件（如 22 MB 的 JRE 包）在快下完时掐断，
    // 表现为「进度条下着下着归零重来」。
    let mut req = client.get(url);
    if existing_len > 0 {
        req = req.header(RANGE, HeaderValue::from_str(&format!("bytes={existing_len}-")).unwrap());
    }

    let resp = req.send().await.map_err(|e| format!("请求失败: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {} ({url})", resp.status().as_u16()));
    }

    let total = resp
        .headers()
        .get(CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0);
    let is_partial = resp.status().as_u16() == 206
        || resp.headers().contains_key(CONTENT_RANGE);

    let mut loaded = if is_partial { existing_len } else { 0 };
    let file_total = if is_partial { existing_len + total } else { total };

    let mut file = if is_partial && existing_len > 0 {
        tokio::fs::OpenOptions::new()
            .append(true)
            .open(&task.dest)
            .await
            .map_err(|e| format!("打开文件失败: {e}"))?
    } else {
        tokio::fs::File::create(&task.dest)
            .await
            .map_err(|e| format!("创建文件失败: {e}"))?
    };

    let mut stream = resp.bytes_stream();
    let mut last_emit = Instant::now();
    let counter = Arc::new(AtomicU64::new(0));
    let timer = Instant::now();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("传输中断: {e}"))?;
        file.write_all(&chunk)
            .await
            .map_err(|e| format!("写入失败: {e}"))?;
        loaded += chunk.len() as u64;
        counter.fetch_add(chunk.len() as u64, Ordering::Relaxed);

        if last_emit.elapsed() >= Duration::from_millis(150) {
            let speed = counter.swap(0, Ordering::Relaxed) as f64 / last_emit.elapsed().as_secs_f64().max(0.001);
            last_emit = Instant::now();
            emit_progress(
                app,
                DlProgress {
                    id: task.id.clone(),
                    name: task.name.clone(),
                    loaded,
                    total: file_total,
                    speed: speed as u64,
                    finished: false,
                    error: None,
                },
            );
        }
    }
    file.flush().await.map_err(|e| e.to_string())?;
    let _ = timer;
    Ok(())
}

// ---------------------------------------------------------------- 分片并发下载

/// 分片下载的共享状态（各线程累加进度、节流发事件）
struct SegCtx {
    loaded: AtomicU64,
    start: Instant,
    last_emit_ms: AtomicU64,
    total: u64,
    id: String,
    name: String,
}

impl SegCtx {
    fn add(&self, n: u64) -> u64 {
        self.loaded.fetch_add(n, Ordering::Relaxed) + n
    }

    /// 节流发进度（约 5 次/秒），速度按「已下载/耗时」算平均速度
    fn emit(&self, app: &AppHandle, loaded: u64) {
        let now = self.start.elapsed().as_millis() as u64;
        let last = self.last_emit_ms.load(Ordering::Relaxed);
        if now.saturating_sub(last) < 200 {
            return;
        }
        if self
            .last_emit_ms
            .compare_exchange(last, now, Ordering::Relaxed, Ordering::Relaxed)
            .is_err()
        {
            return;
        }
        let speed = (loaded as f64 / self.start.elapsed().as_secs_f64().max(0.001)) as u64;
        emit_progress(
            app,
            DlProgress {
                id: self.id.clone(),
                name: self.name.clone(),
                loaded,
                total: self.total,
                speed,
                finished: false,
                error: None,
            },
        );
    }
}

fn part_path(dest: &Path, idx: usize) -> PathBuf {
    let mut name = dest.file_name().map(|s| s.to_os_string()).unwrap_or_default();
    name.push(format!(".part{idx}"));
    dest.with_file_name(name)
}

/// 分段抓取 `[start, end]` 到 `part`，返回本片字节数
///
/// 单片断流后在原文件上续传（`Range` 从已有长度开始），最多 3 次。
async fn fetch_range(
    client: &Client,
    app: &AppHandle,
    ctx: &SegCtx,
    url: &str,
    part: &Path,
    start: u64,
    end: u64,
) -> Result<u64, String> {
    const MAX_TRY: u32 = 3;
    let want = end - start + 1;
    let mut last_err = String::new();

    for attempt in 1..=MAX_TRY {
        let mut have = tokio::fs::metadata(part).await.map(|m| m.len()).unwrap_or(0);
        if have == want {
            return Ok(want);
        }
        if have > want {
            let _ = tokio::fs::remove_file(part).await;
            have = 0;
        }
        let from = start + have;
        let header = HeaderValue::from_str(&format!("bytes={from}-{end}"))
            .map_err(|e| format!("Range 头非法: {e}"))?;

        let resp = match client.get(url).header(RANGE, header).send().await {
            Ok(r) => r,
            Err(e) => {
                last_err = format!("分片 {start}-{end} 请求失败: {e}");
                continue;
            }
        };
        // 只要不是 206 就说明该源不支持分段（或内容已变），交给调用方回退单流下载
        if resp.status().as_u16() != 206 {
            return Err(format!(
                "分片请求返回 HTTP {}（该源不支持 Range 分段）",
                resp.status().as_u16()
            ));
        }

        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(part)
            .await
            .map_err(|e| format!("打开分片文件失败: {e}"))?;

        let mut stream = resp.bytes_stream();
        let mut broken = false;
        while let Some(chunk) = stream.next().await {
            match chunk {
                Ok(bytes) => {
                    file.write_all(&bytes)
                        .await
                        .map_err(|e| format!("写入分片失败: {e}"))?;
                    let loaded = ctx.add(bytes.len() as u64);
                    ctx.emit(app, loaded);
                }
                Err(e) => {
                    last_err = format!("分片 {start}-{end} 传输中断: {e}");
                    broken = true;
                    break;
                }
            }
        }
        let _ = file.flush().await;
        drop(file);
        if broken {
            if attempt < MAX_TRY {
                tokio::time::sleep(Duration::from_millis(500 * attempt as u64)).await;
            }
            continue;
        }

        let have = tokio::fs::metadata(part).await.map(|m| m.len()).unwrap_or(0);
        if have == want {
            return Ok(want);
        }
        last_err = format!("分片 {start}-{end} 未下完（{have}/{want}）");
        if attempt < MAX_TRY {
            tokio::time::sleep(Duration::from_millis(500 * attempt as u64)).await;
        }
    }

    Err(last_err)
}

/// 多线程分片下载一个大文件
///
/// 把 `[0, total)` 切成 `segments` 段，用 `Range` 并发抓取，各写 `.partN`，
/// 全部到齐后顺序合并成目标文件。弱网下单连接往往只有 100~200 KB/s，
/// 分片后能明显跑满带宽。
///
/// 返回 `Err` 的两种情况由调用方回退到单流 `download_one`：
/// - 文件太小（< 4 MB，分片反而更慢）
/// - 源不支持 `Range`（探测请求没拿到 206）
pub async fn download_multi(
    client: &Client,
    app: &AppHandle,
    task: &DlTask,
    total: u64,
    max_segments: usize,
) -> Result<PathBuf, String> {
    /// 每片最小 2 MB：片太小握手开销占比高
    const MIN_SEG: u64 = 2 * 1024 * 1024;
    if total < MIN_SEG * 2 {
        return Err("文件较小，走单流下载".to_string());
    }

    // 探测 Range 支持（请求第一个字节，期望 206 + Content-Range）
    let probe = client
        .get(&task.url)
        .header(RANGE, HeaderValue::from_static("bytes=0-0"))
        .send()
        .await
        .map_err(|e| format!("探测分段支持失败: {e}"))?;
    if probe.status().as_u16() != 206 {
        return Err(format!(
            "该源不支持分段下载（HTTP {}）",
            probe.status().as_u16()
        ));
    }
    drop(probe);

    let seg_count = ((total / MIN_SEG) as usize).clamp(2, max_segments.max(2));
    let seg_size = total.div_ceil(seg_count as u64);
    let ranges: Vec<(usize, u64, u64)> = (0..seg_count)
        .map(|i| {
            let start = i as u64 * seg_size;
            let end = ((i as u64 + 1) * seg_size).min(total) - 1;
            (i, start, end)
        })
        .filter(|(_, s, e)| e >= s)
        .collect();

    let ctx = Arc::new(SegCtx {
        loaded: AtomicU64::new(0),
        start: Instant::now(),
        last_emit_ms: AtomicU64::new(0),
        total,
        id: task.id.clone(),
        name: task.name.clone(),
    });

    let mut handles = Vec::with_capacity(ranges.len());
    for (idx, start, end) in &ranges {
        let client = client.clone();
        let app = app.clone();
        let ctx = ctx.clone();
        let url = task.url.clone();
        let part = part_path(&task.dest, *idx);
        let (s, e) = (*start, *end);
        handles.push(tokio::spawn(async move {
            fetch_range(&client, &app, &ctx, &url, &part, s, e).await
        }));
    }

    let results = futures_util::future::join_all(handles).await;
    let mut err: Option<String> = None;
    for r in results {
        match r {
            Ok(Ok(_)) => {}
            Ok(Err(e)) => {
                if err.is_none() {
                    err = Some(e);
                }
            }
            Err(e) => {
                if err.is_none() {
                    err = Some(format!("下载线程异常: {e}"));
                }
            }
        }
    }
    if let Some(e) = err {
        // 保留各分片：下次调用会接着已有的 `.partN` 续传，不必从头再来
        return Err(e);
    }

    // 合并分片
    if let Some(parent) = task.dest.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| format!("创建目录失败: {e}"))?;
    }
    let mut out = tokio::fs::File::create(&task.dest)
        .await
        .map_err(|e| format!("创建合并文件失败: {e}"))?;
    for (idx, _, _) in &ranges {
        let part = part_path(&task.dest, *idx);
        let mut f = tokio::fs::File::open(&part)
            .await
            .map_err(|e| format!("打开分片失败: {e}"))?;
        tokio::io::copy(&mut f, &mut out)
            .await
            .map_err(|e| format!("合并分片失败: {e}"))?;
        drop(f);
        let _ = tokio::fs::remove_file(&part).await;
    }
    out.flush().await.map_err(|e| e.to_string())?;
    drop(out);

    let got = tokio::fs::metadata(&task.dest)
        .await
        .map(|m| m.len())
        .unwrap_or(0);
    if got != total {
        let _ = tokio::fs::remove_file(&task.dest).await;
        return Err(format!("合并后大小不符：期望 {total} 字节，实际 {got} 字节"));
    }

    emit_progress(
        app,
        DlProgress {
            id: task.id.clone(),
            name: task.name.clone(),
            loaded: 0,
            total: 0,
            speed: 0,
            finished: true,
            error: None,
        },
    );
    Ok(task.dest.clone())
}

pub async fn sha1_of_file(path: &Path) -> Result<String, String> {
    let data = tokio::fs::read(path)
        .await
        .map_err(|e| format!("读取文件失败 {}: {e}", path.display()))?;
    let mut hasher = Sha1::new();
    hasher.update(&data);
    Ok(hex(&hasher.finalize()))
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// 并发下载一批任务；单个失败会中断整体（安装游戏需要全部文件）。
/// concurrency 建议 8~32。
pub async fn download_many(
    client: &Client,
    app: &AppHandle,
    tasks: Vec<DlTask>,
    concurrency: usize,
) -> Result<(), String> {
    let sem = Arc::new(Semaphore::new(concurrency.max(1)));
    let mut set = tokio::task::JoinSet::new();

    for task in tasks {
        let permit = sem.clone().acquire_owned().await.unwrap();
        let client = client.clone();
        let app = app.clone();
        set.spawn(async move {
            let _permit = permit;
            download_one(&client, &app, &task).await.map(|_| ())
        });
    }

    let mut first_err: Option<String> = None;
    while let Some(joined) = set.join_next().await {
        match joined {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                if first_err.is_none() {
                    first_err = Some(e);
                }
            }
            Err(e) => {
                if first_err.is_none() {
                    first_err = Some(format!("任务 panic: {e}"));
                }
            }
        }
    }
    match first_err {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

// ---------------------------------------------------------------- 镜像改写

/// 把官方 Mojang URL 改写为 BMCLAPI 镜像 URL，并生成回退链
/// prefer_mirror=true 时 [镜像, 官方]；false 时 [官方, 镜像]
pub fn mirror_urls(orig: &str, prefer_mirror: bool) -> Vec<String> {
    let mirror = |u: &str| -> Option<String> {
        if let Some(p) = u.strip_prefix("https://piston-meta.mojang.com/") {
            Some(format!("{BMCLAPI}/{p}"))
        } else if let Some(p) = u.strip_prefix("https://piston-data.mojang.com/") {
            Some(format!("{BMCLAPI}/{p}"))
        } else if let Some(p) = u.strip_prefix("https://launcher.mojang.com/") {
            Some(format!("{BMCLAPI}/{p}"))
        } else if let Some(p) = u.strip_prefix("https://launchermeta.mojang.com/") {
            Some(format!("{BMCLAPI}/{p}"))
        } else if let Some(p) = u.strip_prefix("https://libraries.minecraft.net/") {
            Some(format!("{BMCLAPI}/maven/{p}"))
        } else if let Some(p) = u.strip_prefix("https://resources.download.minecraft.net/") {
            Some(format!("{BMCLAPI}/assets/{p}"))
        } else {
            None
        }
    }(orig);

    match mirror {
        Some(m) if prefer_mirror => vec![m, orig.to_string()],
        Some(m) => vec![orig.to_string(), m],
        None => vec![orig.to_string()],
    }
}

/// 根据设置生成回退链（download_source: bmclapi/mojang）
pub fn urls_with_setting(orig: &str, prefer_mirror: bool) -> Vec<String> {
    let mut urls = mirror_urls(orig, prefer_mirror);
    // BMCLAPI2 挂了再试 BMCLAPI1（如果首选是镜像）
    if prefer_mirror {
        if let Some(first) = urls.first().cloned() {
            if first.contains("bmclapi2.bangbang93.com") {
                urls.insert(1, first.replace("bmclapi2", "bmclapi"));
            }
        }
    }
    urls
}

pub fn http_client() -> Client {
    // 走全局代理配置（设置 → 网络代理）：国内直连 GitHub Release 只有 ~45 KB/s，
    // 走 Clash mixed 端口可到 ~190 KB/s
    crate::proxy::client()
}
