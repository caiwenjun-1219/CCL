//! Modrinth API v2 客户端：搜索 / 项目 / 版本 / 下载安装

use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use crate::downloader::{self, DlTask};
use crate::instances;

const API: &str = "https://api.modrinth.com/v2";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModHit {
    pub project_id: String,
    pub slug: String,
    pub title: String,
    pub description: String,
    pub icon_url: Option<String>,
    pub categories: Vec<String>,
    pub versions: Vec<String>,
    pub downloads: u64,
    pub author: String,
}

#[derive(Debug, Deserialize)]
struct SearchResponse {
    hits: Vec<SearchHit>,
}

#[derive(Debug, Deserialize)]
struct SearchHit {
    project_id: String,
    slug: String,
    title: String,
    description: String,
    icon_url: Option<String>,
    #[serde(default)]
    categories: Vec<String>,
    #[serde(default)]
    versions: Vec<String>,
    #[serde(default)]
    downloads: u64,
    #[serde(default)]
    author: String,
}

#[tauri::command]
pub async fn modrinth_search(
    app: AppHandle,
    query: String,
    game_version: Option<String>,
    loader: Option<String>,
    offset: u32,
) -> Result<Vec<ModHit>, String> {
    let client = downloader::http_client();
    let mut facets: Vec<String> = vec![];
    if let Some(gv) = &game_version {
        facets.push(format!("versions:{gv}"));
    }
    if let Some(l) = &loader {
        facets.push(format!("categories:{l}"));
    }
    let facets_json = if facets.is_empty() {
        "[]".to_string()
    } else {
        let parts: Vec<String> = facets.iter().map(|f| format!("[\"{f}\"]")).collect();
        format!("[{}]", parts.join(","))
    };
    let url = format!(
        "{API}/search?query={}&limit=20&offset={offset}&facets={}",
        urlencoding_encode(&query),
        urlencoding_encode(&facets_json)
    );
    let resp: SearchResponse = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("Modrinth 搜索失败: {e}"))?
        .json()
        .await
        .map_err(|e| format!("解析失败: {e}"))?;
    let _ = &app;
    Ok(resp
        .hits
        .into_iter()
        .map(|h| ModHit {
            project_id: h.project_id,
            slug: h.slug,
            title: h.title,
            description: h.description,
            icon_url: h.icon_url,
            categories: h.categories,
            versions: h.versions,
            downloads: h.downloads,
            author: h.author,
        })
        .collect())
}

fn urlencoding_encode(s: &str) -> String {
    percent_encoding::utf8_percent_encode(s, percent_encoding::NON_ALPHANUMERIC).to_string()
}

#[derive(Debug, Deserialize)]
struct ModFile {
    url: String,
    filename: String,
    #[serde(default)]
    hashes: std::collections::HashMap<String, String>,
    #[serde(default)]
    size: u64,
}

#[derive(Debug, Deserialize)]
struct ModVersion {
    id: String,
    #[serde(default)]
    version_number: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    game_versions: Vec<String>,
    #[serde(default)]
    loaders: Vec<String>,
    files: Vec<ModFile>,
}

#[tauri::command]
pub async fn modrinth_versions(
    slug: String,
    game_version: Option<String>,
    loader: Option<String>,
) -> Result<Vec<serde_json::Value>, String> {
    let client = downloader::http_client();
    let mut params: Vec<String> = vec![];
    if let Some(gv) = &game_version {
        params.push(format!("game_versions=[\"{gv}\"]"));
    }
    if let Some(l) = &loader {
        params.push(format!("loaders=[\"{l}\"]"));
    }
    let url = format!("{API}/project/{slug}/version?{}", params.join("&"));
    let resp: Vec<ModVersion> = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("获取版本列表失败: {e}"))?
        .json()
        .await
        .map_err(|e| format!("解析失败: {e}"))?;
    Ok(resp
        .into_iter()
        .map(|v| {
            serde_json::json!({
                "id": v.id,
                "version_number": v.version_number,
                "name": v.name,
                "game_versions": v.game_versions,
                "loaders": v.loaders,
                "files": v.files.into_iter().map(|f| serde_json::json!({
                    "url": f.url,
                    "filename": f.filename,
                    "sha1": f.hashes.get("sha1").cloned().unwrap_or_default(),
                    "size": f.size,
                })).collect::<Vec<_>>(),
            })
        })
        .collect())
}

/// 下载安装模组到指定实例
#[tauri::command]
pub async fn modrinth_install(
    app: AppHandle,
    instance_id: String,
    download_url: String,
    filename: String,
    sha1: Option<String>,
) -> Result<String, String> {
    let inst = instances::get_instance(&app, &instance_id)?;
    let dest = std::path::PathBuf::from(&inst.game_dir)
        .join("mods")
        .join(&filename);
    let client = downloader::http_client();
    let task = DlTask::new(&format!("mod-{filename}"), &filename, &download_url, &dest).with_sha1(sha1);
    downloader::download_one(&client, &app, &task).await?;
    Ok(filename)
}
