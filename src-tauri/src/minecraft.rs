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
    /// 清单里字段名是 type；序列化给前端时保持 vtype（前端契约）
    #[serde(alias = "type")]
    pub vtype: String,
    pub url: String,
    /// 版本 JSON 最后修改时间（ISO 8601）
    #[serde(default)]
    pub time: String,
    /// 正式发布时间（ISO 8601，清单字段名 releaseTime）
    #[serde(default, alias = "releaseTime")]
    pub release_time: String,
}

/// 单个版本的「单独测试」结果
#[derive(Debug, Clone, Serialize)]
pub struct TestStep {
    pub name: String,
    pub ok: bool,
    pub detail: String,
}

/// 单个版本的完整测试报告
#[derive(Debug, Clone, Serialize)]
pub struct VersionTestReport {
    pub version_id: String,
    pub ok: bool,
    pub steps: Vec<TestStep>,
    /// 测试过程累计下载体积（字节）
    pub total_size: u64,
    /// 耗时（毫秒）
    pub elapsed_ms: u128,
    /// 测试用的隔离目录（失败时保留以便排查）
    pub test_dir: String,
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
        // BMCLAPI 的清单走签名 CDN 302，偶发返回空 body——同源重试可拿到新签名
        for attempt in 0..2 {
            match client
                .get(&url)
                .timeout(std::time::Duration::from_secs(60))
                .send()
                .await
            {
                Ok(resp) if resp.status().is_success() => {
                    let text = match resp.text().await {
                        Ok(t) => t,
                        Err(e) => {
                            last_err = format!("读取响应失败: {e}");
                            continue;
                        }
                    };
                    if text.trim().is_empty() {
                        last_err = format!("{url} 返回空内容（第 {} 次）", attempt + 1);
                        continue;
                    }
                    match serde_json::from_str::<VersionManifest>(&text) {
                        Ok(manifest) => {
                            let _ = app.emit(
                                "mc-latest",
                                json!({ "release": manifest.latest.release, "snapshot": manifest.latest.snapshot }),
                            );
                            return Ok(manifest.versions);
                        }
                        Err(e) => last_err = format!("解析版本清单失败: {e}"),
                    }
                }
                Ok(resp) => last_err = format!("HTTP {}", resp.status()),
                Err(e) => last_err = e.to_string(),
            }
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
        // 签名 CDN 偶发空 body：同源重试拿新签名
        for attempt in 0..2 {
            match client
                .get(&u)
                .timeout(std::time::Duration::from_secs(45))
                .send()
                .await
            {
                Ok(resp) if resp.status().is_success() => {
                    let text = match resp.text().await {
                        Ok(t) => t,
                        Err(e) => {
                            last_err = format!("读取响应失败: {e}");
                            continue;
                        }
                    };
                    if text.trim().is_empty() {
                        last_err = format!("{u} 返回空内容（第 {} 次）", attempt + 1);
                        continue;
                    }
                    return serde_json::from_str(&text).map_err(|e| format!("解析版本 JSON 失败: {e}"));
                }
                Ok(resp) => last_err = format!("HTTP {}", resp.status()),
                Err(e) => last_err = e.to_string(),
            }
        }
    }
    Err(format!("获取版本 JSON 失败({url}): {last_err}"))
}

/// 安装一个完整版本（json 已合并好）：client jar / libraries / assets / natives
/// full_assets=false 时只抽样下载少量资源对象（用于版本单独测试，避免全量下载）
async fn install_version(
    app: &AppHandle,
    game_dir: &Path,
    version_id: &str,
    vjson: &Value,
    full_assets: bool,
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
    // 1.13 / 20w06a 等版本 JSON 会把同一库列两份（仅 rules 不同），
    // 若不去重会产生两个任务写同一文件：并发截断/删除导致 SHA1 读取失败。
    let mut seen_dests: std::collections::HashSet<PathBuf> = std::collections::HashSet::new();
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
            // 合成库（如 Quilt maven 兜底）可携带 fallbacks 候选地址列表
            let extra_fallbacks: Vec<String> = artifact
                .get("fallbacks")
                .and_then(|f| f.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|u| u.as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default();
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
            if !seen_dests.insert(dest.clone()) {
                continue; // 同路径已安排下载，跳过重复条目
            }
            let mut fallbacks = extra_fallbacks;
            fallbacks.extend(downloader::urls_with_setting(url, prefer));
            lib_tasks.push(
                DlTask::new(
                    &format!("lib-{version_id}-{i}"),
                    lib.get("name").and_then(|n| n.as_str()).unwrap_or("library"),
                    url,
                    &dest,
                )
                .with_fallbacks(fallbacks)
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
                        if !seen_dests.insert(dest.clone()) {
                            continue; // 同路径已安排下载，跳过重复条目
                        }
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

    // ---- natives 解压（去重：同一 jar 可能被重复列出）
    native_jars.sort();
    native_jars.dedup();
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
        // 测试模式只抽样前若干个对象验证资源 CDN 通路；正式安装才全量下载
        let sample: Vec<(&String, &AssetObject)> = if full_assets {
            objects.clone()
        } else {
            objects.iter().take(8).copied().collect()
        };
        let mut asset_tasks: Vec<DlTask> = vec![];
        for (name, obj) in sample {
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

/// 定位版本并拉取其 JSON（清单多源回退）
async fn resolve_version_json(app: &AppHandle, version_id: &str) -> Result<Value, String> {
    let client = downloader::http_client();
    let prefer = prefer_mirror(app);
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
    fetch_version_json(&client, &version_url, prefer).await
}

/// 安装原版游戏版本（从清单拉取）到指定游戏目录
#[tauri::command]
pub async fn mc_install_version(
    app: AppHandle,
    game_dir: String,
    version_id: String,
) -> Result<String, String> {
    let vjson = resolve_version_json(&app, &version_id).await?;
    install_version(&app, Path::new(&game_dir), &version_id, &vjson, true).await?;
    Ok(version_id)
}

/// 下载游戏本体并为它自动创建一个独立实例（PCL/HMCL 「下载即一个版本」的行为）。
/// 实例名默认与版本号同名，重名自动追加序号；安装失败会回滚删除刚建的实例目录。
#[tauri::command]
pub async fn mc_download_as_instance(
    app: AppHandle,
    version_id: String,
    name: Option<String>,
    max_memory: Option<u32>,
) -> Result<crate::instances::Instance, String> {
    // 先拿到版本 JSON：网络/清单失败时不留任何空实例
    let vjson = resolve_version_json(&app, &version_id).await?;

    let inst_name = match name.filter(|n| !n.trim().is_empty()) {
        Some(n) => n,
        None => crate::instances::unique_name(&app, &version_id),
    };
    let inst = crate::instances::add_instance(
        &app,
        &inst_name,
        &version_id,
        "vanilla",
        None,
        max_memory,
    )?;

    match install_version(&app, Path::new(&inst.game_dir), &version_id, &vjson, true).await {
        Ok(()) => crate::instances::update_instance(
            app.clone(),
            inst.id.clone(),
            crate::instances::InstancePatch {
                installed_version_id: Some(version_id.clone()),
                loader: Some("vanilla".into()),
                ..Default::default()
            },
        ),
        Err(e) => {
            crate::instances::remove_instance_dir(&app, &inst.id);
            Err(format!("下载失败，已回滚新建的实例：{e}"))
        }
    }
}

/// 递归统计目录体积
fn dir_size(p: &Path) -> u64 {
    let mut total = 0u64;
    if let Ok(entries) = std::fs::read_dir(p) {
        for e in entries.flatten() {
            let path = e.path();
            if path.is_dir() {
                total += dir_size(&path);
            } else if let Ok(m) = e.metadata() {
                total += m.len();
            }
        }
    }
    total
}

/// 统计目录内指定扩展名的文件数
fn count_files(p: &Path, ext: &str) -> usize {
    let mut n = 0usize;
    if let Ok(entries) = std::fs::read_dir(p) {
        for e in entries.flatten() {
            let path = e.path();
            if path.is_dir() {
                n += count_files(&path, ext);
            } else if path.extension().and_then(|x| x.to_str()) == Some(ext) {
                n += 1;
            }
        }
    }
    n
}

/// 单个版本「单独测试」：在隔离临时目录中完整走一遍安装管线并逐项校验，
/// 不污染任何实例。测试项：版本清单定位 / 版本 JSON / 客户端 jar(SHA1) /
/// 依赖库 / natives / 资源索引与资源 CDN 抽样。
#[tauri::command]
pub async fn mc_test_version(
    app: AppHandle,
    version_id: String,
) -> Result<VersionTestReport, String> {
    let started = std::time::Instant::now();
    let mut steps: Vec<TestStep> = vec![];
    let client = downloader::http_client();
    let prefer = prefer_mirror(&app);

    let done_report = |steps: Vec<TestStep>, total_size: u64, test_dir: &Path, keep: bool| {
        let ok = steps.iter().all(|s| s.ok);
        if !keep {
            let _ = std::fs::remove_dir_all(test_dir);
        }
        VersionTestReport {
            version_id: version_id.clone(),
            ok,
            steps,
            total_size,
            elapsed_ms: started.elapsed().as_millis(),
            test_dir: test_dir.display().to_string(),
        }
    };

    // 1. 定位版本（镜像链回退）
    let manifest_urls = downloader::urls_with_setting(
        "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json",
        prefer,
    );
    let mut version_url = None;
    let mut last_err = String::new();
    for url in manifest_urls {
        if let Ok(resp) = client.get(&url).send().await {
            if resp.status().is_success() {
                match resp.json::<VersionManifest>().await {
                    Ok(manifest) => {
                        if let Some(entry) = manifest.versions.iter().find(|v| v.id == version_id) {
                            version_url = Some(entry.url.clone());
                            break;
                        }
                        last_err = format!("清单中不存在版本 {version_id}");
                    }
                    Err(e) => last_err = format!("解析版本清单失败: {e}"),
                }
            } else {
                last_err = format!("HTTP {}", resp.status());
            }
        }
    }
    let version_url = match version_url {
        Some(u) => u,
        None => {
            steps.push(TestStep { name: "版本清单定位".into(), ok: false, detail: last_err });
            let empty = std::env::temp_dir().join("ccl-test-empty");
            return Ok(done_report(steps, 0, &empty, false));
        }
    };
    steps.push(TestStep {
        name: "版本清单定位".into(),
        ok: true,
        detail: format!("{}（{}）", version_url, if prefer { "镜像优先" } else { "官方源" }),
    });

    // 2. 版本 JSON
    let vjson = match fetch_version_json(&client, &version_url, prefer).await {
        Ok(j) => j,
        Err(e) => {
            steps.push(TestStep { name: "版本 JSON".into(), ok: false, detail: e });
            let empty = std::env::temp_dir().join("ccl-test-empty");
            return Ok(done_report(steps, 0, &empty, false));
        }
    };
    let vtype = vjson.get("type").and_then(|t| t.as_str()).unwrap_or("unknown");
    steps.push(TestStep {
        name: "版本 JSON".into(),
        ok: true,
        detail: format!("解析成功（type={vtype}）"),
    });

    // 3. 隔离临时目录中执行安装（测试模式：资源对象只抽样）
    let safe_id: String = version_id
        .chars()
        .map(|c| if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') { '_' } else { c })
        .collect();
    let test_dir = std::env::temp_dir().join(format!("ccl-test-{safe_id}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&test_dir);
    if let Err(e) = std::fs::create_dir_all(&test_dir) {
        steps.push(TestStep { name: "测试目录".into(), ok: false, detail: e.to_string() });
        let empty = std::env::temp_dir().join("ccl-test-empty");
        return Ok(done_report(steps, 0, &empty, false));
    }

    let _ = app.emit(
        "mc-test-progress",
        json!({ "version_id": version_id, "phase": "安装中", "done": 0, "total": 0 }),
    );

    match install_version(&app, &test_dir, &version_id, &vjson, false).await {
        Ok(()) => steps.push(TestStep {
            name: "安装管线".into(),
            ok: true,
            detail: "client jar / 依赖库 / natives / 资源索引 全部完成".into(),
        }),
        Err(e) => {
            steps.push(TestStep { name: "安装管线".into(), ok: false, detail: e });
            let size = dir_size(&test_dir);
            return Ok(done_report(steps, size, &test_dir, true));
        }
    }

    // 4. 逐项校验产物
    let version_dir = test_dir.join("versions").join(&version_id);
    let jar = version_dir.join(format!("{version_id}.jar"));
    match std::fs::metadata(&jar) {
        Ok(m) if m.len() > 0 => steps.push(TestStep {
            name: "客户端 jar".into(),
            ok: true,
            detail: format!("{} KB（下载管线已做 SHA1 校验）", m.len() / 1024),
        }),
        Ok(_) => steps.push(TestStep { name: "客户端 jar".into(), ok: false, detail: "文件为空".into() }),
        Err(e) => steps.push(TestStep { name: "客户端 jar".into(), ok: false, detail: e.to_string() }),
    }

    let libs_dir = test_dir.join("libraries");
    let lib_count = count_files(&libs_dir, "jar");
    steps.push(TestStep {
        name: "依赖库".into(),
        ok: lib_count > 0,
        detail: format!("{lib_count} 个 jar"),
    });

    let natives_dir = version_dir.join("natives");
    let dll_count = count_files(&natives_dir, "dll");
    steps.push(TestStep {
        name: "natives 解压".into(),
        ok: natives_dir.exists(),
        detail: format!("{dll_count} 个 dll"),
    });

    let index_path = test_dir
        .join("assets")
        .join("indexes")
        .join(format!("{}.json", vjson.get("assetIndex").and_then(|a| a.get("id")).and_then(|i| i.as_str()).unwrap_or("pre")));
    match std::fs::read_to_string(&index_path) {
        Ok(text) => match serde_json::from_str::<AssetIndex>(&text) {
            Ok(idx) => steps.push(TestStep {
                name: "资源索引".into(),
                ok: true,
                detail: format!("{} 个资源对象（已抽样验证 CDN）", idx.objects.len()),
            }),
            Err(e) => steps.push(TestStep { name: "资源索引".into(), ok: false, detail: e.to_string() }),
        },
        Err(e) => steps.push(TestStep { name: "资源索引".into(), ok: false, detail: e.to_string() }),
    }

    let size = dir_size(&test_dir);
    Ok(done_report(steps, size, &test_dir, false))
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

/// 云控 Worker 基础地址（用于 Quilt meta 代理；域名未开通时自动跳过）
fn cloud_base_url() -> Option<String> {
    crate::updater::CLOUD_CONFIG_URL
        .strip_suffix("/config")
        .map(|s| s.to_string())
}

/// 按源链拉取 JSON 数组，第一个返回非空数组的源胜出。
/// 所有源都能连通但都为空（如 Fabric 已不服务 1.7.10、MC 26.4 尚未发布 loader）时
/// 返回 Ok(vec![])——这是"上游没有"而非网络故障；只有全部源网络/解析失败才返回 Err
async fn fetch_loader_entries(urls: &[String]) -> Result<Vec<FabricLoaderEntry>, String> {
    let client = downloader::http_client();
    let mut last_err = String::new();
    let mut any_ok = false;
    for url in urls {
        match client
            .get(url)
            .timeout(std::time::Duration::from_secs(20))
            .send()
            .await
        {
            Ok(resp) => match resp.json::<Vec<FabricLoaderEntry>>().await {
                Ok(list) if !list.is_empty() => return Ok(list),
                Ok(_) => {
                    any_ok = true;
                    last_err = format!("{url} 返回空列表");
                }
                Err(e) => last_err = format!("{url} 解析失败: {e}"),
            },
            Err(e) => last_err = format!("{url} 请求失败: {e}"),
        }
    }
    if any_ok {
        return Ok(vec![]);
    }
    Err(last_err)
}

/// 列出 Fabric / Quilt loader 版本（新→旧）
/// Fabric：BMCLAPI 镜像 → 官方 meta；Quilt：云控 Worker 代理 → 官方 meta
/// （meta.quiltmc.net 国内 DNS 被污染，必须经 Worker 或科学幻想）
#[tauri::command]
pub async fn loader_versions(
    app: AppHandle,
    kind: String,
    mc_version: String,
) -> Result<Vec<String>, String> {
    let mut urls: Vec<String> = vec![];
    match kind.as_str() {
        "fabric" => {
            urls.push(format!(
                "{}/fabric-meta/v2/versions/loader/{mc_version}",
                downloader::BMCLAPI
            ));
            urls.push(format!(
                "https://meta.fabricmc.net/v2/versions/loader/{mc_version}"
            ));
        }
        "quilt" => {
            if let Some(base) = cloud_base_url() {
                urls.push(format!("{base}/quilt-meta/v3/versions/loader/{mc_version}"));
            }
            urls.push(format!(
                "https://meta.quiltmc.net/v3/versions/loader/{mc_version}"
            ));
        }
        _ => return Ok(vec![]),
    }
    let entries = match fetch_loader_entries(&urls).await {
        Ok(e) => e,
        Err(e) => {
            let _ = &app;
            if kind == "quilt" {
                return Err(format!(
                    "{e}（Quilt 元数据国内不可达：需部署云控 Worker 的 /quilt-meta 代理路由，或使用代理网络）"
                ));
            }
            return Err(e);
        }
    };
    let _ = &app;

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
    // Fabric：BMCLAPI 镜像 → 官方 meta（均为 /v2/ 路径）
    // Quilt：云控 Worker 代理 → 官方 meta → maven.quiltmc.org 现场拼装
    let profile: Value = if is_quilt {
        let mut urls: Vec<String> = vec![];
        if let Some(base) = cloud_base_url() {
            urls.push(format!(
                "{base}/quilt-meta/v3/versions/loader/{mc_version}/{loader_version}/profile/json"
            ));
        }
        urls.push(format!(
            "https://meta.quiltmc.net/v3/versions/loader/{mc_version}/{loader_version}/profile/json"
        ));
        let mut fetched: Option<Value> = None;
        let mut last_err = String::new();
        for url in &urls {
            match client
                .get(url)
                .timeout(std::time::Duration::from_secs(25))
                .send()
                .await
            {
                Ok(resp) => match resp.json::<Value>().await {
                    Ok(v) if v.is_object() => {
                        fetched = Some(v);
                        break;
                    }
                    Ok(_) => last_err = format!("{url} 返回异常响应"),
                    Err(e) => last_err = format!("{url} 解析失败: {e}"),
                },
                Err(e) => last_err = format!("{url} 请求失败: {e}"),
            }
        }
        match fetched {
            Some(v) => v,
            None => quilt_profile_from_maven(&client, &loader_version, &mc_version)
                .await
                .ok_or_else(|| format!("获取 Quilt profile 失败: {last_err}"))?,
        }
    } else {
        let urls = [
            format!(
                "{}/fabric-meta/v2/versions/loader/{mc_version}/{loader_version}/profile/json",
                downloader::BMCLAPI
            ),
            format!(
                "https://meta.fabricmc.net/v2/versions/loader/{mc_version}/{loader_version}/profile/json"
            ),
        ];
        let mut fetched: Option<Value> = None;
        let mut last_err = String::new();
        for url in &urls {
            match client
                .get(url)
                .timeout(std::time::Duration::from_secs(25))
                .send()
                .await
            {
                Ok(resp) => match resp.json::<Value>().await {
                    Ok(v) if v.is_object() => {
                        fetched = Some(v);
                        break;
                    }
                    Ok(_) => last_err = format!("{url} 返回异常响应"),
                    Err(e) => last_err = format!("{url} 解析失败: {e}"),
                },
                Err(e) => last_err = format!("{url} 请求失败: {e}"),
            }
        }
        fetched.ok_or_else(|| format!("获取 Fabric profile 失败: {last_err}"))?
    };

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

    install_version(&app, Path::new(&game_dir), &merged_id, &merged, true).await?;
    Ok(merged_id)
}

/// 从 maven.quiltmc.org 拉取 loader JSON 拼装为 profile
/// （该 JSON 的 libraries 是 {client:[], common:[]} 对象需展平，且无 id/inheritsFrom 需补齐）
/// 为 maven 坐标合成 downloads.artifact（path + 主 url + 回退 url 列表）
/// Quilt 生态分散在多个仓库：org.quiltmc:* → maven.quiltmc.org；
/// net.fabricmc:*（Quilt 复用 Fabric 的 intermediary）→ maven.fabricmc.net；其余走中央仓库。
fn maven_artifact(name: &str) -> Value {
    let path = name_to_path(name).display().to_string().replace('\\', "/");
    let mut urls: Vec<String> = vec![];
    if name.starts_with("org.quiltmc:") {
        urls.push(format!(
            "https://maven.quiltmc.org/repository/release/{path}"
        ));
    }
    if name.starts_with("net.fabricmc:") {
        urls.push(format!("https://maven.fabricmc.net/{path}"));
    }
    urls.push(format!("https://repo1.maven.org/maven2/{path}"));
    urls.push(format!("{}/maven/{path}", downloader::BMCLAPI));
    urls.dedup();
    let primary = urls[0].clone();
    json!({
        "path": path,
        "url": primary,
        "fallbacks": &urls[1..],
    })
}

/// 从 maven.quiltmc.org 的 loader JSON 现场拼装 Quilt profile
/// （云控 Worker 代理 / meta.quiltmc.net 均不可达时的兜底链路）
///
/// 注意：maven 上的 loader JSON 是**残缺版**——libraries 不带 downloads 信息、
/// 没有 loader 本体，也没有 intermediary。且 Quilt 并不发布独立的 quilt-intermediary，
/// 映射层复用 Fabric 的 `net.fabricmc:intermediary:<MC版本>`，必须手动补上，
/// 否则装出来的版本缺映射 jar，启动即崩。
async fn quilt_profile_from_maven(
    client: &reqwest::Client,
    loader_version: &str,
    mc_version: &str,
) -> Option<Value> {
    let url = format!(
        "https://maven.quiltmc.org/repository/release/org/quiltmc/quilt-loader/{loader_version}/quilt-loader-{loader_version}.json"
    );
    let raw: Value = client
        .get(&url)
        .timeout(std::time::Duration::from_secs(25))
        .send()
        .await
        .ok()?
        .json()
        .await
        .ok()?;
    let mut libs: Vec<Value> = vec![];
    if let Some(obj) = raw.get("libraries").and_then(|l| l.as_object()) {
        for key in ["client", "common"] {
            if let Some(arr) = obj.get(key).and_then(|v| v.as_array()) {
                for l in arr {
                    if let Some(name) = l.get("name").and_then(|n| n.as_str()) {
                        libs.push(json!({
                            "name": name,
                            "downloads": { "artifact": maven_artifact(name) },
                        }));
                    }
                }
            }
        }
    }
    // loader 本体（mainClass 所在 jar）
    let loader_name = format!("org.quiltmc:quilt-loader:{loader_version}");
    libs.push(json!({
        "name": loader_name,
        "downloads": { "artifact": maven_artifact(&loader_name) },
    }));
    // Fabric intermediary（映射版 MC，Quilt 启动必需）
    let intermediary_name = format!("net.fabricmc:intermediary:{mc_version}");
    libs.push(json!({
        "name": intermediary_name,
        "downloads": { "artifact": maven_artifact(&intermediary_name) },
    }));
    let mut profile = serde_json::Map::new();
    profile.insert(
        "id".into(),
        Value::String(format!("quilt-loader-{loader_version}-{mc_version}")),
    );
    // mainClass 在 loader JSON 里是 {client, server, serverLauncher} 映射，取 client
    if let Some(mc) = raw.get("mainClass") {
        let main_class = match mc.as_str() {
            Some(s) => Some(Value::String(s.to_string())),
            None => mc.get("client").cloned(),
        };
        if let Some(main_class) = main_class.filter(|v| v.is_string()) {
            profile.insert("mainClass".into(), main_class);
        }
    }
    // 不覆盖 javaVersion：父版本 JSON 里的才是该 MC 版本的真实 Java 要求
    // （loader JSON 的 min_java_version 只是 loader 自身下限，如 0.30.1 是 8）
    profile.insert("libraries".into(), Value::Array(libs));
    Some(Value::Object(profile))
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
        if let Some(inner) = line
            .strip_prefix("<version>")
            .and_then(|s| s.strip_suffix("</version>"))
        {
            versions.push(inner.to_string());
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

/// 版本串排序键：取全部数字段
fn version_key(v: &str) -> Vec<u32> {
    v.split(|c: char| !c.is_ascii_digit())
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.parse::<u32>().ok())
        .collect()
}

/// 列出 Forge / NeoForge 版本（新→旧，稳定版优先）
/// 逐个源拉取并立即按 MC 版本过滤，过滤后非空才采用——
/// 避免陈旧镜像（BMCLAPI 的 forge 元数据停在 2022）或错误产物线
/// （net.neoforged:forge 兼容线里没有 21.x 版本）静默产生空结果
#[tauri::command]
pub async fn forge_versions(
    app: AppHandle,
    kind: String,
    mc_version: String,
) -> Result<Vec<String>, String> {
    let mut last_err = String::new();
    let bmcl = downloader::BMCLAPI;

    // 候选版本号前缀：旧式 "{mc}-"（Forge / NeoForge 兼容线）+ 新式（NeoForge 20.2 起丢弃前导 1.）
    let mut prefixes: Vec<String> = vec![format!("{mc_version}-")];
    let (major, minor, patch) = java::parse_mc_version(&mc_version);
    if major == 1 {
        prefixes.push(format!("{minor}.{patch}."));
    } else if major >= 2 {
        prefixes.push(format!("{major}.{minor}."));
    }
    let matches = |v: &String| prefixes.iter().any(|p| v.starts_with(p));

    let sources: Vec<String> = if kind == "neoforge" {
        vec![
            // BMCLAPI 镜像优先（国内直连官方 maven 不通）
            format!("{bmcl}/maven/net/neoforged/neoforge/maven-metadata.xml"),
            "https://maven.neoforged.net/releases/net/neoforged/neoforge/maven-metadata.xml".to_string(),
            // 1.20.1 及更早的兼容线（net.neoforged:forge，版本号 1.20.1-47.1.x）
            format!("{bmcl}/maven/net/neoforged/forge/maven-metadata.xml"),
            "https://maven.neoforged.net/releases/net/neoforged/forge/maven-metadata.xml".to_string(),
        ]
    } else {
        // Forge：官方源数据最新且国内可达，放首位；BMCLAPI 镜像兜底
        vec![
            "https://maven.minecraftforge.net/net/minecraftforge/forge/maven-metadata.xml".to_string(),
            format!("{bmcl}/maven/net/minecraftforge/forge/maven-metadata.xml"),
        ]
    };

    let mut versions: Vec<String> = vec![];
    let mut any_ok = false; // 是否有源成功拉取（哪怕过滤后为空）
    for url in &sources {
        match maven_versions_at(url).await {
            Ok(list) => {
                any_ok = true;
                let mut filtered: Vec<String> = list.into_iter().filter(|v| matches(v)).collect();
                if !filtered.is_empty() {
                    versions.append(&mut filtered);
                    break;
                }
                last_err = format!("{url} 中没有 MC {mc_version} 的版本");
            }
            Err(e) => last_err = e,
        }
    }
    // 官方 Maven API 兜底（仅 NeoForge，JSON 接口）
    if versions.is_empty() && kind == "neoforge" {
        let api = neoforge_versions_api().await;
        if !api.is_empty() {
            any_ok = true;
            let mut filtered: Vec<String> = api.into_iter().filter(|v| matches(v)).collect();
            if !filtered.is_empty() {
                versions = filtered;
            }
        }
    }
    if versions.is_empty() {
        // 源都可达但都没有该 MC 的版本（如 MC 26.4 尚未发布 loader）→ 空列表而非报错
        if any_ok {
            return Ok(vec![]);
        }
        let name = if kind == "neoforge" { "NeoForge" } else { "Forge" };
        return Err(format!("获取 {name} 版本失败: {last_err}"));
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
            install_version(&app, game_path, &new_id, &vjson, true).await?;
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
