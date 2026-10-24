//! 整合包导入导出：Modrinth .mrpack 与 CurseForge manifest.zip

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::io::{Read, Write};
use std::path::Path;
use tauri::{AppHandle, Emitter};
use zip::{ZipArchive, ZipWriter};

use crate::downloader::{self, DlTask};
use crate::instances;
use crate::minecraft;

#[derive(Debug, Clone, Serialize)]
pub struct ModpackProgress {
    pub phase: String,
    pub done: u64,
    pub total: u64,
}

fn emit(app: &AppHandle, phase: &str, done: u64, total: u64) {
    let _ = app.emit(
        "modpack-progress",
        ModpackProgress { phase: phase.into(), done, total },
    );
}

// ---------------------------------------------------------------- 导入

#[tauri::command]
pub async fn import_modpack(app: AppHandle, path: String, name_override: Option<String>) -> Result<instances::Instance, String> {
    let file = fs::File::open(&path).map_err(|e| format!("打开整合包失败: {e}"))?;
    let mut archive = ZipArchive::new(file).map_err(|e| format!("整合包 zip 解析失败: {e}"))?;

    // 判断格式：modrinth.index.json (mrpack) 或 manifest.json (CurseForge)
    let is_mrpack = archive.by_name("modrinth.index.json").is_ok();

    if is_mrpack {
        import_mrpack(&app, &mut archive, name_override).await
    } else {
        import_curseforge(&app, &mut archive, name_override).await
    }
}

async fn import_mrpack(
    app: &AppHandle,
    archive: &mut ZipArchive<fs::File>,
    name_override: Option<String>,
) -> Result<instances::Instance, String> {
    let mut index_file = archive
        .by_name("modrinth.index.json")
        .map_err(|_| "缺少 modrinth.index.json")?;
    let mut text = String::new();
    index_file.read_to_string(&mut text).map_err(|e| e.to_string())?;
    drop(index_file); // 释放对 archive 的独占借用，后面还要遍历 zip
    let index: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;

    let name = name_override.unwrap_or_else(|| {
        index.get("name").and_then(|v| v.as_str()).unwrap_or("导入的整合包").to_string()
    });
    let deps = index.get("dependencies").cloned().unwrap_or(Value::Null);
    let mc_version = deps
        .get("minecraft")
        .and_then(|v| v.as_str())
        .ok_or("整合包未指定 Minecraft 版本")?
        .to_string();

    // 创建实例
    let inst = instances::create_instance(app.clone(), name.clone(), mc_version.clone(), "vanilla".into(), None, None)?;

    // 安装加载器
    let mut loader_kind = "vanilla";
    let mut loader_version: Option<String> = None;
    for (key, kind) in [
        ("fabric-loader", "fabric"),
        ("quilt-loader", "quilt"),
        ("neoforge", "neoforge"),
        ("forge", "forge"),
    ] {
        if let Some(v) = deps.get(key).and_then(|v| v.as_str()) {
            loader_kind = kind;
            loader_version = Some(v.to_string());
            break;
        }
    }
    // 先装原版本体（client jar + libraries + assets）。没有本体，加载器实例启动时
    // KnotClient 会 ClassNotFoundException（找不到 net.minecraft.client.main.Main）。
    emit(app, "下载游戏本体", 0, 3);
    crate::minecraft::mc_install_version(app.clone(), crate::paths::mc_root(&app, &inst).to_string_lossy().to_string(), mc_version.clone())
        .await?;

    install_loader_for_instance(app, &inst, loader_kind, loader_version.as_deref()).await?;

    // 下载 files
    let files = index.get("files").and_then(|f| f.as_array()).cloned().unwrap_or_default();
    emit(app, "下载模组文件", 0, files.len() as u64);
    let client = downloader::http_client();
    let mut tasks = vec![];
    for (i, f) in files.iter().enumerate() {
        let rel = f.get("path").and_then(|p| p.as_str()).unwrap_or("");
        if rel.is_empty() {
            continue;
        }
        let url = f
            .get("downloads")
            .and_then(|d| d.as_array())
            .and_then(|a| a.first())
            .and_then(|u| u.as_str())
            .unwrap_or("");
        if url.is_empty() {
            continue;
        }
        let sha1 = f
            .get("hashes")
            .and_then(|h| h.get("sha1"))
            .and_then(|s| s.as_str())
            .map(|s| s.to_string());
        let dest = Path::new(&inst.game_dir).join(rel);
        tasks.push(
            DlTask::new(&format!("pack-file-{i}"), rel, url, &dest).with_sha1(sha1),
        );
    }
    downloader::download_many(&client, app, tasks, 12).await?;
    emit(app, "下载模组文件", files.len() as u64, files.len() as u64);

    // 解压 overrides
    let overrides: Vec<String> = (0..archive.len())
        .filter_map(|i| archive.by_index(i).ok().map(|e| e.name().to_string()))
        .filter(|n| n.starts_with("overrides/"))
        .collect();
    emit(app, "解压配置", 0, overrides.len() as u64);
    for (i, name) in overrides.iter().enumerate() {
        let mut entry = archive.by_name(name).map_err(|e| e.to_string())?;
        if entry.is_dir() {
            continue;
        }
        let rel = name.trim_start_matches("overrides/");
        if rel.is_empty() {
            continue;
        }
        let dest = Path::new(&inst.game_dir).join(rel.replace('\\', "/"));
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).ok();
        }
        let mut out = fs::File::create(&dest).map_err(|e| e.to_string())?;
        std::io::copy(&mut entry, &mut out).map_err(|e| e.to_string())?;
        emit(app, "解压配置", i as u64 + 1, overrides.len() as u64);
    }

    Ok(inst)
}

async fn import_curseforge(
    app: &AppHandle,
    archive: &mut ZipArchive<fs::File>,
    name_override: Option<String>,
) -> Result<instances::Instance, String> {
    let mut manifest_file = archive
        .by_name("manifest.json")
        .map_err(|_| "缺少 manifest.json（CurseForge 整合包）")?;
    let mut text = String::new();
    manifest_file.read_to_string(&mut text).map_err(|e| e.to_string())?;
    drop(manifest_file); // 释放对 archive 的独占借用
    let manifest: CfManifest = serde_json::from_str(&text).map_err(|e| e.to_string())?;

    let name = name_override.unwrap_or(manifest.name.clone());
    let mc_version = manifest.minecraft.version.clone();
    let inst = instances::create_instance(
        app.clone(),
        name,
        mc_version.clone(),
        "vanilla".into(),
        None,
        None,
    )?;

    // modLoaders: [{id: "forge-47.2.0"} / "neoforge-21.11.42" / "fabric-0.16.9"]
    let mut loader_kind = "vanilla";
    let mut loader_version: Option<String> = None;
    if let Some(first) = manifest.modLoaders.first() {
        let id = &first.id;
        if let Some(rest) = id.strip_prefix("neoforge-") {
            loader_kind = "neoforge";
            loader_version = Some(rest.to_string());
        } else if let Some(rest) = id.strip_prefix("forge-") {
            loader_kind = "forge";
            loader_version = Some(rest.to_string());
        } else if let Some(rest) = id.strip_prefix("fabric-") {
            loader_kind = "fabric";
            loader_version = Some(rest.to_string());
        } else if let Some(rest) = id.strip_prefix("quilt-") {
            loader_kind = "quilt";
            loader_version = Some(rest.to_string());
        }
    }
    // 先装原版本体，理由同 mrpack 分支
    emit(app, "下载游戏本体", 0, 3);
    crate::minecraft::mc_install_version(app.clone(), crate::paths::mc_root(&app, &inst).to_string_lossy().to_string(), mc_version.clone())
        .await?;

    install_loader_for_instance(app, &inst, loader_kind, loader_version.as_deref()).await?;

    // 逐个解析模组文件并下载
    let mods = manifest.mods.clone();
    emit(app, "解析模组清单", 0, mods.len() as u64);
    let api_key = crate::settings::load(app).curseforge_api_key;
    let client = downloader::http_client();
    let mut tasks = vec![];
    for (i, m) in mods.iter().enumerate() {
        emit(app, "解析模组清单", i as u64 + 1, mods.len() as u64);
        if api_key.trim().is_empty() {
            // 无 API Key：跳过（可之后手动装）
            continue;
        }
        let url = format!("{}/v1/mods/{}/files/{}", "https://api.curseforge.com", m.projectID, m.fileID);
        let resp: Value = match client.get(&url).header("x-api-key", &api_key).send().await {
            Ok(r) => r.json().await.unwrap_or(Value::Null),
            Err(_) => Value::Null,
        };
        let download_url = resp
            .get("data")
            .and_then(|d| d.get("downloadUrl"))
            .and_then(|u| u.as_str())
            .unwrap_or("")
            .to_string();
        let file_name = resp
            .get("data")
            .and_then(|d| d.get("fileName"))
            .and_then(|u| u.as_str())
            .unwrap_or("")
            .to_string();
        if download_url.is_empty() || file_name.is_empty() {
            continue;
        }
        let dest = Path::new(&inst.game_dir).join("mods").join(&file_name);
        tasks.push(DlTask::new(
            &format!("cf-pack-{i}"),
            &file_name,
            &download_url,
            &dest,
        ));
    }
    emit(app, "下载模组", 0, tasks.len() as u64);
    if !tasks.is_empty() {
        downloader::download_many(&client, app, tasks, 8).await?;
    }
    emit(app, "下载模组", 1, 1);

    // overrides 解压
    let overrides: Vec<String> = (0..archive.len())
        .filter_map(|i| archive.by_index(i).ok().map(|e| e.name().to_string()))
        .filter(|n| n.starts_with("overrides/"))
        .collect();
    for name in overrides.iter() {
        let mut entry = archive.by_name(name).map_err(|e| e.to_string())?;
        if entry.is_dir() {
            continue;
        }
        let rel = name.trim_start_matches("overrides/");
        if rel.is_empty() {
            continue;
        }
        let dest = Path::new(&inst.game_dir).join(rel.replace('\\', "/"));
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).ok();
        }
        let mut out = fs::File::create(&dest).map_err(|e| e.to_string())?;
        std::io::copy(&mut entry, &mut out).map_err(|e| e.to_string())?;
    }

    Ok(inst)
}

/// 给新实例安装加载器，并回写 installed_version_id
async fn install_loader_for_instance(
    app: &AppHandle,
    inst: &instances::Instance,
    kind: &str,
    version: Option<&str>,
) -> Result<(), String> {
    let installed_id = match kind {
        "vanilla" => {
            minecraft::mc_install_version(
                app.clone(),
                crate::paths::mc_root(&app, &inst).to_string_lossy().to_string(),
                inst.minecraft_version.clone(),
            )
            .await?
        }
        "fabric" | "quilt" => {
            let v = version.ok_or(format!("整合包未指定 {kind} 版本"))?;
            minecraft::mc_install_loader(
                app.clone(),
                crate::paths::mc_root(&app, &inst).to_string_lossy().to_string(),
                inst.minecraft_version.clone(),
                kind.to_string(),
                v.to_string(),
            )
            .await?
        }
        "forge" | "neoforge" => {
            let v = version.ok_or(format!("整合包未指定 {kind} 版本"))?;
            minecraft::mc_install_forge(
                app.clone(),
                crate::paths::mc_root(&app, &inst).to_string_lossy().to_string(),
                inst.minecraft_version.clone(),
                kind.to_string(),
                v.to_string(),
            )
            .await?
        }
        _ => return Err(format!("未知加载器类型 {kind}")),
    };
    instances::update_instance(
        app.clone(),
        inst.id.clone(),
        instances::InstancePatch {
            name: None,
            max_memory: None,
            installed_version_id: Some(installed_id),
            loader_version: version.map(|v| v.to_string()),
            loader: Some(kind.to_string()),
            ..Default::default()
        },
    )?;
    Ok(())
}

#[derive(Debug, Deserialize, Clone)]
struct CfManifest {
    #[serde(default)]
    name: String,
    minecraft: CfMinecraft,
    #[serde(default)]
    modLoaders: Vec<CfModLoader>,
    #[serde(default)]
    mods: Vec<CfManifestMod>,
}

#[derive(Debug, Deserialize, Clone)]
struct CfMinecraft {
    version: String,
    #[serde(default)]
    #[allow(dead_code)]
    modLoaders: Vec<String>,
}

#[derive(Debug, Deserialize, Clone)]
struct CfModLoader {
    id: String,
}

#[derive(Debug, Deserialize, Clone)]
struct CfManifestMod {
    projectID: u64,
    fileID: u64,
}

// ---------------------------------------------------------------- 导出

/// 导出实例为 .mrpack（mods + config + options.txt 进 overrides）
#[tauri::command]
pub fn export_modpack(app: AppHandle, instance_id: String, dest_path: String) -> Result<String, String> {
    let inst = instances::get_instance(&app, &instance_id)?;
    let game = Path::new(&inst.game_dir);

    let mut deps = serde_json::Map::new();
    deps.insert("minecraft".into(), Value::String(inst.minecraft_version.clone()));
    match (inst.loader.as_str(), inst.loader_version.as_deref()) {
        ("fabric", Some(v)) => { deps.insert("fabric-loader".into(), Value::String(v.to_string())); }
        ("quilt", Some(v)) => { deps.insert("quilt-loader".into(), Value::String(v.to_string())); }
        ("forge", Some(v)) => { deps.insert("forge".into(), Value::String(v.to_string())); }
        ("neoforge", Some(v)) => { deps.insert("neoforge".into(), Value::String(v.to_string())); }
        _ => {}
    }
    let index = serde_json::json!({
        "formatVersion": 1,
        "name": inst.name,
        "versionId": "1.0.0",
        "files": [],
        "dependencies": Value::Object(deps),
    });

    let file = fs::File::create(&dest_path).map_err(|e| format!("创建文件失败: {e}"))?;
    let mut zip = ZipWriter::new(file);
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    zip.start_file("modrinth.index.json", opts)
        .map_err(|e| e.to_string())?;
    zip.write_all(serde_json::to_string_pretty(&index).unwrap().as_bytes())
        .map_err(|e| e.to_string())?;

    // overrides: mods/ config/ options.txt servers.dat
    let include_dirs = ["mods", "config", "resourcepacks", "shaderpacks"];
    for dir_name in include_dirs {
        let dir = game.join(dir_name);
        if !dir.exists() {
            continue;
        }
        add_dir_to_zip(&mut zip, &dir, &format!("overrides/{dir_name}"))?;
    }
    for f in ["options.txt", "servers.dat"] {
        let p = game.join(f);
        if p.exists() {
            zip.start_file(format!("overrides/{f}"), opts).map_err(|e| e.to_string())?;
            let data = fs::read(&p).map_err(|e| e.to_string())?;
            zip.write_all(&data).map_err(|e| e.to_string())?;
        }
    }
    zip.finish().map_err(|e| e.to_string())?;
    Ok(dest_path)
}

fn add_dir_to_zip(zip: &mut ZipWriter<fs::File>, dir: &Path, prefix: &str) -> Result<(), String> {
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    let entries = fs::read_dir(dir).map_err(|e| e.to_string())?;
    for entry in entries.flatten() {
        let path = entry.path();
        let name = format!("{}/{}", prefix, entry.file_name().to_string_lossy());
        if path.is_dir() {
            add_dir_to_zip(zip, &path, &name)?;
        } else {
            let data = fs::read(&path).map_err(|e| e.to_string())?;
            zip.start_file(name, opts).map_err(|e| e.to_string())?;
            zip.write_all(&data).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}
