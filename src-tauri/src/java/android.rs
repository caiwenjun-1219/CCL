//! Android 端 Java 运行时管理
//!
//! ## 为什么不能复用桌面的 Adoptium 流程
//!
//! Adoptium 提供的是 **glibc / Windows x64** 的 JDK，在 Android（bionic libc、
//! aarch64）上根本无法执行。Android 端需要的是**交叉编译到 Android 的 OpenJDK**，
//! 即 FCL / PojavLauncher 生态使用的「Android JRE」。
//!
//! ## 采用的方案（对齐 FCL）
//!
//! FCL 把 Android JRE 打包进 APK assets（`app_runtime/java/jre{8,17,21,25}/`），
//! 首次启动解包 + 打补丁。我们走**运行时下载**同构的产物，来源用 EclipseJRE
//! （FCL 同生态的 OpenJDK-Android 构建流水线），产物布局与 FCL 完全一致：
//!
//! ```text
//! jre{major}-universal.tar.xz     # 通用部分：conf/ legal/ lib/(非 native)
//! jre{major}-bin-{arch}.tar.xz    # 架构部分：bin/ lib/*.so lib/server/libjvm.so
//! ```
//!
//! 两者**叠加**解到同一目录才是一个完整 JRE（`universal` 先解，`bin-<arch>` 后解覆盖）。
//! 每个文件都有官方 sha256，下载后逐个校验。
//!
//! **Java 8 的特殊处理**：其 universal 里是 pack200 压缩的 jar（如 `lib/rt.jar.pack`），
//! 解压后要用随包自带的 `bin/unpack200` 解包成 `.jar` 才是可用 JRE；Java 17/21/25 无此问题。
//! 这一步对应 FCL 的 `patchJava()` / `Pack200Utils.unpack`，我们直接调用 JRE 自带 binary。
//!
//! ## 架构映射
//!
//! Pojav 生态的 arch 命名与 Android ABI 的对应：
//! `arm64` ↔ `arm64-v8a`，`arm` ↔ `armeabi-v7a`，`x86_64` ↔ `x86_64`，`x86` ↔ `x86`。

use serde::Deserialize;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter};

use super::common::{parse_major, required_java_major, select_java, JavaInfo};
use crate::downloader::{self, DlTask};
use crate::settings;

/// EclipseJRE 发布页原始地址（GitHub Release）
///
/// 注意：本机与国内移动网络直连 `github.com/.../releases/download/...` 常被重置
/// （`error sending request for url`），因此实际请求会先套一层加速前缀，
/// 见 [`source_prefixes`]。
const GH_RELEASE: &str =
    "https://github.com/SouhardyaMaybe/EclipseJRE/releases/download/jres-v1.0.0";

/// 内置 GitHub 加速前缀（按优先级尝试）。空串 = 直连 GitHub。
///
/// - `axisnow.gh-proxy.org`：gh-proxy 的三网优选节点（移动/联通/电信），国内首选
/// - `gh-proxy.com` / `ghproxy.net`：备用公共节点
/// - 空串：直连兜底（2026-10 实测国内基本被重置，仅作最后尝试）
const BUILTIN_PREFIXES: &[&str] = &[
    "https://axisnow.gh-proxy.org/",
    "https://gh-proxy.com/",
    "https://ghproxy.net/",
    "",
];

/// Android 端支持的 JRE 主版本（对应 EclipseJRE 构建的 8/17/21/25）
pub const SUPPORTED_MAJORS: &[u32] = &[8, 17, 21, 25];

/// 下载源名称（前端展示用）
pub const SOURCE_NAME: &str = "EclipseJRE（Android 交叉编译 OpenJDK）";

/// 当前平台可下载的 Java 主版本
pub fn supported_majors() -> &'static [u32] {
    SUPPORTED_MAJORS
}

/// 设备 ABI → Pojav 生态的 arch 命名
fn device_arch() -> &'static str {
    match std::env::consts::ARCH {
        "aarch64" => "arm64",
        "arm" => "arm",
        "x86_64" => "x86_64",
        "x86" => "x86",
        _ => "arm64",
    }
}

/// `jre{major}-multiarch.json` —— 官方清单，含每个文件的 sha256 与大小
#[derive(Debug, Deserialize)]
struct MultiArchManifest {
    java_version: u32,
    build_id: String,
    universal: Asset,
    architectures: Vec<ArchEntry>,
}

/// 一个待下载的文件（universal 与 arch 包共用）
#[derive(Debug, Deserialize, Clone)]
struct Asset {
    name: String,
    sha256: String,
    #[allow(dead_code)]
    size: u64,
}

#[derive(Debug, Deserialize)]
struct ArchEntry {
    arch: String,
    name: String,
    sha256: String,
    #[allow(dead_code)]
    size: u64,
}

impl ArchEntry {
    fn as_asset(&self) -> Asset {
        Asset {
            name: self.name.clone(),
            sha256: self.sha256.clone(),
            size: self.size,
        }
    }
}

/// 规整用户填写的加速前缀：补协议、补结尾斜杠；空串原样返回（= 直连）
fn normalize_prefix(raw: &str) -> String {
    let t = raw.trim();
    if t.is_empty() {
        return String::new();
    }
    let mut s = t.to_string();
    if !s.starts_with("http://") && !s.starts_with("https://") {
        s = format!("https://{s}");
    }
    if !s.ends_with('/') {
        s.push('/');
    }
    s
}

/// 候选下载前缀：自定义（设置里填的）优先，其后是内置加速源与直连
fn source_prefixes(app: &AppHandle) -> Vec<String> {
    let custom = normalize_prefix(&settings::load(app).github_mirror);
    let mut out: Vec<String> = Vec::with_capacity(BUILTIN_PREFIXES.len() + 1);
    if !custom.is_empty() {
        out.push(custom);
    }
    for p in BUILTIN_PREFIXES {
        let p = p.to_string();
        if !out.contains(&p) {
            out.push(p);
        }
    }
    out
}

/// 用指定前缀拼出 release 资产的完整地址
fn release_url(prefix: &str, file: &str) -> String {
    format!("{prefix}{GH_RELEASE}/{file}")
}

/// 人类可读的源名称（错误信息用）
fn source_label(prefix: &str) -> &str {
    if prefix.is_empty() {
        "GitHub 直连"
    } else {
        prefix
    }
}

/// 从指定源拉取 `jre{major}-multiarch.json`
async fn fetch_manifest_from(
    client: &reqwest::Client,
    major: u32,
    prefix: &str,
) -> Result<MultiArchManifest, String> {
    let url = release_url(prefix, &format!("jre{major}-multiarch.json"));
    let resp = client
        .get(&url)
        .timeout(std::time::Duration::from_secs(45))
        .send()
        .await
        .map_err(|e| format!("请求失败: {e}"))?;
    let status = resp.status();
    let body = resp
        .text()
        .await
        .map_err(|e| format!("读取失败: {e}"))?;
    if !status.is_success() {
        return Err(format!(
            "HTTP {status}：{}",
            body.chars().take(120).collect::<String>()
        ));
    }
    serde_json::from_str(&body).map_err(|e| format!("清单解析失败: {e}"))
}

/// 依次尝试各加速源拉清单，返回「命中的清单 + 排好序的前缀列表」
///
/// 命中的源会被提到最前，后续两个归档包优先从同一个源下载（换源会重新握手，更慢）；
/// 该源后续失败时仍会自动回退到其余源。
async fn pick_manifest(
    client: &reqwest::Client,
    app: &AppHandle,
    major: u32,
) -> Result<(MultiArchManifest, Vec<String>), String> {
    let prefixes = source_prefixes(app);
    let mut errors: Vec<String> = Vec::new();
    for prefix in &prefixes {
        match fetch_manifest_from(client, major, prefix).await {
            Ok(m) => {
                let mut ordered = vec![prefix.clone()];
                ordered.extend(prefixes.iter().filter(|p| *p != prefix).cloned());
                return Ok((m, ordered));
            }
            Err(e) => errors.push(format!("{}：{e}", source_label(prefix))),
        }
    }
    Err(format!(
        "请求 JRE 清单失败，所有下载源均不可用：\n  {}\n可在「设置 → 网络 → GitHub 加速」里填写自己的加速地址（如 https://axisnow.gh-proxy.org/）。",
        errors.join("\n  ")
    ))
}

/// 计算文件的 sha256（十六进制小写）
fn sha256_file(path: &Path) -> Result<String, String> {
    use sha2::{Digest, Sha256};
    let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = file.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

/// 解开一个 `.tar.xz`：xz 解压 → tar 解包到 dest（保留符号链接）
///
/// 纯 Rust 实现（`lzma-rs` + `tar`），不依赖 liblzma，Android 交叉编译无需 NDK 工具链。
fn extract_tar_xz(tar_xz: &Path, dest: &Path) -> Result<(usize, usize), String> {
    let f = std::fs::File::open(tar_xz).map_err(|e| format!("打开压缩包失败: {e}"))?;
    let mut reader = BufReader::new(f);

    // xz → tar 字节流
    let mut tar_bytes: Vec<u8> = Vec::new();
    lzma_rs::xz_decompress(&mut reader, &mut tar_bytes)
        .map_err(|e| format!("xz 解压失败: {e}"))?;

    let mut archive = tar::Archive::new(std::io::Cursor::new(tar_bytes));
    let mut files = 0usize;
    let mut links = 0usize;
    let entries = archive
        .entries()
        .map_err(|e| format!("读取 tar 失败: {e}"))?;
    for entry in entries {
        let mut entry = entry.map_err(|e| format!("读取 tar 条目失败: {e}"))?;
        let path = entry
            .path()
            .map_err(|e| format!("tar 路径非法: {e}"))?
            .into_owned();
        // 防目录穿越：拒绝任何含 .. 或根分量的路径
        if path.components().any(|c| {
            matches!(
                c,
                std::path::Component::ParentDir | std::path::Component::RootDir
            )
        }) {
            continue;
        }
        let out = dest.join(&path);
        let ftype = entry.header().entry_type();
        if ftype.is_dir() {
            let _ = std::fs::create_dir_all(&out);
            continue;
        }
        if let Some(parent) = out.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if ftype.is_symlink() {
            // Pojav 的 JRE 里 legal/ 下有 136 个相对符号链接（JDK 各模块的 LICENSE），
            // 不还原会导致法律文件缺失，保留它们。
            let target = entry
                .link_name()
                .map_err(|e| format!("符号链接读取失败: {e}"))?
                .ok_or("符号链接缺少目标")?;
            let _ = std::fs::remove_file(&out);
            #[cfg(unix)]
            {
                let _ = std::os::unix::fs::symlink(&target, &out);
            }
            links += 1;
            continue;
        }
        let mut o = std::fs::File::create(&out).map_err(|e| format!("创建文件失败: {e}"))?;
        std::io::copy(&mut entry, &mut o).map_err(|e| format!("写入文件失败: {e}"))?;
        files += 1;
    }
    Ok((files, links))
}

/// 扫描已安装的 Android JRE
///
/// Android 上无法执行 `java -version`（bionic 下也缺少 shell 环境），
/// 因此改读 JRE 根目录的 `release` 文件取 `JAVA_VERSION="..."`，
/// 这也是 FCL 的 `JavaManager.addToJavaVersion` 采用的方式。
fn probe_jre(dir: &Path) -> Option<JavaInfo> {
    let release = dir.join("release");
    let text = std::fs::read_to_string(&release).ok()?;
    let ver = text
        .lines()
        .find_map(|l| l.strip_prefix("JAVA_VERSION=\"")?.strip_suffix('"'))
        .unwrap_or("");
    if ver.is_empty() {
        return None;
    }
    let major = parse_major(ver)?;
    Some(JavaInfo {
        path: dir.join("bin").join("java").to_string_lossy().to_string(),
        major,
        version: ver.to_string(),
        vendor: "Eclipse JRE for Android".to_string(),
        managed: true,
    })
}

/// 列出已安装的 Android JRE
pub fn scan_javas(app: AppHandle) -> Vec<JavaInfo> {
    let root = settings::java_dir(&app);
    let mut result: Vec<JavaInfo> = vec![];
    if let Ok(entries) = std::fs::read_dir(&root) {
        for entry in entries.flatten() {
            let dir = entry.path();
            if !dir.is_dir() {
                continue;
            }
            if let Some(info) = probe_jre(&dir) {
                result.push(info);
            }
        }
    }
    result.sort_by(|a, b| a.major.cmp(&b.major));
    result
}

/// 下载一个 JRE 组件并校验 sha256 / 大小，失败自动重试
///
/// 弱网下断流很常见，因此：
/// 1. **保留已下载的分片**，重试时由 `try_download` 用 `Range` 续传，
///    这样进度条只会往前走，不会归零（旧版每次重试都删文件重下，大包永远下不完）
/// 2. 只有「文件比期望大」或「sha256 不符」这类无法续传的情况才删文件重下
/// 3. 单个源内重试 4 次，仍不成再换下一个加速源
async fn download_verified(
    client: &reqwest::Client,
    app: &AppHandle,
    major: u32,
    idx: usize,
    entry: &Asset,
    dest_dir: &Path,
    prefixes: &[String],
) -> Result<PathBuf, String> {
    /// 单个源内的重试次数（续传，所以多试几次很便宜）
    const MAX_TRY: u32 = 4;
    let file_name = &entry.name;
    let dl_path = dest_dir.join(file_name);
    let mut last_err = String::new();

    for (si, prefix) in prefixes.iter().enumerate() {
        let url = release_url(prefix, file_name);
        for attempt in 1..=MAX_TRY {
            let task = DlTask::new(
                &format!("java-{major}-{idx}"),
                &format!("Java {major}（{file_name}）"),
                &url,
                &dl_path,
            );
            // 优先多源并行分片：单个加速节点跑到几 MB 就会被限流/断连，
            // 把分片摊到所有可用源上，各域名配额独立，总吞吐叠加。
            // 分片失败（都不支持 Range / 大文件断流）则回退到单流下载。
            let mut downloaded = false;
            if entry.size >= 2 * 1024 * 1024 {
                let urls: Vec<String> = prefixes
                    .iter()
                    .map(|p| release_url(p, file_name))
                    .collect();
                match downloader::download_multi(client, app, &task, &urls, entry.size, 8).await {
                    Ok(_) => downloaded = true,
                    Err(e) => last_err = format!("{}：{e}", source_label(prefix)),
                }
            }
            if !downloaded {
                match downloader::download_one(client, app, &task).await {
                    Ok(_) => downloaded = true,
                    Err(e) => {
                        last_err = format!("{}：{e}", source_label(prefix));
                        if attempt < MAX_TRY {
                            tokio::time::sleep(std::time::Duration::from_millis(600 * attempt as u64))
                                .await;
                        }
                        continue;
                    }
                }
            }

            // 大小校验：截断是最常见的失败形式，先于 sha256 拦截
            let actual_size = std::fs::metadata(&dl_path).map(|m| m.len()).unwrap_or(0);
            if entry.size > 0 && actual_size != entry.size {
                // 比期望小 = 还没下完，保留分片下一轮续传；比期望大 = 数据是脏的，删掉重来
                if actual_size > entry.size {
                    let _ = std::fs::remove_file(&dl_path);
                }
                last_err = format!(
                    "{}：{file_name} 大小不符（期望 {} 字节，实际 {actual_size} 字节）",
                    source_label(prefix),
                    entry.size
                );
                if attempt < MAX_TRY {
                    tokio::time::sleep(std::time::Duration::from_millis(600 * attempt as u64)).await;
                }
                continue;
            }

            // sha256 校验
            let actual = sha256_file(&dl_path)?;
            if !actual.eq_ignore_ascii_case(&entry.sha256) {
                // 内容不对（可能是镜像返回的错误页），续传没有意义，删掉整包重下
                let _ = std::fs::remove_file(&dl_path);
                last_err = format!(
                    "{}：{file_name} 校验失败（期望 sha256 {}，实际 {actual}）",
                    source_label(prefix),
                    entry.sha256
                );
                if attempt < MAX_TRY {
                    tokio::time::sleep(std::time::Duration::from_millis(600 * attempt as u64)).await;
                }
                continue;
            }

            return Ok(dl_path);
        }
        // 换下一个源之前稍作停顿，避免连续打同一个 CDN
        if si + 1 < prefixes.len() {
            tokio::time::sleep(std::time::Duration::from_millis(400)).await;
        }
    }

    // 保留 .tmp 里的分片：用户重新点「下载」时可以从断点续传，不必从 0 开始
    Err(format!(
        "Java {major} 组件 {file_name} 下载失败（已尝试 {} 个源）：{last_err}",
        prefixes.len()
    ))
}

/// 下载并安装 Android JRE
///
/// 流程：拉清单 → 按设备 ABI 选架构包 → 下载 universal + bin 归档 → 逐个 sha256 校验
/// → 叠加解压到 `java/jre{major}-{arch}/`。
pub async fn download_java(app: AppHandle, major: u32) -> Result<JavaInfo, String> {
    if !SUPPORTED_MAJORS.contains(&major) {
        return Err(format!(
            "Android 端暂不支持 Java {major}，可用版本：{}",
            SUPPORTED_MAJORS
                .iter()
                .map(|m| m.to_string())
                .collect::<Vec<_>>()
                .join(" / ")
        ));
    }

    let client = downloader::http_client();
    let (manifest, prefixes) = pick_manifest(&client, &app, major).await?;
    if manifest.java_version != major {
        return Err(format!(
            "JRE 清单版本不符：请求 Java {major}，返回 {}",
            manifest.java_version
        ));
    }

    let want_arch = device_arch();
    let arch = manifest
        .architectures
        .iter()
        .find(|a| a.arch == want_arch)
        .ok_or_else(|| {
            format!(
                "当前设备架构 {want_arch} 没有对应的 Android JRE（可用：{}）",
                manifest
                    .architectures
                    .iter()
                    .map(|a| a.arch.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })?;

    // 目标目录：java/jre{major}-{arch}
    let dest = settings::java_dir(&app).join(format!("jre{major}-{}", arch.arch));
    let _ = std::fs::remove_dir_all(&dest);
    std::fs::create_dir_all(&dest).map_err(|e| format!("创建目录失败: {e}"))?;

    let tmp = settings::java_dir(&app).join(".tmp");
    let _ = std::fs::create_dir_all(&tmp);

    // 先解 universal，再解 bin-<arch>（后者覆盖同名的 lib/ 条目）
    let arch_asset = arch.as_asset();
    let mut installed = 0usize;
    for (idx, entry) in [&manifest.universal, &arch_asset].into_iter().enumerate() {
        let dl_path =
            download_verified(&client, &app, major, idx, entry, &tmp, &prefixes).await?;

        let (files, links) = extract_tar_xz(&dl_path, &dest)?;
        installed += files;
        let _ = links;
        let _ = std::fs::remove_file(&dl_path);
    }
    let _ = installed;

    // 校验产物完整性：JRE 必须有 bin/java 与 libjvm.so
    let java_bin = dest.join("bin").join("java");
    // Java 17+ 布局：lib/server/libjvm.so；Java 8 布局：lib/<arch>/server/libjvm.so
    let libjvm = dest.join("lib").join("server").join("libjvm.so");
    let libjvm8 = dest
        .join("lib")
        .join(device_arch())
        .join("server")
        .join("libjvm.so");
    if !java_bin.exists() {
        return Err("JRE 解压后未找到 bin/java".to_string());
    }
    if !libjvm.exists() && !libjvm8.exists() {
        return Err("JRE 解压后未找到 libjvm.so（架构包可能不完整）".to_string());
    }

    // Android 上 JRE 的 bin/ 启动器与 lib/*.so 需要可执行权限
    set_exec_recursive(&dest)?;

    // Java 8 的 universal 里是 .pack（如 rt.jar.pack），需要随包自带的 unpack200 解包成 .jar。
    // Java 17/21/25 没有 .pack，此步为空操作。这与 FCL 的 patchJava/unpack 行为一致。
    unpack_packs(&dest)?;

    // 取版本号
    let version = std::fs::read_to_string(dest.join("release"))
        .ok()
        .and_then(|t| {
            t.lines()
                .find_map(|l| l.strip_prefix("JAVA_VERSION=\"")?.strip_suffix('"'))
                .map(|s| s.to_string())
        })
        .unwrap_or_else(|| format!("{major}.0.0+0"));
    // 以 release 里的真实版本号反推主版本，避免与请求值不一致
    let real_major = parse_major(&version).unwrap_or(major);

    let _ = app.emit("java-installed", &version);
    Ok(JavaInfo {
        path: java_bin.to_string_lossy().to_string(),
        major: real_major,
        version,
        vendor: format!(
            "Eclipse JRE for Android ({} · build {})",
            arch.arch,
            manifest.build_id.chars().take(8).collect::<String>()
        ),
        managed: true,
    })
}

/// 标记文件可执行（Android 上 JRE 的 bin/ 脚本需要）
fn set_exec(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = std::fs::metadata(path) {
            let mut perm = meta.permissions();
            perm.set_mode(perm.mode() | 0o755);
            let _ = std::fs::set_permissions(path, perm);
        }
    }
    #[cfg(not(unix))]
    let _ = path;
}

/// 递归给 JRE 目录下所有 `bin/*` 与 `*.so` 加可执行位。
///
/// Android 上 `bin/java` 等启动器必须是可执行文件，native 库 `*.so` 也建议带 exec；
/// 整树遍历一次即可，符号链接不处理。
fn set_exec_recursive(dest: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let bin_dir = dest.join("bin");
        fn walk(dir: &Path, bin_dir: &Path) {
            if let Ok(entries) = std::fs::read_dir(dir) {
                for e in entries.flatten() {
                    let p = e.path();
                    if let Ok(file_type) = e.file_type() {
                        if file_type.is_dir() {
                            walk(&p, bin_dir);
                            continue;
                        }
                    }
                    let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    if p.starts_with(bin_dir) || name.ends_with(".so") {
                        if let Ok(meta) = std::fs::metadata(&p) {
                            let mut perm = meta.permissions();
                            perm.set_mode(perm.mode() | 0o755);
                            let _ = std::fs::set_permissions(&p, perm);
                        }
                    }
                }
            }
        }
        walk(dest, &bin_dir);
    }
    #[cfg(not(unix))]
    let _ = dest;
    Ok(())
}

/// 把 Java 8 的 `.pack` / `.pack.gz` 用随包自带的 `bin/unpack200` 解包成 `.jar`。
///
/// EclipseJRE 的 Java 8 universal 里是 pack200 压缩的 jar（如 `lib/rt.jar.pack`），
/// 必须解包后才是完整 JRE；Java 17/21/25 没有 `.pack`，此函数直接返回。
/// 这与 FCL 的 `Pack200Utils.unpack` 行为一致——直接调用 JRE 自带的 unpack200 二进制。
fn unpack_packs(dest: &Path) -> Result<(), String> {
    let unpack200 = dest.join("bin").join("unpack200");
    if !unpack200.exists() {
        return Ok(());
    }
    // 收集所有 .pack / .pack.gz
    let mut packs: Vec<PathBuf> = vec![];
    collect_packs(dest, dest, &mut packs)?;
    if packs.is_empty() {
        return Ok(());
    }

    #[cfg(unix)]
    set_exec(&unpack200);

    for pack in packs {
        let file_name = match pack.file_name().and_then(|n| n.to_str()) {
            Some(n) if n.ends_with(".pack.gz") => {
                n.trim_end_matches(".pack.gz").to_string() + ".jar"
            }
            Some(n) if n.ends_with(".pack") => n.trim_end_matches(".pack").to_string() + ".jar",
            _ => continue,
        };
        let out = pack.with_file_name(file_name);
        // -r：解包成功后自动删除源 .pack
        let status = std::process::Command::new(&unpack200)
            .arg("-r")
            .arg(&pack)
            .arg(&out)
            .status()
            .map_err(|e| format!("执行 unpack200 失败: {e}"))?;
        if !status.success() {
            return Err(format!(
                "unpack200 解包 {} 失败（退出码 {:?}）",
                pack.display(),
                status.code()
            ));
        }
    }
    Ok(())
}

/// 递归收集目录下的 `.pack` / `.pack.gz` 文件
fn collect_packs(root: &Path, dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    for entry in std::fs::read_dir(dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let p = entry.path();
        let ftype = entry.file_type().map_err(|e| e.to_string())?;
        if ftype.is_dir() {
            if p != *root {
                collect_packs(root, &p, out)?;
            }
        } else if let Some(name) = p.file_name().and_then(|n| n.to_str()) {
            if name.ends_with(".pack") || name.ends_with(".pack.gz") {
                out.push(p);
            }
        }
    }
    Ok(())
}

/// 按需求选择最合适的 Android JRE
pub fn auto_select_java(app: &AppHandle, mc_version: &str) -> Result<JavaInfo, String> {
    select_java(
        &scan_javas(app.clone()),
        required_java_major(mc_version),
        mc_version,
    )
}

/// 解析启动用的 Java 路径：设置里手动指定 > 自动选择
pub fn resolve_java(app: AppHandle, mc_version: String) -> Result<JavaInfo, String> {
    let s = settings::load(&app);
    if !s.java_path.is_empty() && Path::new(&s.java_path).exists() {
        if let Some(info) = probe_jre(Path::new(&s.java_path).parent().unwrap_or(Path::new("/"))) {
            return Ok(info);
        }
    }
    auto_select_java(&app, &mc_version)
}

