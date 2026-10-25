//! Java 运行时管理：扫描本机 JDK、从 Adoptium 下载托管 JDK、按 MC 版本自动选择

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;
use tauri::{AppHandle, Emitter};
use zip::ZipArchive;

use crate::downloader::{self, DlTask};
use crate::settings;

#[derive(Debug, Clone, Serialize)]
pub struct JavaInfo {
    pub path: String,
    /// 主版本号, 如 8 / 17 / 21
    pub major: u32,
    /// 完整版本字符串, 如 "21.0.5+11"
    pub version: String,
    pub vendor: String,
    /// 是否为启动器下载托管的
    pub managed: bool,
}

/// 常见 JDK 安装目录
fn scan_dirs() -> Vec<PathBuf> {
    let mut dirs = vec![];
    let mut push = |p: PathBuf| {
        if p.exists() {
            dirs.push(p);
        }
    };
    push(PathBuf::from("F:\\jdk-25.0.2"));
    push(PathBuf::from("F:\\jdk-25.0.2\\jdk"));
    for base in [
        "C:\\Program Files\\Java",
        "C:\\Program Files\\Eclipse Adoptium",
        "C:\\Program Files\\Microsoft",
        "C:\\Program Files\\Zulu",
        "C:\\Program Files\\Amazon Corretto",
        "C:\\Program Files\\BellSoft",
        "C:\\Program Files\\ojdkbuild",
    ] {
        push(PathBuf::from(base));
    }
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        push(PathBuf::from(&local).join("Programs\\Eclipse Adoptium"));
    }
    if let Ok(profile) = std::env::var("USERPROFILE") {
        push(PathBuf::from(&profile).join(".jdks"));
        push(PathBuf::from(&profile).join("scoop\\apps\\temurin\\current"));
        push(PathBuf::from(&profile).join("scoop\\apps\\openjdk\\current"));
    }
    dirs
}

fn parse_java_version(output: &str) -> Option<(u32, String, String)> {
    // openjdk version "21.0.5" 2024-...  /  java version "1.8.0_431"
    let line = output.lines().find(|l| l.contains("version"))?;
    let start = line.find('"')?;
    let rest = &line[start + 1..];
    let end = rest.find('"')?;
    let ver = rest[..end].to_string();
    let major = parse_major(&ver)?;
    let vendor = if line.contains("OpenJDK") || line.contains("openjdk") {
        "Eclipse Temurin/OpenJDK"
    } else if line.contains("Zulu") {
        "Azul Zulu"
    } else if line.contains("Microsoft") {
        "Microsoft"
    } else if line.contains("Corretto") {
        "Amazon Corretto"
    } else {
        "Oracle/Other"
    };
    Some((major, ver, vendor.to_string()))
}

fn parse_major(ver: &str) -> Option<u32> {
    let ver = ver.trim_start_matches("1.").trim();
    let first = ver.split(['.', '+', '-', '_']).next()?;
    first.parse::<u32>().ok()
}

fn probe_java(java_exe: &Path) -> Option<JavaInfo> {
    let out = Command::new(java_exe)
        .arg("-version")
        .output()
        .ok()?;
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let (major, version, vendor) = parse_java_version(&text)?;
    Some(JavaInfo {
        path: java_exe.to_string_lossy().to_string(),
        major,
        version,
        vendor,
        managed: false,
    })
}

/// 扫描本机已安装的 JDK（去重，按主版本排序）
#[tauri::command]
pub fn scan_javas(app: AppHandle) -> Vec<JavaInfo> {
    let mut result: Vec<JavaInfo> = vec![];

    // 1. 托管目录（启动器自己下载的）
    let managed_root = settings::java_dir(&app);
    if managed_root.exists() {
        if let Ok(entries) = std::fs::read_dir(&managed_root) {
            for entry in entries.flatten() {
                let exe = entry.path().join("bin\\java.exe");
                if exe.exists() {
                    if let Some(mut info) = probe_java(&exe) {
                        info.managed = true;
                        result.push(info);
                    }
                }
            }
        }
    }

    // 2. 常见安装目录
    for dir in scan_dirs() {
        let direct = dir.join("bin\\java.exe");
        if direct.exists() {
            if let Some(info) = probe_java(&direct) {
                result.push(info);
            }
        } else if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let exe = entry.path().join("bin\\java.exe");
                if exe.exists() {
                    if let Some(info) = probe_java(&exe) {
                        result.push(info);
                    }
                }
            }
        }
    }

    // 3. PATH 中的 java
    if let Ok(path_var) = std::env::var("PATH") {
        for dir in path_var.split(';') {
            let exe = Path::new(dir).join("java.exe");
            if exe.exists() {
                if let Some(info) = probe_java(&exe) {
                    result.push(info);
                }
            }
        }
    }

    // 去重（按 java.exe 路径）
    result.sort_by(|a, b| a.path.cmp(&b.path));
    result.dedup_by(|a, b| a.path == b.path);
    result.sort_by(|a, b| a.major.cmp(&b.major));
    result
}

/// MC 版本 → 需要的 Java 主版本
pub fn required_java_major(mc_version: &str) -> u32 {
    let v = parse_mc_version(mc_version);
    match v {
        (1, minor, patch) if minor >= 21 || (minor == 20 && patch >= 5) => 21,
        (1, minor, _) if minor >= 18 => 17,
        (1, 17, _) => 16,
        _ => 8,
    }
}

/// "1.21.11" → (1, 21, 11)
pub fn parse_mc_version(v: &str) -> (u32, u32, u32) {
    let nums: Vec<u32> = v
        .split('.')
        .map(|s| s.parse::<u32>().unwrap_or(0))
        .collect();
    (
        nums.first().copied().unwrap_or(1),
        nums.get(1).copied().unwrap_or(0),
        nums.get(2).copied().unwrap_or(0),
    )
}

/// 按需求选择最合适的本机 Java（满足主版本 ≥ 需求的最小者）
pub fn auto_select_java(app: &AppHandle, mc_version: &str) -> Result<JavaInfo, String> {
    let need = required_java_major(mc_version);
    let javas = scan_javas(app.clone());
    let mut candidates: Vec<&JavaInfo> = javas.iter().filter(|j| j.major >= need).collect();
    candidates.sort_by(|a, b| a.major.cmp(&b.major));
    match candidates.first() {
        Some(j) => Ok((*j).clone()),
        None => Err(format!(
            "未找到 Java {need} 或更高版本（MC {mc_version} 需要），请到 Java 管理页下载"
        )),
    }
}

/// Adoptium `/v3/assets/latest/...` 返回的单个资产。
///
/// 注意：真实响应里**没有 `release` 字段**，版本信息在顶层 `version` 对象里
/// （`version.semver` / `version.openjdk_version`）。之前把 `release` 声明成必填
/// 导致 serde 反序列化必然失败（`error decoding response body`），Java 下载完全不可用。
/// 所有可选字段一律 `#[serde(default)]`，避免上游字段增删就整个接口挂掉。
#[derive(Debug, Deserialize)]
struct AdoptiumAsset {
    binary: AdoptiumBinary,
    #[serde(default)]
    version: AdoptiumVersion,
    /// 例如 `jdk-17.0.20.1+1`，作为版本号的可靠兜底
    #[serde(default)]
    release_name: String,
}

#[derive(Debug, Default, Deserialize)]
struct AdoptiumVersion {
    #[serde(default)]
    semver: String,
    #[serde(default)]
    openjdk_version: String,
}

#[derive(Debug, Deserialize)]
struct AdoptiumBinary {
    package: AdoptiumPackage,
}

#[derive(Debug, Deserialize)]
struct AdoptiumPackage {
    link: String,
    /// 某些构建没有签名校验值，Adoptium 会给 `null`
    #[serde(default)]
    checksum: Option<String>,
}

/// 带重试的 Adoptium 元数据请求。
///
/// 实测 api.adoptium.net 直连存在间歇性握手失败（同一 URL 连续请求约有 1/4 概率
/// 报 `error sending request`），单次请求会让用户看到「下载失败」。这里最多重试 3 次。
async fn fetch_adoptium_assets(
    client: &reqwest::Client,
    url: &str,
) -> Result<Vec<AdoptiumAsset>, String> {
    const MAX_TRY: u32 = 3;
    let mut last_err = String::new();
    for attempt in 1..=MAX_TRY {
        match client.get(url).send().await {
            Err(e) => {
                last_err = format!("请求 Adoptium 失败: {e}");
            }
            Ok(resp) => {
                let status = resp.status();
                let body = resp
                    .text()
                    .await
                    .map_err(|e| format!("读取 Adoptium 响应失败: {e}"))?;
                if !status.is_success() {
                    last_err = format!(
                        "Adoptium 返回 HTTP {status}：{}",
                        body.chars().take(200).collect::<String>()
                    );
                    // 4xx 是请求本身有问题（版本号非法等），重试没意义
                    if status.is_client_error() {
                        return Err(last_err);
                    }
                } else {
                    return serde_json::from_str(&body).map_err(|e| {
                        format!(
                            "解析 Adoptium 响应失败: {e}（响应开头：{}）",
                            body.chars().take(200).collect::<String>()
                        )
                    });
                }
            }
        }
        if attempt < MAX_TRY {
            tokio::time::sleep(std::time::Duration::from_millis(600 * attempt as u64)).await;
        }
    }
    Err(format!("{last_err}（已重试 {MAX_TRY} 次）"))
}

/// 从 Adoptium API 下载指定主版本的 JDK（自动选最新 patch）
#[tauri::command]
pub async fn download_java(app: AppHandle, major: u32) -> Result<JavaInfo, String> {
    let client = downloader::http_client();
    let url = format!(
        "https://api.adoptium.net/v3/assets/latest/{major}/hotspot?architecture=x64&image_type=jdk&os=windows&vendor=eclipse"
    );
    let assets = fetch_adoptium_assets(&client, &url).await?;

    let asset = assets
        .first()
        .ok_or_else(|| format!("Adoptium 未返回 Java {major} 的可用 JDK"))?;
    let link = asset.binary.package.link.clone();
    if link.is_empty() {
        return Err("Adoptium 返回的下载地址为空".to_string());
    }

    // 版本号：version.semver → version.openjdk_version → release_name → 兜底
    let semver = if !asset.version.semver.is_empty() {
        asset.version.semver.clone()
    } else if !asset.version.openjdk_version.is_empty() {
        asset.version.openjdk_version.clone()
    } else if !asset.release_name.is_empty() {
        asset.release_name.clone()
    } else {
        format!("{major}.0.0+0")
    };

    let zip_path = settings::java_dir(&app).join(format!("{semver}.zip"));
    let task = DlTask::new(
        &format!("java-{major}"),
        &format!("Java {major} ({semver})"),
        &link,
        &zip_path,
    );
    downloader::download_one(&client, &app, &task).await?;

    // 解压
    let dest = settings::java_dir(&app).join(&semver);
    let _ = std::fs::remove_dir_all(&dest);
    std::fs::create_dir_all(&dest).map_err(|e| format!("创建目录失败: {e}"))?;
    let file = std::fs::File::open(&zip_path).map_err(|e| e.to_string())?;
    let mut archive = ZipArchive::new(file).map_err(|e| format!("打开 zip 失败: {e}"))?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let name = entry.name().to_string();
        // zip 内顶层是 jdk-xxx/，解压时剥掉
        let rel = match name.split_once('/') {
            Some((_, rest)) if !rest.is_empty() => rest.to_string(),
            _ => continue,
        };
        let out_path = dest.join(rel.replace('\\', "/"));
        if entry.is_dir() {
            std::fs::create_dir_all(&out_path).map_err(|e| e.to_string())?;
        } else {
            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let mut out = std::fs::File::create(&out_path).map_err(|e| e.to_string())?;
            std::io::copy(&mut entry, &mut out).map_err(|e| e.to_string())?;
        }
    }
    let _ = std::fs::remove_file(&zip_path);

    let exe = dest.join("bin\\java.exe");
    if !exe.exists() {
        return Err("JDK 解压后未找到 bin\\java.exe".to_string());
    }
    let _ = app.emit("java-installed", &semver);
    Ok(JavaInfo {
        path: exe.to_string_lossy().to_string(),
        major,
        version: semver.clone(),
        vendor: "Eclipse Temurin (托管)".to_string(),
        managed: true,
    })
}

/// 解析启动用的 Java 路径：设置里手动指定 > 自动选择
#[tauri::command]
pub fn resolve_java(app: AppHandle, mc_version: String) -> Result<JavaInfo, String> {
    let s = settings::load(&app);
    if !s.java_path.is_empty() && Path::new(&s.java_path).exists() {
        if let Some(info) = probe_java(Path::new(&s.java_path)) {
            return Ok(info);
        }
    }
    auto_select_java(&app, &mc_version)
}
