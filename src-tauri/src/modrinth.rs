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
    #[serde(default)]
    project_type: String,
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
    follows: u64,
    #[serde(default)]
    author: String,
    #[serde(default)]
    date_modified: String,
    #[serde(default)]
    color: Option<u64>,
}

/// 推荐位用的项目卡片（比 ModHit 多带关注数 / 类型 / 更新时间 / 主题色）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectHit {
    pub project_id: String,
    pub project_type: String,
    pub slug: String,
    pub title: String,
    pub description: String,
    pub icon_url: Option<String>,
    pub categories: Vec<String>,
    pub versions: Vec<String>,
    pub downloads: u64,
    pub follows: u64,
    pub author: String,
    pub date_modified: String,
    pub color: Option<u64>,
}

impl SearchHit {
    fn into_project(self) -> ProjectHit {
        ProjectHit {
            project_id: self.project_id,
            project_type: self.project_type,
            slug: self.slug,
            title: self.title,
            description: self.description,
            icon_url: self.icon_url,
            categories: self.categories,
            versions: self.versions,
            downloads: self.downloads,
            follows: self.follows,
            author: self.author,
            date_modified: self.date_modified,
            color: self.color,
        }
    }
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
    let resp: SearchResponse = get_json(&client, &url, "Modrinth 搜索").await?;
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

/// GET 并反序列化，网络抖动时最多重试 3 次（推荐位不能因为一次超时就空白）
async fn get_json<T: serde::de::DeserializeOwned>(
    client: &reqwest::Client,
    url: &str,
    what: &str,
) -> Result<T, String> {
    let mut last = String::new();
    for attempt in 0..3u32 {
        if attempt > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(500 * attempt as u64)).await;
        }
        match client
            .get(url)
            .timeout(std::time::Duration::from_secs(20))
            .send()
            .await
        {
            Ok(resp) => {
                if !resp.status().is_success() {
                    last = format!("{what}失败: HTTP {}", resp.status().as_u16());
                    continue;
                }
                match resp.json::<T>().await {
                    Ok(v) => return Ok(v),
                    Err(e) => last = format!("{what}解析失败: {e}"),
                }
            }
            Err(e) => last = format!("{what}失败: {e}"),
        }
    }
    Err(last)
}

// ---------------------------------------------------------------- 推荐位

/// 推荐位：按项目类型 + 分类 + 游戏版本 / 加载器过滤，按热度排序。
/// 过滤过窄导致空结果时逐级放宽（分类 → 版本/加载器 → 完全不限），保证永远有内容可展示。
#[tauri::command]
pub async fn modrinth_recommend(
    project_type: String,
    category: Option<String>,
    game_version: Option<String>,
    loader: Option<String>,
    index: Option<String>,
    offset: u32,
    limit: Option<u32>,
) -> Result<Vec<ProjectHit>, String> {
    if project_type != "mod" && project_type != "modpack" {
        return Err(format!("不支持的推荐类型: {project_type}"));
    }
    let idx = match index.as_deref() {
        Some("follows") => "follows",
        Some("newest") => "newest",
        Some("updated") => "updated",
        _ => "downloads",
    };
    let lim = limit.unwrap_or(12).clamp(1, 20);
    let client = downloader::http_client();

    let strict = search_projects(
        &client,
        &project_type,
        category.as_deref(),
        game_version.as_deref(),
        loader.as_deref(),
        idx,
        offset,
        lim,
    )
    .await?;
    if !strict.is_empty() {
        return Ok(strict);
    }

    // 兜底 1：丢掉分类，只看版本 / 加载器
    let relaxed = search_projects(
        &client,
        &project_type,
        None,
        game_version.as_deref(),
        loader.as_deref(),
        idx,
        offset,
        lim,
    )
    .await?;
    if !relaxed.is_empty() {
        return Ok(relaxed);
    }

    // 兜底 2：全站热门
    search_projects(&client, &project_type, None, None, None, idx, offset, lim).await
}

#[allow(clippy::too_many_arguments)]
async fn search_projects(
    client: &reqwest::Client,
    project_type: &str,
    category: Option<&str>,
    game_version: Option<&str>,
    loader: Option<&str>,
    index: &str,
    offset: u32,
    limit: u32,
) -> Result<Vec<ProjectHit>, String> {
    let mut facets: Vec<String> = vec![format!("[\"project_type:{project_type}\"]")];
    if let Some(c) = category.filter(|c| !c.is_empty()) {
        facets.push(format!("[\"categories:{c}\"]"));
    }
    if let Some(g) = game_version.filter(|g| !g.is_empty()) {
        facets.push(format!("[\"versions:{g}\"]"));
    }
    if let Some(l) = loader.filter(|l| !l.is_empty() && *l != "vanilla") {
        facets.push(format!("[\"categories:{l}\"]"));
    }
    let facets_json = format!("[{}]", facets.join(","));
    let url = format!(
        "{API}/search?query=&limit={limit}&offset={offset}&index={index}&facets={}",
        urlencoding_encode(&facets_json)
    );
    let resp: SearchResponse = get_json(client, &url, "Modrinth 推荐请求").await?;
    Ok(resp.hits.into_iter().map(|h| h.into_project()).collect())
}

#[derive(Debug, Deserialize)]
struct ModFile {
    url: String,
    filename: String,
    #[serde(default)]
    hashes: std::collections::HashMap<String, String>,
    #[serde(default)]
    size: u64,
    #[serde(default)]
    primary: bool,
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
    #[serde(default)]
    version_type: String,
    files: Vec<ModFile>,
}

#[derive(Debug, Deserialize)]
struct ProjectDetail {
    #[serde(default)]
    versions: Vec<String>,
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
    let resp: Vec<ModVersion> = get_json(&client, &url, "获取版本列表").await?;
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

/// 在线安装 Modrinth 整合包：拉版本 → 下载 .mrpack 到临时目录 → 转交整合包导入流程
#[tauri::command]
pub async fn modrinth_install_modpack(
    app: AppHandle,
    slug: String,
    version_id: Option<String>,
) -> Result<instances::Instance, String> {
    let client = downloader::http_client();

    let picked: ModVersion = match version_id {
        Some(vid) => get_json(&client, &format!("{API}/version/{vid}"), "获取整合包版本").await?,
        None => {
            // 只拉项目详情（十几 KB）；/project/{slug}/version 动辄 2~3 MB、十几秒
            let detail: ProjectDetail =
                get_json(&client, &format!("{API}/project/{slug}"), "获取整合包信息").await?;
            if detail.versions.is_empty() {
                return Err("该整合包没有可用版本".into());
            }
            // 从最新往前找，优先正式版（最多看 5 个），全为预览版则取最新
            let mut best: Option<ModVersion> = None;
            for id in detail.versions.iter().rev().take(5) {
                let v: ModVersion =
                    get_json(&client, &format!("{API}/version/{id}"), "获取整合包版本").await?;
                if v.version_type == "release" {
                    best = Some(v);
                    break;
                }
                if best.is_none() {
                    best = Some(v);
                }
            }
            best.ok_or_else(|| "没有可安装的整合包版本".to_string())?
        }
    };

    let file = picked
        .files
        .iter()
        .find(|f| f.primary && f.filename.ends_with(".mrpack"))
        .or_else(|| picked.files.iter().find(|f| f.filename.ends_with(".mrpack")))
        .or_else(|| picked.files.first())
        .ok_or_else(|| "该版本没有可下载的文件".to_string())?;

    let dir = std::env::temp_dir().join("ccl-packs");
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建临时目录失败: {e}"))?;
    let dest = dir.join(&file.filename);

    let sha1 = file.hashes.get("sha1").cloned();
    let task =
        DlTask::new(&format!("pack-{}", file.filename), &file.filename, &file.url, &dest).with_sha1(sha1);
    downloader::download_one(&client, &app, &task).await?;

    let result = crate::modpack::import_modpack(app, dest.to_string_lossy().to_string(), None).await;
    let _ = std::fs::remove_file(&dest);
    result
}
