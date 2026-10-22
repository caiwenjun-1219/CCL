//! 实例管理：CRUD、模组列表/启停/删除、游戏目录结构

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tauri::AppHandle;
use tauri_plugin_opener::OpenerExt;
use uuid::Uuid;

use crate::settings;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Instance {
    pub id: String,
    pub name: String,
    /// MC 版本, 如 "1.21.11"
    pub minecraft_version: String,
    /// vanilla / fabric / quilt / forge / neoforge
    pub loader: String,
    /// 加载器版本（fabric loader 版本号 / forge 版本号），vanilla 为 None
    #[serde(default)]
    pub loader_version: Option<String>,
    /// 实际启动的版本目录 id（安装加载器后由安装器决定）
    #[serde(default)]
    pub installed_version_id: Option<String>,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub max_memory: Option<u32>,
    /// 游戏目录（绝对路径）
    pub game_dir: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModInfo {
    pub file_name: String,
    pub enabled: bool,
    pub size: u64,
    /// 项目名（从 jar 文件名推断，仅展示）
    pub project_name: String,
}

fn instance_dir(app: &AppHandle, id: &str) -> PathBuf {
    settings::instances_dir(app).join(id)
}

fn instance_json(app: &AppHandle, id: &str) -> PathBuf {
    instance_dir(app, id).join("instance.json")
}

pub fn load_instances(app: &AppHandle) -> Vec<Instance> {
    let mut out = vec![];
    let root = settings::instances_dir(app);
    if let Ok(entries) = fs::read_dir(&root) {
        for entry in entries.flatten() {
            let json = entry.path().join("instance.json");
            if let Ok(text) = fs::read_to_string(&json) {
                if let Ok(inst) = serde_json::from_str::<Instance>(&text) {
                    out.push(inst);
                }
            }
        }
    }
    out.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    out
}

pub fn get_instance(app: &AppHandle, id: &str) -> Result<Instance, String> {
    load_instances(app)
        .into_iter()
        .find(|i| i.id == id)
        .ok_or_else(|| "实例不存在".to_string())
}

fn write_instance(app: &AppHandle, inst: &Instance) -> Result<(), String> {
    let dir = instance_dir(app, &inst.id);
    fs::create_dir_all(dir.join("game")).map_err(|e| e.to_string())?;
    let text = serde_json::to_string_pretty(inst).map_err(|e| e.to_string())?;
    fs::write(instance_json(app, &inst.id), text).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn list_instances(app: AppHandle) -> Vec<Instance> {
    load_instances(&app)
}

#[tauri::command]
pub fn create_instance(
    app: AppHandle,
    name: String,
    minecraft_version: String,
    loader: String,
    loader_version: Option<String>,
    max_memory: Option<u32>,
) -> Result<Instance, String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("实例名不能为空".into());
    }
    let id = Uuid::new_v4().to_string();
    let game_dir = instance_dir(&app, &id).join("game");
    fs::create_dir_all(&game_dir).map_err(|e| e.to_string())?;
    for sub in ["mods", "config", "saves", "resourcepacks", "screenshots"] {
        fs::create_dir_all(game_dir.join(sub)).ok();
    }
    let inst = Instance {
        id,
        name,
        minecraft_version,
        loader,
        loader_version,
        installed_version_id: None,
        created_at: chrono::Utc::now().to_rfc3339(),
        max_memory,
        game_dir: game_dir.to_string_lossy().to_string(),
    };
    write_instance(&app, &inst)?;
    Ok(inst)
}

#[derive(Debug, Deserialize, Default)]
pub struct InstancePatch {
    pub name: Option<String>,
    pub max_memory: Option<u32>,
    pub installed_version_id: Option<String>,
    pub loader_version: Option<String>,
}

#[tauri::command]
pub fn update_instance(app: AppHandle, id: String, patch: InstancePatch) -> Result<Instance, String> {
    let mut inst = get_instance(&app, &id)?;
    if let Some(name) = patch.name.filter(|n| !n.trim().is_empty()) {
        inst.name = name.trim().to_string();
    }
    if let Some(m) = patch.max_memory {
        inst.max_memory = Some(m);
    }
    if let Some(v) = patch.installed_version_id {
        inst.installed_version_id = Some(v);
    }
    if let Some(v) = patch.loader_version {
        inst.loader_version = Some(v);
    }
    write_instance(&app, &inst)?;
    Ok(inst)
}

#[tauri::command]
pub fn delete_instance(app: AppHandle, id: String) -> Result<(), String> {
    let dir = instance_dir(&app, &id);
    if dir.exists() {
        fs::remove_dir_all(&dir).map_err(|e| format!("删除实例失败: {e}"))?;
    }
    Ok(())
}

/// 列出实例截图（按修改时间新→旧，最多 24 张，仅返回文件名）
/// 前端通过 ccl://instances/<id>/screenshots/<文件名> 直接加载图片
#[tauri::command]
pub fn list_screenshots(app: AppHandle, instance_id: String) -> Result<Vec<String>, String> {
    let inst = get_instance(&app, &instance_id)?;
    let dir = std::path::Path::new(&inst.game_dir).join("screenshots");
    let mut entries: Vec<(std::time::SystemTime, String)> = Vec::new();
    if let Ok(rd) = fs::read_dir(&dir) {
        for e in rd.flatten() {
            let p = e.path();
            if !p.is_file() {
                continue;
            }
            let ext = p
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if !["png", "jpg", "jpeg", "webp", "gif"].contains(&ext.as_str()) {
                continue;
            }
            let mtime = e
                .metadata()
                .and_then(|m| m.modified())
                .unwrap_or(std::time::UNIX_EPOCH);
            if let Some(name) = p.file_name().and_then(|s| s.to_str()) {
                entries.push((mtime, name.to_string()));
            }
        }
    }
    entries.sort_by(|a, b| b.0.cmp(&a.0));
    Ok(entries.into_iter().take(24).map(|(_, n)| n).collect())
}

#[tauri::command]
pub fn open_instance_dir(app: AppHandle, id: String) -> Result<(), String> {
    let inst = get_instance(&app, &id)?;
    app.opener()
        .open_path(&inst.game_dir, None::<String>)
        .map_err(|e| e.to_string())
}

// ---------------------------------------------------------------- 模组管理

fn mods_dir(inst: &Instance) -> PathBuf {
    PathBuf::from(&inst.game_dir).join("mods")
}

#[tauri::command]
pub fn list_mods(app: AppHandle, instance_id: String) -> Result<Vec<ModInfo>, String> {
    let inst = get_instance(&app, &instance_id)?;
    let dir = mods_dir(&inst);
    let mut out = vec![];
    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if !(name.ends_with(".jar") || name.ends_with(".jar.disabled")) {
                continue;
            }
            let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
            let enabled = name.ends_with(".jar");
            let project_name = name
                .trim_end_matches(".jar.disabled")
                .trim_end_matches(".jar")
                .to_string();
            out.push(ModInfo {
                file_name: name,
                enabled,
                size,
                project_name,
            });
        }
    }
    out.sort_by(|a, b| a.file_name.cmp(&b.file_name));
    Ok(out)
}

#[tauri::command]
pub fn toggle_mod(app: AppHandle, instance_id: String, file_name: String, enabled: bool) -> Result<(), String> {
    let inst = get_instance(&app, &instance_id)?;
    let src = mods_dir(&inst).join(&file_name);
    let dst = if enabled {
        mods_dir(&inst).join(file_name.trim_end_matches(".disabled"))
    } else {
        mods_dir(&inst).join(format!("{file_name}.disabled"))
    };
    fs::rename(&src, &dst).map_err(|e| format!("切换失败: {e}"))?;
    Ok(())
}

#[tauri::command]
pub fn delete_mod(app: AppHandle, instance_id: String, file_name: String) -> Result<(), String> {
    let inst = get_instance(&app, &instance_id)?;
    let path = mods_dir(&inst).join(&file_name);
    if path.exists() {
        fs::remove_file(&path).map_err(|e| format!("删除失败: {e}"))?;
    }
    Ok(())
}

/// 安装模组文件（复制到 mods 目录）
#[tauri::command]
pub fn install_mod_file(app: AppHandle, instance_id: String, src_path: String) -> Result<String, String> {
    let inst = get_instance(&app, &instance_id)?;
    let src = PathBuf::from(&src_path);
    let file_name = src
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .ok_or("无效文件路径")?;
    let dst = mods_dir(&inst).join(&file_name);
    fs::copy(&src, &dst).map_err(|e| format!("复制模组失败: {e}"))?;
    Ok(file_name)
}

#[tauri::command]
pub fn open_mods_dir(app: AppHandle, instance_id: String) -> Result<(), String> {
    let inst = get_instance(&app, &instance_id)?;
    let dir = mods_dir(&inst);
    fs::create_dir_all(&dir).ok();
    app.opener()
        .open_path(dir.to_string_lossy().to_string(), None::<String>)
        .map_err(|e| e.to_string())
}
