//! 云控更新：从 Cloudflare Worker (/config) 获取更新信息并安装
//! Worker 域名: cclconfig.caiwenjun.top

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use tauri::{AppHandle, Emitter};
use tauri_plugin_opener::OpenerExt;

use crate::downloader;

pub const CLOUD_CONFIG_URL: &str = "https://cclconfig.caiwenjun.top/config";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudConfig {
    #[serde(default)]
    pub latest_version: String,
    #[serde(default)]
    pub min_version: String,
    #[serde(default)]
    pub download_url: String,
    #[serde(default)]
    pub sha256: String,
    #[serde(default)]
    pub release_notes: String,
    #[serde(default)]
    pub force_update: bool,
    #[serde(default)]
    pub announcement: String,
    #[serde(default)]
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct UpdateInfo {
    /// 当前启动器版本
    pub current_version: String,
    pub latest_version: String,
    /// 是否有新版本
    pub has_update: bool,
    /// 是否必须更新（低于 min_version 且 force_update）
    pub must_update: bool,
    pub download_url: String,
    pub sha256: String,
    pub release_notes: String,
    pub announcement: String,
    /// 云控配置更新时间（用于公告「已读」去重）
    #[serde(default)]
    pub updated_at: String,
    /// 云控是否可达（离线时静默跳过，不阻塞使用）
    pub cloud_online: bool,
}

/// 语义化版本比较: -1 a<b, 0 相等, 1 a>b（忽略预发布后缀差异）
pub fn compare_versions(a: &str, b: &str) -> i32 {
    let parse = |v: &str| -> Vec<u32> {
        let core = v.split(['-', '+']).next().unwrap_or(v);
        core.split('.')
            .map(|s| s.trim().parse::<u32>().unwrap_or(0))
            .collect()
    };
    let va = parse(a);
    let vb = parse(b);
    let len = va.len().max(vb.len());
    for i in 0..len {
        let x = va.get(i).copied().unwrap_or(0);
        let y = vb.get(i).copied().unwrap_or(0);
        if x < y {
            return -1;
        }
        if x > y {
            return 1;
        }
    }
    0
}

#[tauri::command]
pub fn app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// 检查云控更新（任何网络错误都返回 has_update=false，不阻塞启动）
#[tauri::command]
pub async fn check_cloud_update() -> Result<UpdateInfo, String> {
    let current = env!("CARGO_PKG_VERSION").to_string();
    let client = downloader::http_client();
    let resp = client
        .get(CLOUD_CONFIG_URL)
        .timeout(std::time::Duration::from_secs(8))
        .send()
        .await;

    let resp = match resp {
        Ok(r) if r.status().is_success() => r,
        _ => {
            return Ok(UpdateInfo {
                current_version: current.clone(),
                latest_version: current.clone(),
                has_update: false,
                must_update: false,
                download_url: String::new(),
                sha256: String::new(),
                release_notes: String::new(),
                announcement: String::new(),
                updated_at: String::new(),
                cloud_online: false,
            });
        }
    };

    let cfg: CloudConfig = match resp.json().await {
        Ok(c) => c,
        Err(_) => {
            return Ok(UpdateInfo {
                current_version: current.clone(),
                latest_version: current,
                has_update: false,
                must_update: false,
                download_url: String::new(),
                sha256: String::new(),
                release_notes: String::new(),
                announcement: String::new(),
                updated_at: String::new(),
                cloud_online: false,
            });
        }
    };

    let latest = if cfg.latest_version.is_empty() {
        current.clone()
    } else {
        cfg.latest_version.clone()
    };
    let has_update = compare_versions(&current, &latest) < 0;
    let must_update = cfg.force_update
        && !cfg.min_version.is_empty()
        && compare_versions(&current, &cfg.min_version) < 0;

    Ok(UpdateInfo {
        current_version: current,
        latest_version: latest,
        has_update,
        must_update,
        download_url: cfg.download_url,
        sha256: cfg.sha256,
        release_notes: cfg.release_notes,
        announcement: cfg.announcement,
        updated_at: cfg.updated_at,
        cloud_online: true,
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct UpdateDownloadProgress {
    pub loaded: u64,
    pub total: u64,
}

/// 下载并安装更新：下载安装包 → SHA256 校验 → Windows 静默安装 → 退出当前程序
#[tauri::command]
pub async fn install_cloud_update(app: AppHandle, download_url: String, sha256: String) -> Result<(), String> {
    if download_url.trim().is_empty() {
        return Err("云控未配置下载地址".into());
    }
    let current = env!("CARGO_PKG_VERSION").to_string();
    let client = downloader::http_client();

    // 临时目录下载
    let file_name = download_url
        .rsplit('/')
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or("CCL-update.exe")
        .to_string();
    let dest: PathBuf = std::env::temp_dir().join(format!("CCL-update-{file_name}"));

    // 手动下载以推送进度事件
    let resp = client
        .get(&download_url)
        .send()
        .await
        .map_err(|e| format!("下载更新失败: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("下载更新失败: HTTP {}", resp.status()));
    }
    let total = resp.content_length().unwrap_or(0);
    let mut loaded: u64 = 0;
    let mut stream = resp.bytes_stream();
    use futures_util::StreamExt;
    use tokio::io::AsyncWriteExt;
    let mut file = tokio::fs::File::create(&dest)
        .await
        .map_err(|e| format!("创建临时文件失败: {e}"))?;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("下载中断: {e}"))?;
        file.write_all(&chunk).await.map_err(|e| e.to_string())?;
        loaded += chunk.len() as u64;
        let _ = app.emit("update-download", UpdateDownloadProgress { loaded, total });
    }
    file.flush().await.map_err(|e| e.to_string())?;

    // SHA256 校验
    if !sha256.trim().is_empty() {
        let data = tokio::fs::read(&dest)
            .await
            .map_err(|e| format!("读取安装包失败: {e}"))?;
        let actual = format!("{:x}", Sha256::digest(&data));
        if actual.to_lowercase() != sha256.trim().to_lowercase() {
            let _ = tokio::fs::remove_file(&dest).await;
            return Err(format!("SHA256 校验失败：期望 {sha256}，实际 {actual}"));
        }
    }

    let ext = dest
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();

    if ext == "exe" || ext == "msi" {
        // NSIS 静默安装参数 /S（Tauri 默认 NSIS 安装包支持）
        let mut cmd = std::process::Command::new(&dest);
        if ext == "exe" {
            cmd.arg("/S");
        } else {
            cmd.arg("/qn");
        }
        cmd.spawn()
            .map_err(|e| format!("启动安装程序失败: {e}"))?;
        // 退出启动器让安装程序接管
        app.exit(0);
        Ok(())
    } else {
        // zip 便携包：打开所在目录让用户手动替换
        app.opener()
            .open_path(
                dest.parent().unwrap_or(&dest).to_string_lossy().to_string(),
                None::<String>,
            )
            .map_err(|e| e.to_string())?;
        Err("已下载更新包（非安装器格式），请手动解压替换程序文件".into())
    }
}
