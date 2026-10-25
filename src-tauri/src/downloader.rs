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
                let _ = tokio::fs::remove_file(&task.dest).await;
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

    let mut req = client.get(url).timeout(Duration::from_secs(120));
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
