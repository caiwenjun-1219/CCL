//! Minecraft 版本清单读取与安装：vanilla / Fabric / Quilt / Forge / NeoForge
//! 全部走多源回退下载引擎（BMCLAPI 镜像优先，官方源回退）

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter};

use crate::downloader::{self, DlTask};
use crate::java;
use crate::settings;

// ---------------------------------------------------------------- 数据结构

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestEntry {
    pub id: String,
    #[serde(rename = "type")]
    pub vtype: String,
    pub url: String,
}

#[derive(Debug, Deserialize)]
struct VersionManifest {
    latest: Latest,
    versions: Vec<ManifestEntry>,
}

#[derive(Debug, Deserialize)]
struct Latest {
    release: String,
    snapshot: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct VersionJson {
    id: String,
    #[serde(rename = "mainClass", default)]
    main_class: String,
    #[serde(default)]
    arguments: Option<Value>,
    #[serde(rename = "minecraftArguments", default)]
    minecraft_arguments: Option<String>,
    #[serde(default)]
    downloads: HashMap<String, Download>,
    libraries: Vec<Value>,
    #[serde(rename = "assetIndex", default)]
    asset_index: Option<AssetIndexRef>,
    #[serde(rename = "javaVersion", default)]
    java_version: Option<Value>,
    #[serde(rename = "inheritsFrom", default)]
    inherits_from: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    #[serde(rename = "type")]
    vtype: String,
}

#[derive(Debug, Clone, Deserialize)]
struct Download {
    url: String,
    #[serde(default)]
    sha1: String,
    #[serde(default)]
    size: u64,
    #[serde(default)]
    path: String,
}

#[derive(Debug, Clone, Deserialize)]
struct AssetIndexRef {
    id: String,
    #[serde(default)]
    sha1: String,
    #[serde(default)]
    url: String,
}

#[derive(Debug, Deserialize)]
struct AssetIndex {
    objects: HashMap<String, AssetObject>,
}

#[derive(Debug, Deserialize)]
struct AssetObject {
    hash: String,
    #[serde(default)]
    #[allow(dead_code)]
    size: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct InstallProgress {
    pub phase: String,
    pub done: u64,
    pub total: u64,
}

// ---------------------------------------------------------------- 版本清单

fn prefer_mirror(app: &AppHandle) -> bool {
    settings::load(app).download_source != "mojang"
}

#[tauri::command]
pub async fn mc_version_manifest(app: AppHandle) -> Result<Vec<ManifestEntry>, String> {
    let client = downloader::http_client();
    let urls = downloader::urls_with_setting(
        "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json",
        prefer_mirror(&app),
    );
    let mut last_err = String::new();
    for url in urls {
        match client.get(&url).send().await {
            Ok(resp) if resp.status().is_success() => {
                let manifest: VersionManifest =
                    resp.json().await.map_err(|e| format!("解析版本清单失败: {e}"))?;
                let _ = app.emit(
                    "mc-latest",
                    json!({ "release": manifest.latest.release, "snapshot": manifest.latest.snapshot }),
                );
                return Ok(manifest.versions);
            }
            Ok(resp) => last_err = format!("HTTP {}", resp.status()),
            Err(e) => last_err = e.to_string(),
        }
    }
    Err(format!("获取版本清单失败: {last_err}"))
}

/// 游戏目录下的已安装版本
#[tauri::command]
pub fn mc_local_versions(app: AppHandle, game_dir: String) -> Vec<String> {
    let versions_dir = Path::new(&game_dir).join("versions");
    let mut out = vec![];
    if let Ok(entries) = std::fs::read_dir(&versions_dir) {
        for entry in entries.flatten() {
            let json = entry.path().join(format!("{}.json", entry.file_name().to_string_lossy()));
            if json.exists() {
                out.push(entry.file_name().to_string_lossy().to_string());
            }
        }
    }
    out.sort();
    out
}

#[tauri::command]
pub fn mc_delete_version(app: AppHandle, game_dir: String, version_id: String) -> Result<(), String> {
    let dir = Path::new(&game_dir).join("versions").join(&version_id);
    if dir.exists() {
        std::fs::remove_dir_all(&dir).map_err(|e| format!("删除失败: {e}"))?;
    }
    Ok(())
}

// ---------------------------------------------------------------- 安装管线

async fn fetch_version_json(client: &reqwest::Client, url: &str, prefer: bool) -> Result<Value, String> {
    let urls = downloader::urls_with_setting(url, prefer);
    let mut last_err = String::new();
    for u in urls {
        match client.get(&u).send().await {
            Ok(resp) if resp.status().is_success() => {
                return resp.json().await.map_err(|e| format!("解析版本 JSON 失败: {e}"))
            }
            Ok(resp) => last_err = format!("HTTP {}", resp.status()),
            Err(e) => last_err = e.to_string(),
        }
    }
    Err(format!("获取版本 JSON 失败({url}): {last_err}"))
}

/// 安装一个完整版本（json 已合并好）：client jar / libraries / assets / natives
async fn install_version(
    app: &AppHandle,
    game_dir: &Path,
    version_id: &str,
    vjson: &Value,
) -> Result<(), String> {
    let client = downloader::http_client();
    let prefer = prefer_mirror(app);
    let version_dir = game_dir.join("versions").join(version_id);
    std::fs::create_dir_all(&version_dir).map_err(|e| e.to_string())?;

    // 保存版本 json（原样保存，启动时直接用）
    let json_text = serde_json::to_string_pretty(vjson).map_err(|e| e.to_string())?;
    std::fs::write(version_dir.join(format!("{version_id}.json")), json_text)
        .map_err(|e| format!("保存版本 JSON 失败: {e}"))?;

    // ---- client jar
    let v: VersionJson = serde_json::from_value(vjson.clone()).map_err(|e| e.to_string())?;
    if let Some(client_dl) = v.downloads.get("client") {
        let dest = version_dir.join(format!("{version_id}.jar"));
        if !dest.exists() {
            let task = DlTask::new(
                &format!("client-{version_id}"),
                &format!("{version_id} 客户端"),
                &client_dl.url,
                &dest,
            )
            .with_fallbacks(downloader::urls_with_setting(&client_dl.url, prefer))
            .with_sha1(Some(client_dl.sha1.clone()));
            downloader::download_one(&client, app, &task).await?;
        }
    }

    // ---- libraries
    let _ = app.emit(
        "mc-install-progress",
        InstallProgress { phase: "libraries".into(), done: 0, total: v.libraries.len() as u64 },
    );
    let mut lib_tasks: Vec<DlTask> = vec![];
    let mut native_jars: Vec<PathBuf> = vec![];
    for (i, lib) in v.libraries.iter().enumerate() {
        if !library_allowed(lib) {
            continue;
        }
        let downloads = match lib.get("downloads") {
            Some(d) => d,
            None => continue,
        };
        let lib_path = game_dir.join("libraries");

        if let Some(artifact) = downloads.get("artifact") {
            let url = artifact.get("url").and_then(|u| u.as_str()).unwrap_or("");
            if url.is_empty() {
                continue;
            }
            let path = artifact.get("path").and_then(|p| p.as_str()).unwrap_or("");
            let sha1 = artifact.get("sha1").and_then(|s| s.as_str()).map(|s| s.to_string());
            let dest = if path.is_empty() {
                // 用 name 推导 path: group:artifact:version
                lib_path.join(name_to_path(lib.get("name").and_then(|n| n.as_str()).unwrap_or("")))
            } else {
                lib_path.join(path.replace('\\', "/"))
            };
            // 新格式 natives（MC 1.19+）：库名末段为 natives-<平台>，如 org.lwjgl:lwjgl:3.3.3:natives-windows
            let name_suffix = lib.get("name").and_then(|n| n.as_str()).unwrap_or("").split(':').last().unwrap_or("");
            if name_suffix.starts_with("natives-") && name_suffix == current_native_suffix() {
                native_jars.push(dest.clone());
            }
            lib_tasks.push(
                DlTask::new(
                    &format!("lib-{version_id}-{i}"),
                    lib.get("name").and_then(|n| n.as_str()).unwrap_or("library"),
                    url,
                    &dest,
                )
                .with_fallbacks(downloader::urls_with_setting(url, prefer))
                .with_sha1(sha1),
            );
        }

        // natives 分类器（Windows）
        if let Some(classifiers) = downloads.get("classifiers") {
            if let Some(natives) = lib.get("natives").and_then(|n| n.as_object()) {
                let key = natives.get("windows").and_then(|v| v.as_str()).unwrap_or("natives-windows");
                if let Some(nat) = classifiers.get(key) {
                    let url = nat.get("url").and_then(|u| u.as_str()).unwrap_or("");
                    let path = nat.get("path").and_then(|p| p.as_str()).unwrap_or("");
                    if !url.is_empty() && !path.is_empty() {
                        let dest = lib_path.join(path.replace('\\', "/"));
                        native_jars.push(dest.clone());
                        lib_tasks.push(
                            DlTask::new(
                                &format!("natives-{version_id}-{i}"),
                                lib.get("name").and_then(|n| n.as_str()).unwrap_or("natives"),
                                url,
                                &dest,
                            )
                            .with_fallbacks(downloader::urls_with_setting(url, prefer)),
                        );
                    }
                }
            }
        }
    }
    downloader::download_many(&client, app, lib_tasks, 16).await?;
    let _ = app.emit(
        "mc-install-progress",
        InstallProgress { phase: "libraries".into(), done: v.libraries.len() as u64, total: v.libraries.len() as u64 },
    );

    // ---- natives 解压
    let natives_dir = version_dir.join("natives");
    std::fs::create_dir_all(&natives_dir).map_err(|e| e.to_string())?;
    for jar in &native_jars {
        extract_natives(jar, &natives_dir)?;
    }

    // ---- 资源文件
    if let Some(asset_ref) = &v.asset_index {
        let indexes_dir = game_dir.join("assets").join("indexes");
        std::fs::create_dir_all(&indexes_dir).map_err(|e| e.to_string())?;
        let index_path = indexes_dir.join(format!("{}.json", asset_ref.id));
        if !index_path.exists() {
            let task = DlTask::new(
                &format!("asset-index-{version_id}"),
                "资源索引",
                &asset_ref.url,
                &index_path,
            )
            .with_fallbacks(downloader::urls_with_setting(&asset_ref.url, prefer))
            .with_sha1(Some(asset_ref.sha1.clone()).filter(|s| !s.is_empty()));
            downloader::download_one(&client, app, &task).await?;
        }

        let index_text = std::fs::read_to_string(&index_path).map_err(|e| e.to_string())?;
        let index: AssetIndex = serde_json::from_str(&index_text).map_err(|e| e.to_string())?;
        let objects: Vec<(&String, &AssetObject)> = index.objects.iter().collect();
        let _ = app.emit(
            "mc-install-progress",
            InstallProgress { phase: "assets".into(), done: 0, total: objects.len() as u64 },
        );
        let mut asset_tasks: Vec<DlTask> = vec![];
        for (name, obj) in objects {
            let dest = game_dir.join("assets").join("objects").join(&obj.hash[..2]).join(&obj.hash);
            if dest.exists() {
                continue;
            }
            let url = format!("https://resources.download.minecraft.net/{}/{}", &obj.hash[..2], obj.hash);
            asset_tasks.push(
                DlTask::new(&format!("asset-{name}"), name, &url, &dest)
                    .with_fallbacks(downloader::urls_with_setting(&url, prefer))
                    .with_sha1(Some(obj.hash.clone())),
            );
        }
        downloader::download_many(&client, app, asset_tasks, 24).await?;
        let _ = app.emit(
            "mc-install-progress",
            InstallProgress { phase: "assets".into(), done: 0, total: 0 },
        );
    }

    Ok(())
}

/// 当前平台对应的新格式 natives 库名后缀（MC 1.19+）
fn current_native_suffix() -> &'static str {
    if cfg!(target_os = "windows") {
        if cfg!(target_arch = "aarch64") {
            "natives-windows-arm64"
        } else if cfg!(target_pointer_width = "32") {
            "natives-windows-x86"
        } else {
            "natives-windows"
        }
    } else if cfg!(target_os = "macos") {
        if cfg!(target_arch = "aarch64") {
            "natives-macos-arm64"
        } else {
            "natives-macos"
        }
    } else if cfg!(target_arch = "aarch64") {
        "natives-linux-aarch_64"
    } else {
        "natives-linux"
    }
}

fn name_to_path(name: &str) -> PathBuf {
    // group:artifact:version[:classifier] → group/path/artifact/version/artifact-version[-classifier].jar
    let parts: Vec<&str> = name.split(':').collect();
    if parts.len() < 3 {
        return PathBuf::from(name.replace(':', "/"));
    }
    let group_path = parts[0].replace('.', "/");
    let artifact = parts[1];
    let version = parts[2];
    let classifier = parts.get(3).copied();
    let file = match classifier {
        Some(c) => format!("{artifact}-{version}-{c}.jar"),
        None => format!("{artifact}-{version}.jar"),
    };
    PathBuf::from(format!("{group_path}/{artifact}/{version}/{file}"))
}

fn extract_natives(jar_path: &Path, dest: &Path) -> Result<(), String> {
    let file = std::fs::File::open(jar_path).map_err(|e| format!("打开 natives jar 失败: {e}"))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("natives zip 解析失败: {e}"))?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let name = entry.name().to_string();
        let lower = name.to_lowercase();
        if !(lower.ends_with(".dll") || lower.ends_with(".so") || lower.ends_with(".dylib")) {
            continue;
        }
        let out = dest.join(Path::new(&name).file_name().unwrap_or_default());
        let mut out_file = std::fs::File::create(&out).map_err(|e| e.to_string())?;
        std::io::copy(&mut entry, &mut out_file).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// rules 判定：该 library/argument 在当前平台(Windows x64)是否可用
fn library_allowed(lib: &Value) -> bool {
    match lib.get("rules") {
        None => true,
        Some(Value::Array(rules)) => {
            // 规范：存在 rules 时默认不允许（修复 linux/macos 专属库被误下载的问题）
            let mut allowed = false;
            for rule in rules {
                let action = rule.get("action").and_then(|a| a.as_str()).unwrap_or("allow");
                let os_match = rule
                    .get("os")
                    .map(|os| {
                        os.get("name").and_then(|n| n.as_str()) == Some("windows")
                    })
                    .unwrap_or(true);
                let features_ok = rule
                    .get("features")
                    .map(|f| {
                        // has_custom_resolution / is_demo_user 等一律视为 false
                        f.as_object().map(|m| m.is_empty()).unwrap_or(false)
                    })
                    .unwrap_or(true);
                if os_match && features_ok {
                    allowed = action == "allow";
                }
            }
            allowed
        }
        _ => true,
    }
}

// ---------------------------------------------------------------- 版本安装命令

/// 安装原版游戏版本（从清单拉取）
#[tauri::command]
pub async fn mc_install_version(
    app: AppHandle,
    game_dir: String,
    version_id: String,
) -> Result<String, String> {
    let client = downloader::http_client();
    let prefer = prefer_mirror(&app);
    let manifest_urls = downloader::urls_with_setting(
        "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json",
        prefer,
    );
    let mut version_url = None;
    let mut last_err = String::new();
    for url in manifest_urls {
        if let Ok(resp) = client.get(&url).send().await {
            if resp.status().is_success() {
                let manifest: VersionManifest = resp.json().await.map_err(|e| e.to_string())?;
                if let Some(entry) = manifest.versions.iter().find(|v| v.id == version_id) {
                    version_url = Some(entry.url.clone());
                    break;
                }
                last_err = format!("清单中不存在版本 {version_id}");
            } else {
                last_err = format!("HTTP {}", resp.status());
            }
        }
    }
    let version_url = version_url.ok_or(format!("定位版本失败: {last_err}"))?;
    let vjson = fetch_version_json(&client, &version_url, prefer).await?;
    install_version(&app, Path::new(&game_dir), &version_id, &vjson).await?;
    Ok(version_id)
}

// ---------------------------------------------------------------- Fabric / Quilt

#[derive(Debug, Deserialize)]
struct FabricLoaderEntry {
    loader: FabricLoader,
    #[serde(default)]
    intermediary: FabricIntermediary,
}

#[derive(Debug, Deserialize)]
struct FabricLoader {
    version: String,
    #[serde(default)]
    stable: bool,
}

#[derive(Debug, Deserialize, Default)]
struct FabricIntermediary {
    #[serde(default)]
    version: String,
}

/// 列出 Fabric loader 版本（新→旧，稳定版优先）
#[tauri::command]
pub async fn loader_versions(
    app: AppHandle,
    kind: String,
    mc_version: String,
) -> Result<Vec<String>, String> {
    let client = downloader::http_client();
    let (meta_base, is_quilt) = match kind.as_str() {
        "fabric" => (format!("{}/fabric-meta", downloader::BMCLAPI), false),
        "quilt" => (format!("{}/quilt-meta/v3", downloader::BMCLAPI), true),
        _ => return Ok(vec![]),
    };
    let path = if is_quilt {
        format!("{meta_base}/versions/loader/{mc_version}")
    } else {
        format!("{meta_base}/v2/versions/loader/{mc_version}")
    };
    let entries: Vec<FabricLoaderEntry> = client
        .get(&path)
        .send()
        .await
        .map_err(|e| format!("获取 loader 列表失败: {e}"))?
        .json()
        .await
        .map_err(|e| format!("解析 loader 列表失败: {e}"))?;

    let mut versions: Vec<String> = entries.into_iter().map(|e| e.loader.version).collect();
    // 保持服务端顺序（通常新→旧）
    versions.dedup();
    Ok(versions)
}

/// 安装 Fabric / Quilt（下载 profile json 与原版 json 合并后走统一安装管线）
#[tauri::command]
pub async fn mc_install_loader(
    app: AppHandle,
    game_dir: String,
    mc_version: String,
    kind: String,
    loader_version: String,
) -> Result<String, String> {
    let client = downloader::http_client();
    let prefer = prefer_mirror(&app);
    let is_quilt = kind == "quilt";

    // 1. 原版 json（父版本）
    let manifest_urls = downloader::urls_with_setting(
        "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json",
        prefer,
    );
    let mut version_url = None;
    for url in manifest_urls {
        if let Ok(resp) = client.get(&url).send().await {
            if let Ok(manifest) = resp.json::<VersionManifest>().await {
                if let Some(entry) = manifest.versions.iter().find(|v| v.id == mc_version) {
                    version_url = Some(entry.url.clone());
                    break;
                }
            }
        }
    }
    let version_url = version_url.ok_or("版本清单中不存在该 MC 版本")?;
    let parent = fetch_version_json(&client, &version_url, prefer).await?;

    // 2. loader profile json
    let meta_base = if is_quilt {
        format!("{}/quilt-meta/v3", downloader::BMCLAPI)
    } else {
        format!("{}/fabric-meta", downloader::BMCLAPI)
    };
    let profile_url = format!(
        "{meta_base}/versions/loader/{mc_version}/{loader_version}/profile/json"
    );
    let profile: Value = client
        .get(&profile_url)
        .send()
        .await
        .map_err(|e| format!("获取 loader profile 失败: {e}"))?
        .json()
        .await
        .map_err(|e| format!("解析 loader profile 失败: {e}"))?;

    let merged_id = profile
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or(&format!("{kind}-loader-{loader_version}-{mc_version}"))
        .to_string();

    // 3. 合并：父为基础，子覆盖
    let mut merged = parent.clone();
    if let Value::Object(map) = &mut merged {
        if let Value::Object(child) = &profile {
            for (k, v) in child {
                match k.as_str() {
                    "libraries" => {
                        // 合并 libraries 数组
                        let mut libs = map.get("libraries").cloned().unwrap_or(json!([]));
                        if let Value::Array(parent_libs) = &mut libs {
                            if let Value::Array(child_libs) = v {
                                parent_libs.extend(child_libs.iter().cloned());
                            }
                        }
                        map.insert(k.clone(), libs);
                    }
                    "inheritsFrom" => {}
                    _ => {
                        map.insert(k.clone(), v.clone());
                    }
                }
            }
        }
        map.insert("id".to_string(), Value::String(merged_id.clone()));
    }

    install_version(&app, Path::new(&game_dir), &merged_id, &merged).await?;
    Ok(merged_id)
}

// ---------------------------------------------------------------- Forge / NeoForge

/// 拉取 maven-metadata.xml 里的全部版本号（url 为完整地址）
async fn maven_versions_at(url: &str) -> Result<Vec<String>, String> {
    let client = downloader::http_client();
    let text = client
        .get(url)
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await
        .map_err(|e| format!("获取 maven 元数据失败: {e}"))?
        .text()
        .await
        .map_err(|e| format!("读取 maven 元数据失败: {e}"))?;
    let mut versions = vec![];
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with("<version>") && line.ends_with("</version>") {
            versions.push(line[8..line.len() - 9].to_string());
        }
    }
    Ok(versions)
}

/// 官方 Maven API（JSON）兜底：NeoForge 版本列表
async fn neoforge_versions_api() -> Vec<String> {
    let client = downloader::http_client();
    let url = "https://maven.neoforged.net/api/maven/versions/releases/net/neoforged/neoforge";
    match client.get(url).timeout(std::time::Duration::from_secs(20)).send().await {
        Ok(resp) if resp.status().is_success() => match resp.json::<serde_json::Value>().await {
            Ok(v) => v
                .get("versions")
                .and_then(|x| x.as_array())
                .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
                .unwrap_or_default(),
            Err(_) => vec![],
        },
        _ => vec![],
    }
}

/// 按 MC 版本推导 NeoForge 版本号前缀（新号规则：丢弃前导 1.）
/// MC 1.21.1 → 21.1.x；MC 26.4 → 26.4.x；旧版 1.20.1 → 1.20.1-（net.neoforged:forge 兼容线）
fn neoforge_prefix(mc_version: &str) -> (String, bool) {
    let (major, minor, patch) = java::parse_mc_version(mc_version);
    let is_new = major >= 2 || (major == 1 && (minor >= 21 || (minor == 20 && patch >= 5)));
    let prefix = if !is_new {
        format!("{major}.{minor}")
    } else if major == 1 {
        format!("{minor}.{patch}")
    } else {
        format!("{major}.{minor}")
    };
    (prefix, is_new)
}

/// 版本串排序键：取全部数字段
fn version_key(v: &str) -> Vec<u32> {
    v.split(|c: char| !c.is_ascii_digit())
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.parse::<u32>().ok())
        .collect()
}

/// 列出 Forge / NeoForge 版本（新→旧，稳定版优先）
/// 全部走 BMCLAPI /maven/ 镜像优先，官方源回退——国内直连官方 maven 基本不通
#[tauri::command]
pub async fn forge_versions(
    app: AppHandle,
    kind: String,
    mc_version: String,
) -> Result<Vec<String>, String> {
    let mut versions: Vec<String> = vec![];
    let mut last_err = String::new();
    let bmcl = downloader::BMCLAPI;

    if kind == "neoforge" {
        let (prefix, is_new) = neoforge_prefix(&mc_version);
        // 源1：BMCLAPI 镜像 neoforge 元数据；源2：BMCLAPI 镜像 forge 元数据（1.20.1 兼容线）
        // 源3：官方 /releases；源4：官方 Maven API
        let sources = [
            format!("{bmcl}/maven/net/neoforged/neoforge/maven-metadata.xml"),
            format!("{bmcl}/maven/net/neoforged/forge/maven-metadata.xml"),
            "https://maven.neoforged.net/releases/net/neoforged/neoforge/maven-metadata.xml".to_string(),
            "https://maven.neoforged.net/releases/net/neoforged/forge/maven-metadata.xml".to_string(),
        ];
        for url in &sources {
            match maven_versions_at(url).await {
                Ok(v) if !v.is_empty() => {
                    versions = v;
                    break;
                }
                Ok(_) => last_err = format!("{url} 返回空列表"),
                Err(e) => last_err = e,
            }
        }
        if versions.is_empty() {
            let api = neoforge_versions_api().await;
            if !api.is_empty() {
                versions = api;
            }
        }
        if versions.is_empty() {
            return Err(format!("获取 NeoForge 版本失败: {last_err}"));
        }
        if is_new {
            versions.retain(|v| v.starts_with(&format!("{prefix}.")) || v == &prefix);
        } else {
            versions.retain(|v| v.starts_with(&format!("{mc_version}-")));
        }
    } else {
        // Forge：版本号形如 1.20.1-47.1.79 / 1.21.1-52.0.40
        let sources = [
            format!("{bmcl}/maven/net/minecraftforge/forge/maven-metadata.xml"),
            "https://maven.minecraftforge.net/net/minecraftforge/forge/maven-metadata.xml".to_string(),
        ];
        for url in &sources {
            match maven_versions_at(url).await {
                Ok(v) if !v.is_empty() => {
                    versions = v;
                    break;
                }
                Ok(_) => last_err = format!("{url} 返回空列表"),
                Err(e) => last_err = e,
            }
        }
        if versions.is_empty() {
            return Err(format!("获取 Forge 版本失败: {last_err}"));
        }
        versions.retain(|v| v.starts_with(&format!("{mc_version}-")));
    }

    // 数字段倒序（新的在前）
    versions.sort_by(|a, b| version_key(b).cmp(&version_key(a)));
    versions.dedup();
    // 稳定版（不含 beta/alpha）提前作为推荐
    if let Some(idx) = versions.iter().position(|v| !v.contains("-beta") && !v.contains("-alpha")) {
        if idx > 0 {
            let stable = versions.remove(idx);
            versions.insert(0, stable);
        }
    }
    versions.truncate(30);
    let _ = &app;
    Ok(versions)
}

/// Forge/NeoForge 官方安装器会检查游戏目录下的 launcher_profiles.json，缺失即中止
fn ensure_launcher_profile(game_dir: &Path, mc_version: &str) {
    let path = game_dir.join("launcher_profiles.json");
    if path.exists() {
        return;
    }
    let now = chrono::Utc::now().to_rfc3339();
    let profile = json!({
        "profiles": {
            mc_version: {
                "name": mc_version,
                "type": "custom",
                "created": now,
                "lastUsed": now,
                "lastVersionId": mc_version,
                "icon": "Furnace"
            }
        },
        "selectedProfile": mc_version,
        "clientToken": format!("ccl-{}", std::process::id())
    });
    if let Ok(text) = serde_json::to_string_pretty(&profile) {
        let _ = std::fs::write(path, text);
    }
}

/// 把 installer jar 内 install_profile.json / version.json 的官方 Maven 地址
/// 改写为 BMCLAPI 镜像，避免安装器内部下载依赖时直连官方超时（国内必需）
fn patch_installer_mirrors(installer_path: &Path) -> Result<(), String> {
    use std::io::{Read, Write};

    let entries: Vec<(String, Vec<u8>)> = {
        let file = std::fs::File::open(installer_path).map_err(|e| e.to_string())?;
        let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
        let mut out = Vec::with_capacity(archive.len());
        for i in 0..archive.len() {
            let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
            let name = entry.name().to_string();
            let mut buf = Vec::new();
            entry.read_to_end(&mut buf).map_err(|e| e.to_string())?;
            out.push((name, buf));
        }
        out
    };

    let tmp_path = installer_path.with_extension("mirror.tmp");
    {
        let out_file = std::fs::File::create(&tmp_path).map_err(|e| e.to_string())?;
        let mut writer = zip::ZipWriter::new(out_file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for (name, buf) in entries {
            if name.ends_with('/') {
                continue; // 目录条目跳过，避免 zip 库差异
            }
            let data = if name == "install_profile.json" || name == "version.json" {
                let text = String::from_utf8_lossy(&buf)
                    .replace("https://maven.neoforged.net/releases/", &format!("{}/maven/", downloader::BMCLAPI))
                    .replace("https://maven.neoforged.net/", &format!("{}/maven/", downloader::BMCLAPI))
                    .replace("https://maven.minecraftforge.net/", &format!("{}/maven/", downloader::BMCLAPI))
                    .replace("https://libraries.minecraft.net/", &format!("{}/maven/", downloader::BMCLAPI));
                text.into_bytes()
            } else {
                buf
            };
            writer.start_file(name, options).map_err(|e| e.to_string())?;
            writer.write_all(&data).map_err(|e| e.to_string())?;
        }
        writer.finish().map_err(|e| e.to_string())?;
    }

    std::fs::remove_file(installer_path).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp_path, installer_path).map_err(|e| e.to_string())?;
    Ok(())
}

/// 安装 Forge / NeoForge（下载 installer jar → 改写内嵌 Maven 地址 → --installClient）
#[tauri::command]
pub async fn mc_install_forge(
    app: AppHandle,
    game_dir: String,
    mc_version: String,
    kind: String,
    loader_version: String,
) -> Result<String, String> {
    let client = downloader::http_client();
    let game_path = Path::new(&game_dir);

    // 先装原版（installer 会复用已装文件，且国内镜像更快）
    let _ = mc_install_version(app.clone(), game_dir.clone(), mc_version.clone()).await;
    ensure_launcher_profile(game_path, &mc_version);

    // installer jar 地址：BMCLAPI 镜像优先，官方 /releases 回退
    // NeoForge 1.20.1 兼容线（版本号以 1. 开头）包名是 forge，其余是 neoforge
    let (jar_urls, jar_name) = if kind == "neoforge" {
        let package = if loader_version.starts_with("1.") { "forge" } else { "neoforge" };
        (
            vec![
                format!(
                    "{}/maven/net/neoforged/{package}/{loader_version}/{package}-{loader_version}-installer.jar",
                    downloader::BMCLAPI
                ),
                format!(
                    "https://maven.neoforged.net/releases/net/neoforged/{package}/{loader_version}/{package}-{loader_version}-installer.jar"
                ),
            ],
            format!("{package}-{loader_version}-installer.jar"),
        )
    } else {
        let v = format!("{mc_version}-{loader_version}");
        (
            vec![
                format!(
                    "{}/maven/net/minecraftforge/forge/{v}/forge-{v}-installer.jar",
                    downloader::BMCLAPI
                ),
                format!(
                    "https://maven.minecraftforge.net/net/minecraftforge/forge/{v}/forge-{v}-installer.jar"
                ),
            ],
            format!("forge-{v}-installer.jar"),
        )
    };

    let installer_dir = game_path.join("installers");
    std::fs::create_dir_all(&installer_dir).map_err(|e| e.to_string())?;
    let jar_path = installer_dir.join(&jar_name);
    let task = DlTask::new(&format!("installer-{jar_name}"), &jar_name, &jar_urls[0], &jar_path)
        .with_fallbacks(jar_urls[1..].to_vec());
    downloader::download_one(&client, &app, &task).await?;

    // 安装器内部依赖下载走官方 maven，国内必超时 → 改写 jar 内地址为 BMCLAPI 镜像
    let orig_path = jar_path.with_extension("orig.jar");
    let patched = std::fs::copy(&jar_path, &orig_path)
        .ok()
        .and_then(|_| patch_installer_mirrors(&jar_path).ok())
        .is_some();

    let java_info = java::resolve_java(app.clone(), mc_version.clone())
        .or_else(|_| java::auto_select_java(&app, "1.21"))
        .map_err(|e| format!("安装 {kind} 需要 Java 17+: {e}"))?;

    let versions_before: Vec<String> = std::fs::read_dir(game_path.join("versions"))
        .map(|rd| rd.flatten().filter_map(|e| e.file_name().to_string_lossy().to_string().into()).collect())
        .unwrap_or_default();

    let _ = app.emit(
        "mc-install-progress",
        InstallProgress { phase: format!("安装 {kind}"), done: 0, total: 0 },
    );

    // 先跑镜像改写版；失败则还原原版 jar 再跑一次（官方源可用时兜底）
    let run = |jar: &Path| -> Result<std::process::Output, String> {
        std::process::Command::new(&java_info.path)
            .arg("-jar")
            .arg(jar)
            .arg("--installClient")
            .arg(game_path)
            .current_dir(game_path)
            .output()
            .map_err(|e| format!("运行 installer 失败: {e}"))
    };

    let mut output = run(&jar_path)?;
    let mut used_mirror = true;
    if (!output.status.success() || !stderr_looks_ok(&output)) && patched && orig_path.exists() {
        let _ = std::fs::copy(&orig_path, &jar_path);
        if let Ok(o2) = run(&jar_path) {
            output = o2;
            used_mirror = false;
        }
    }

    if !output.status.success() {
        let _ = std::fs::remove_file(&jar_path);
        let _ = std::fs::remove_file(&orig_path);
        return Err(format!(
            "{} 安装器执行失败: {}",
            kind,
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let _ = used_mirror;

    // 找出新安装的版本目录
    let versions_after: Vec<String> = std::fs::read_dir(game_path.join("versions"))
        .map(|rd| rd.flatten().filter_map(|e| e.file_name().to_string_lossy().to_string().into()).collect())
        .unwrap_or_default();
    let new_id = versions_after
        .into_iter()
        .find(|v| !versions_before.contains(v))
        .ok_or_else(|| format!("{} 安装后未发现新版本目录", kind))?;

    // installer 装的版本统一补一遍 assets/libraries（走镜像）
    let vjson_path = game_path
        .join("versions")
        .join(&new_id)
        .join(format!("{new_id}.json"));
    if let Ok(text) = std::fs::read_to_string(&vjson_path) {
        if let Ok(vjson) = serde_json::from_str::<Value>(&text) {
            install_version(&app, game_path, &new_id, &vjson).await?;
        }
    }

    let _ = std::fs::remove_file(&jar_path);
    let _ = std::fs::remove_file(&orig_path);
    Ok(new_id)
}

/// installer 退出码为 0 但 stderr 含典型失败字样时也视为失败（换源重试）
fn stderr_looks_ok(output: &std::process::Output) -> bool {
    let text = String::from_utf8_lossy(&output.stderr).to_lowercase();
    !text.contains("failed to download")
        && !text.contains("connection")
        && !text.contains("timed out")
        && !text.contains("unable to")
}
