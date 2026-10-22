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

#[derive(Debug, Deserialize)]
struct AdoptiumAsset {
    binary: AdoptiumBinary,
    #[allow(dead_code)]
    release: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct AdoptiumBinary {
    package: AdoptiumPackage,
}

#[derive(Debug, Deserialize)]
struct AdoptiumPackage {
    link: String,
    #[serde(default)]
    checksum: String,
}

/// 从 Adoptium API 下载指定主版本的 JDK（自动选最新 patch）
#[tauri::command]
pub async fn download_java(app: AppHandle, major: u32) -> Result<JavaInfo, String> {
    let client = downloader::http_client();
    let url = format!(
        "https://api.adoptium.net/v3/assets/latest/{major}/hotspot?architecture=x64&image_type=jdk&os=windows&vendor=eclipse"
    );
    let assets: Vec<AdoptiumAsset> = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("请求 Adoptium 失败: {e}"))?
        .json()
        .await
        .map_err(|e| format!("解析 Adoptium 响应失败: {e}"))?;

    let asset = assets
        .first()
        .ok_or_else(|| "Adoptium 未返回可用 JDK".to_string())?;
    let link = asset.binary.package.link.clone();

    let semver = asset
        .release
        .get("version")
        .and_then(|v| v.get("semver"))
        .and_then(|v| v.as_str())
        .unwrap_or(&format!("{major}.0.0+0"))
        .to_string();

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
