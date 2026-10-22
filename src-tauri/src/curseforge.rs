//! CurseForge API 客户端：搜索 / 项目 / 文件 / 下载安装
//! 需要 API Key（在设置页填写，来自 https://console.curseforge.com/）

use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use crate::downloader::{self, DlTask};
use crate::instances;
use crate::settings;

const API: &str = "https://api.curseforge.com/v1";
const GAME_ID_MINECRAFT: u32 = 432;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CfModHit {
    pub id: u64,
    pub name: String,
    pub summary: String,
    pub icon_url: Option<String>,
    pub download_count: u64,
    pub date_modified: String,
}

#[derive(Debug, Deserialize)]
struct CfSearchResponse {
    data: Vec<CfMod>,
}

#[derive(Debug, Deserialize)]
struct CfMod {
    id: u64,
    name: String,
    #[serde(default)]
    summary: String,
    logo: Option<CfLogo>,
    #[serde(default)]
    downloadCount: u64,
    #[serde(default)]
    dateModified: String,
}

#[derive(Debug, Deserialize)]
struct CfLogo {
    url: String,
}

fn api_key(app: &AppHandle) -> Result<String, String> {
    let key = settings::load(app).curseforge_api_key;
    if key.trim().is_empty() {
        return Err("未配置 CurseForge API Key，请到设置页填写".into());
    }
    Ok(key)
}

#[tauri::command]
pub async fn cf_search(
    app: AppHandle,
    query: String,
    game_version: Option<String>,
    offset: u32,
) -> Result<Vec<CfModHit>, String> {
    let key = api_key(&app)?;
    let client = downloader::http_client();
    let mut url = format!(
        "{API}/mods/search?gameId={GAME_ID_MINECRAFT}&pageSize=20&index={offset}&searchFilter={}",
        urlencoding(&query)
    );
    if let Some(gv) = &game_version {
        url.push_str(&format!("&gameVersion={gv}"));
    }
    let resp: CfSearchResponse = client
        .get(&url)
        .header("x-api-key", &key)
        .send()
        .await
        .map_err(|e| format!("CurseForge 搜索失败: {e}"))?
        .json()
        .await
        .map_err(|e| format!("解析失败: {e}"))?;
    Ok(resp
        .data
        .into_iter()
        .map(|m| CfModHit {
            id: m.id,
            name: m.name,
            summary: m.summary,
            icon_url: m.logo.map(|l| l.url),
            download_count: m.downloadCount,
            date_modified: m.dateModified,
        })
        .collect())
}

fn urlencoding(s: &str) -> String {
    percent_encoding::utf8_percent_encode(s, percent_encoding::NON_ALPHANUMERIC).to_string()
}

#[derive(Debug, Deserialize)]
struct CfFile {
    id: u64,
    #[serde(default)]
    fileName: String,
    #[serde(default)]
    downloadUrl: String,
    #[serde(default)]
    gameVersions: Vec<String>,
    #[serde(default)]
    fileDate: String,
    #[serde(default)]
    fileLength: u64,
}

#[derive(Debug, Deserialize)]
struct CfFilesResponse {
    data: Vec<CfFile>,
}

/// 获取模组的文件列表（可按游戏版本过滤）
#[tauri::command]
pub async fn cf_files(
    app: AppHandle,
    mod_id: u64,
    game_version: Option<String>,
) -> Result<Vec<serde_json::Value>, String> {
    let key = api_key(&app)?;
    let client = downloader::http_client();
    let mut url = format!("{API}/mods/{mod_id}/files?pageSize=50");
    if let Some(gv) = &game_version {
        url.push_str(&format!("&gameVersion={gv}"));
    }
    let resp: CfFilesResponse = client
        .get(&url)
        .header("x-api-key", &key)
        .send()
        .await
        .map_err(|e| format!("获取文件列表失败: {e}"))?
        .json()
        .await
        .map_err(|e| format!("解析失败: {e}"))?;
    Ok(resp
        .data
        .into_iter()
        .map(|f| {
            serde_json::json!({
                "id": f.id,
                "fileName": f.fileName,
                "downloadUrl": f.downloadUrl,
                "gameVersions": f.gameVersions,
                "fileDate": f.fileDate,
                "fileLength": f.fileLength,
            })
        })
        .collect())
}

/// 下载安装 CurseForge 模组到指定实例
#[tauri::command]
pub async fn cf_install(
    app: AppHandle,
    instance_id: String,
    download_url: String,
    filename: String,
) -> Result<String, String> {
    let inst = instances::get_instance(&app, &instance_id)?;
    let dest = std::path::PathBuf::from(&inst.game_dir)
        .join("mods")
        .join(&filename);
    let client = downloader::http_client();
    let task = DlTask::new(&format!("cf-mod-{filename}"), &filename, &download_url, &dest);
    downloader::download_one(&client, &app, &task).await?;
    Ok(filename)
}
