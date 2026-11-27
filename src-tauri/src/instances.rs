//! 实例管理：CRUD、模组列表/启停/删除、游戏目录结构

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
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

    // ---- 以下为「版本设置」（对齐 PCL 的版本独立设置），全部可缺省 ----
    /// 最小内存 MB（-Xms），None = 不传
    #[serde(default)]
    pub min_memory: Option<u32>,
    /// 指定 Java 路径，None/空 = 自动选择
    #[serde(default)]
    pub java_path: Option<String>,
    /// 额外 JVM 参数（空格分隔），None = 回退全局设置
    #[serde(default)]
    pub jvm_args: Option<String>,
    /// 额外游戏参数（空格分隔）
    #[serde(default)]
    pub game_args: Option<String>,
    /// 窗口宽/高，None = 自动
    #[serde(default)]
    pub window_width: Option<u32>,
    #[serde(default)]
    pub window_height: Option<u32>,
    /// 全屏启动
    #[serde(default)]
    pub fullscreen: bool,
    /// 启动后自动加入服务器，形如 "mc.example.com" 或 "mc.example.com:25565"
    #[serde(default)]
    pub auto_join_server: Option<String>,
    /// 上次启动时间（RFC3339），主页按此排序「最近游玩」
    #[serde(default)]
    pub last_played: Option<String>,

    // ---- PCL CE 风格：.minecraft\versions\<文件夹名> 布局 ----
    /// 实例在 `<.minecraft>\versions\` 下的文件夹名；None 表示旧版布局
    /// （`%APPDATA%\com.caiwenjun.ccl\instances\<uuid>\game`）
    #[serde(default)]
    pub folder_name: Option<String>,
    /// 版本隔离的实例级覆盖：None/-1 = 跟随全局策略，1 = 强制隔离，2 = 强制不隔离
    #[serde(default)]
    pub version_isolation: Option<i32>,
}

impl Instance {
    /// 实例文件夹的绝对路径（新布局），旧布局为 None
    pub fn folder_path(&self, app: &AppHandle) -> Option<PathBuf> {
        self.folder_name
            .as_deref()
            .map(|n| crate::paths::instance_folder(app, n))
    }

    /// `--gameDir`：由版本隔离策略推导
    pub fn game_dir_path(&self, app: &AppHandle) -> PathBuf {
        crate::paths::resolve_game_dir(app, self)
    }

    /// versions / libraries / assets 所在的 MC 根
    pub fn mc_root(&self, app: &AppHandle) -> PathBuf {
        crate::paths::mc_root(app, self)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ModInfo {
    pub file_name: String,
    pub enabled: bool,
    pub size: u64,
    /// 项目名（从 jar 文件名推断，仅展示）
    pub project_name: String,
    /// jar 内声明的模组名（fabric.mod.json / quilt.mod.json / mods.toml / mcmod.info）
    #[serde(default)]
    pub mod_name: Option<String>,
    #[serde(default)]
    pub mod_version: Option<String>,
    #[serde(default)]
    pub mod_id: Option<String>,
}

/// 旧布局（v0.x）：%APPDATA%\com.caiwenjun.ccl\instances\<uuid>
fn instance_dir(app: &AppHandle, id: &str) -> PathBuf {
    settings::instances_dir(app).join(id)
}

fn instance_json(app: &AppHandle, id: &str) -> PathBuf {
    instance_dir(app, id).join("instance.json")
}

/// 扫描一处目录下的 instance.json
fn collect_instances(root: &Path, out: &mut Vec<Instance>, seen: &mut std::collections::HashSet<String>) {
    if let Ok(entries) = fs::read_dir(root) {
        for entry in entries.flatten() {
            let json = entry.path().join("instance.json");
            if let Ok(text) = fs::read_to_string(&json) {
                if let Ok(inst) = serde_json::from_str::<Instance>(&text) {
                    if seen.insert(inst.id.clone()) {
                        out.push(inst);
                    }
                }
            }
        }
    }
}

/// 读取全部实例：新布局（.minecraft\versions\*\CCL）+ 旧布局（instances\<uuid>）
pub fn load_instances(app: &AppHandle) -> Vec<Instance> {
    let mut out = vec![];
    let mut seen = std::collections::HashSet::new();

    // 新布局：<.minecraft>\versions\<文件夹名>\CCL\instance.json
    let versions = crate::paths::versions_root(app);
    if let Ok(entries) = fs::read_dir(&versions) {
        for entry in entries.flatten() {
            let json = entry.path().join("CCL").join("instance.json");
            if let Ok(text) = fs::read_to_string(&json) {
                if let Ok(inst) = serde_json::from_str::<Instance>(&text) {
                    if seen.insert(inst.id.clone()) {
                        out.push(inst);
                    }
                }
            }
        }
    }
    // 旧布局：兼容升级前创建的实例
    collect_instances(&settings::instances_dir(app), &mut out, &mut seen);

    // 归一化：把 game_dir 刷新成「版本隔离」策略当前推导出的真实路径。
    // 这样全局隔离设置改动后，所有按 inst.game_dir 读取游戏数据的代码无需感知隔离逻辑。
    for inst in out.iter_mut() {
        if inst.folder_name.is_some() {
            let resolved = crate::paths::resolve_game_dir(app, inst);
            if resolved.to_string_lossy() != inst.game_dir {
                inst.game_dir = resolved.to_string_lossy().to_string();
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
    let text = serde_json::to_string_pretty(inst).map_err(|e| e.to_string())?;
    match inst.folder_name.as_deref() {
        // 新布局：<实例文件夹>\CCL\instance.json
        Some(folder) => {
            let dir = crate::paths::ccl_meta_dir(app, folder);
            fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            fs::write(dir.join("instance.json"), text).map_err(|e| e.to_string())?;
        }
        // 旧布局：<instances>\<uuid>\instance.json
        None => {
            let dir = instance_dir(app, &inst.id);
            fs::create_dir_all(dir.join("game")).map_err(|e| e.to_string())?;
            fs::write(instance_json(app, &inst.id), text).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// 供其他模块回写实例（如启动后记录 last_played）
pub fn save_instance(app: &AppHandle, inst: &Instance) -> Result<(), String> {
    write_instance(app, inst)
}

#[tauri::command]
pub fn list_instances(app: AppHandle) -> Vec<Instance> {
    load_instances(&app)
}

/// 生成一个不与现有实例重名的名字：1.21.11 → 1.21.11 (2) → 1.21.11 (3)…
pub fn unique_name(app: &AppHandle, base: &str) -> String {
    let base = base.trim();
    let base = if base.is_empty() { "新实例" } else { base };
    let taken: Vec<String> = load_instances(app)
        .into_iter()
        .map(|i| i.name.to_lowercase())
        .collect();
    if !taken.iter().any(|n| n == &base.to_lowercase()) {
        return base.to_string();
    }
    for n in 2..999 {
        let cand = format!("{base} ({n})");
        if !taken.iter().any(|t| t == &cand.to_lowercase()) {
            return cand;
        }
    }
    format!("{base} ({})", Uuid::new_v4())
}

/// 创建实例（非命令内部实现）：建目录 + 写 instance.json
pub fn add_instance(
    app: &AppHandle,
    name: &str,
    minecraft_version: &str,
    loader: &str,
    loader_version: Option<String>,
    max_memory: Option<u32>,
) -> Result<Instance, String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("实例名不能为空".into());
    }
    let id = Uuid::new_v4().to_string();
    // PCL CE 布局：Minecraft 文件夹 + versions\<实例文件夹名>
    let root = crate::paths::ensure_folder_layout(app);
    let folder_name = crate::paths::unique_folder_name(app, &name);
    let game_dir = crate::paths::instance_folder(app, &folder_name);
    fs::create_dir_all(&game_dir).map_err(|e| e.to_string())?;
    for sub in ["mods", "config", "saves", "resourcepacks", "screenshots"] {
        fs::create_dir_all(game_dir.join(sub)).ok();
    }
    let inst = Instance {
        id,
        name,
        minecraft_version: minecraft_version.to_string(),
        loader: loader.to_string(),
        loader_version,
        installed_version_id: None,
        created_at: chrono::Utc::now().to_rfc3339(),
        max_memory,
        game_dir: game_dir.to_string_lossy().to_string(),
        min_memory: None,
        java_path: None,
        jvm_args: None,
        game_args: None,
        window_width: None,
        window_height: None,
        fullscreen: false,
        auto_join_server: None,
        last_played: None,
        folder_name: Some(folder_name),
        version_isolation: None,
    };
    write_instance(app, &inst)?;
    let _ = root;
    Ok(inst)
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
    add_instance(&app, &name, &minecraft_version, &loader, loader_version, max_memory)
}

/// 删除实例目录（下载失败时回滚刚建的实例用）
pub fn remove_instance_dir(app: &AppHandle, id: &str) {
    if let Ok(inst) = get_instance(app, id) {
        if let Some(dir) = inst.folder_path(app) {
            let _ = fs::remove_dir_all(&dir);
            return;
        }
    }
    let dir = instance_dir(app, id);
    if dir.exists() {
        let _ = fs::remove_dir_all(&dir);
    }
}

#[derive(Debug, Deserialize, Default)]
pub struct InstancePatch {
    pub name: Option<String>,
    pub max_memory: Option<u32>,
    pub installed_version_id: Option<String>,
    pub loader_version: Option<String>,
    pub loader: Option<String>,
    /// 以下为版本设置项；字符串传空串表示清空（→ None）
    pub min_memory: Option<u32>,
    pub java_path: Option<String>,
    pub jvm_args: Option<String>,
    pub game_args: Option<String>,
    pub window_width: Option<u32>,
    pub window_height: Option<u32>,
    pub fullscreen: Option<bool>,
    pub auto_join_server: Option<String>,
    /// 版本隔离覆盖：-1/None 跟随全局，1 强制隔离，2 强制不隔离（此处用 0 表示跟随全局）
    pub version_isolation: Option<i32>,
}

/// 空串 / 纯空白 → None，用于「清空该项」语义
fn blank_to_none(s: String) -> Option<String> {
    let t = s.trim().to_string();
    if t.is_empty() {
        None
    } else {
        Some(t)
    }
}

#[tauri::command]
pub fn update_instance(app: AppHandle, id: String, patch: InstancePatch) -> Result<Instance, String> {
    let mut inst = get_instance(&app, &id)?;
    if let Some(name) = patch.name.filter(|n| !n.trim().is_empty()) {
        let new_name = name.trim().to_string();
        // PCL 行为：改名同步重命名实例文件夹；目标已存在则保留原文件夹名
        if let Some(old_folder) = inst.folder_name.clone() {
            let want = crate::paths::sanitize_folder_name(&new_name);
            if want != old_folder {
                let from = crate::paths::instance_folder(&app, &old_folder);
                let to = crate::paths::instance_folder(&app, &want);
                if from.exists() && !to.exists() {
                    if fs::rename(&from, &to).is_ok() {
                        inst.folder_name = Some(want);
                        inst.game_dir = to.to_string_lossy().to_string();
                    }
                }
            }
        }
        inst.name = new_name;
    }
    if let Some(v) = patch.version_isolation {
        inst.version_isolation = match v {
            1 => Some(1),
            2 => Some(2),
            _ => None,
        };
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
    if let Some(v) = patch.loader {
        inst.loader = v;
    }
    // ---- 版本设置
    if let Some(m) = patch.min_memory {
        inst.min_memory = if m == 0 { None } else { Some(m) };
    }
    if let Some(v) = patch.java_path {
        inst.java_path = blank_to_none(v);
    }
    if let Some(v) = patch.jvm_args {
        inst.jvm_args = blank_to_none(v);
    }
    if let Some(v) = patch.game_args {
        inst.game_args = blank_to_none(v);
    }
    if let Some(w) = patch.window_width {
        inst.window_width = if w == 0 { None } else { Some(w) };
    }
    if let Some(h) = patch.window_height {
        inst.window_height = if h == 0 { None } else { Some(h) };
    }
    if let Some(f) = patch.fullscreen {
        inst.fullscreen = f;
    }
    if let Some(v) = patch.auto_join_server {
        inst.auto_join_server = blank_to_none(v);
    }
    write_instance(&app, &inst)?;
    Ok(inst)
}

#[tauri::command]
pub fn delete_instance(app: AppHandle, id: String, remove_files: Option<bool>) -> Result<(), String> {
    let inst = get_instance(&app, &id)?;
    let full = remove_files.unwrap_or(true);
    match inst.folder_path(&app) {
        // 新布局：默认只删实例文件夹（游戏数据）；实例元数据也在这里，一起删掉
        Some(dir) => {
            if full && dir.exists() {
                fs::remove_dir_all(&dir).map_err(|e| format!("删除实例失败: {e}"))?;
            } else if !full {
                // 只删启动器私有数据，保留存档与模组
                let meta = crate::paths::ccl_meta_dir(&app, inst.folder_name.as_deref().unwrap_or(""));
                if meta.exists() {
                    fs::remove_dir_all(&meta).map_err(|e| format!("删除实例数据失败: {e}"))?;
                }
            }
        }
        None => {
            let dir = instance_dir(&app, &id);
            if full && dir.exists() {
                fs::remove_dir_all(&dir).map_err(|e| format!("删除实例失败: {e}"))?;
            }
        }
    }
    Ok(())
}

/// 把旧版布局（instances\<uuid>\game）的实例迁进 .minecraft\versions\<文件夹名>
///
/// 同盘用 rename（瞬间完成），跨盘退化为复制。迁移后原目录会被删除。
#[tauri::command]
pub fn migrate_instances_to_mc_folder(app: AppHandle) -> Result<usize, String> {
    let root = crate::paths::ensure_folder_layout(&app);
    let legacy = settings::instances_dir(&app);
    let mut moved = 0usize;

    let entries: Vec<_> = fs::read_dir(&legacy)
        .map(|rd| rd.flatten().collect::<Vec<_>>())
        .unwrap_or_default();

    for entry in entries {
        let src_dir = entry.path();
        let json = src_dir.join("instance.json");
        let Ok(text) = fs::read_to_string(&json) else {
            continue;
        };
        let Ok(mut inst) = serde_json::from_str::<Instance>(&text) else {
            continue;
        };
        if inst.folder_name.is_some() {
            continue; // 已经是新布局（理论上不会出现在旧目录里）
        }

        let folder_name = crate::paths::unique_folder_name(&app, &inst.name);
        let dest = crate::paths::instance_folder(&app, &folder_name);
        let src_game = src_dir.join("game");

        // 目标存在同名则换个名字继续，避免覆盖已有数据
        if dest.exists() {
            continue;
        }
        fs::create_dir_all(&dest).map_err(|e| format!("创建实例文件夹失败: {e}"))?;

        if src_game.exists() {
            if let Err(e) = move_dir_contents(&src_game, &dest) {
                let _ = fs::remove_dir_all(&dest);
                return Err(format!("迁移「{}」失败: {e}", inst.name));
            }
        }

        inst.folder_name = Some(folder_name);
        inst.game_dir = dest.to_string_lossy().to_string();
        write_instance(&app, &inst)?;

        // 元数据写成功后才删旧目录（含旧 instance.json）
        let _ = fs::remove_dir_all(&src_dir);
        moved += 1;
    }

    let _ = root;
    Ok(moved)
}

/// 把 src 下的内容整体搬到 dest，优先 rename（同盘是原子操作），跨盘则复制
fn move_dir_contents(src: &Path, dest: &Path) -> Result<(), String> {
    for entry in fs::read_dir(src).map_err(|e| e.to_string())?.flatten() {
        let from = entry.path();
        let to = dest.join(entry.file_name());
        if from.is_dir() && to.exists() {
            // 目标已有同名目录（不太可能），合并进去
            move_dir_contents(&from, &to)?;
            let _ = fs::remove_dir_all(&from);
            continue;
        }
        if let Ok(_) = fs::rename(&from, &to) {
            continue;
        }
        if from.is_dir() {
            fs::create_dir_all(&to).map_err(|e| e.to_string())?;
            move_dir_contents(&from, &to)?;
            let _ = fs::remove_dir_all(&from);
        } else {
            fs::copy(&from, &to).map_err(|e| format!("复制 {:?} 失败: {e}", entry.file_name()))?;
            let _ = fs::remove_file(&from);
        }
    }
    Ok(())
}

/// 查询某实例当前生效的游戏目录与隔离状态（给界面显示）
#[tauri::command]
pub fn instance_paths(
    app: AppHandle,
    id: String,
) -> Result<InstancePaths, String> {
    let inst = get_instance(&app, &id)?;
    Ok(InstancePaths {
        mc_root: crate::paths::mc_root(&app, &inst).to_string_lossy().to_string(),
        game_dir: crate::paths::resolve_game_dir(&app, &inst)
            .to_string_lossy()
            .to_string(),
        instance_folder: inst
            .folder_path(&app)
            .map(|p| p.to_string_lossy().to_string()),
        isolation_label: crate::paths::isolation_label(&app, &inst),
        isolated: crate::paths::isolation_enabled(&app, &inst),
        legacy: inst.folder_name.is_none(),
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct InstancePaths {
    /// .minecraft 根（versions / libraries / assets 所在地）
    pub mc_root: String,
    /// 传给 --gameDir 的路径
    pub game_dir: String,
    /// 实例文件夹（新布局）
    pub instance_folder: Option<String>,
    pub isolation_label: String,
    pub isolated: bool,
    /// 是否为旧版布局（可迁移）
    pub legacy: bool,
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

/// jar 内声明的元数据，读不到就全 None
#[derive(Debug, Default, Clone)]
struct ModMeta {
    id: Option<String>,
    name: Option<String>,
    version: Option<String>,
}

impl ModMeta {
    fn is_empty(&self) -> bool {
        self.id.is_none() && self.name.is_none() && self.version.is_none()
    }
}

/// key = jar 路径，value = (文件大小, 元数据)。大小变了视为内容变了，重新读。
fn meta_cache() -> &'static std::sync::Mutex<std::collections::HashMap<String, (u64, ModMeta)>> {
    static CACHE: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<String, (u64, ModMeta)>>> =
        std::sync::OnceLock::new();
    CACHE.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

fn non_empty(s: Option<String>) -> Option<String> {
    s.map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty() && !v.contains("${"))
}

/// 宽容解析：部分模组的 mod.json 里有裸控制字符（未转义的换行/制表符），
/// serde_json 严格模式会整份拒绝，而 Fabric 用的 Gson 是宽容的。
/// 这里把字符串字面量内的裸控制字符转义后重试一次。
fn lenient_json(s: &str) -> Option<serde_json::Value> {
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(s) {
        return Some(v);
    }
    let mut out = String::with_capacity(s.len() + 32);
    let mut in_str = false;
    let mut esc = false;
    for ch in s.chars() {
        if in_str {
            if esc {
                out.push(ch);
                esc = false;
                continue;
            }
            match ch {
                '\\' => {
                    out.push(ch);
                    esc = true;
                }
                '"' => {
                    out.push(ch);
                    in_str = false;
                }
                c if (c as u32) < 0x20 => match c {
                    '\n' => out.push_str("\\n"),
                    '\r' => out.push_str("\\r"),
                    '\t' => out.push_str("\\t"),
                    _ => out.push_str(&format!("\\u{:04x}", c as u32)),
                },
                c => out.push(c),
            }
        } else {
            if ch == '"' {
                in_str = true;
            }
            out.push(ch);
        }
    }
    serde_json::from_str(&out).ok()
}

fn json_mod(s: &str, id_key: &str, name_key: &str, ver_key: &str) -> Option<ModMeta> {
    let v = lenient_json(s)?;
    Some(ModMeta {
        id: non_empty(v.get(id_key)?.as_str().map(String::from)),
        name: non_empty(v.get(name_key).and_then(|x| x.as_str()).map(String::from)),
        version: non_empty(v.get(ver_key).and_then(|x| x.as_str()).map(String::from)),
    })
}

/// mods.toml / neoforge.mods.toml 的极简取值：取第一个 key = "value" 行
fn toml_field(text: &str, key: &str) -> Option<String> {
    for line in text.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix(key) {
            if let Some(r) = rest.trim_start().strip_prefix('=') {
                let v = r.trim().trim_matches('"').trim_matches('\'').to_string();
                if let Some(v) = non_empty(Some(v)) {
                    return Some(v);
                }
            }
        }
    }
    None
}

fn read_mod_meta(path: &std::path::Path) -> ModMeta {
    use std::io::Read;

    let file = match fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return ModMeta::default(),
    };
    let mut zip = match zip::ZipArchive::new(file) {
        Ok(z) => z,
        Err(_) => return ModMeta::default(),
    };

    // Fabric / Quilt（Quilt 兼容层也会带 fabric.mod.json）
    for entry_name in ["fabric.mod.json", "quilt.mod.json"] {
        if let Ok(mut entry) = zip.by_name(entry_name) {
            let mut s = String::new();
            if entry.read_to_string(&mut s).is_ok() {
                if entry_name == "fabric.mod.json" {
                    if let Some(m) = json_mod(&s, "id", "name", "version") {
                        if !m.is_empty() {
                            return m;
                        }
                    }
                } else if let Some(v) = lenient_json(&s) {
                    if let Some(loader) = v.get("quilt_loader") {
                        let m = ModMeta {
                            id: non_empty(loader.get("id").and_then(|x| x.as_str()).map(String::from)),
                            name: non_empty(
                                loader
                                    .get("metadata")
                                    .and_then(|x| x.get("name"))
                                    .and_then(|x| x.as_str())
                                    .map(String::from),
                            ),
                            version: non_empty(
                                loader.get("version").and_then(|x| x.as_str()).map(String::from),
                            ),
                        };
                        if !m.is_empty() {
                            return m;
                        }
                    }
                }
            }
        }
    }

    // Forge / NeoForge
    for entry_name in ["META-INF/neoforge.mods.toml", "META-INF/mods.toml"] {
        if let Ok(mut entry) = zip.by_name(entry_name) {
            let mut s = String::new();
            if entry.read_to_string(&mut s).is_ok() {
                let m = ModMeta {
                    id: toml_field(&s, "modId"),
                    name: toml_field(&s, "displayName"),
                    version: toml_field(&s, "version"),
                };
                if !m.is_empty() {
                    return m;
                }
            }
        }
    }

    // 远古 Forge
    if let Ok(mut entry) = zip.by_name("mcmod.info") {
        let mut s = String::new();
        if entry.read_to_string(&mut s).is_ok() {
            if let Some(v) = lenient_json(&s) {
                let first = match &v {
                    serde_json::Value::Array(a) => a.first().cloned(),
                    serde_json::Value::Object(o) => o
                        .get("modList")
                        .and_then(|l| l.as_array())
                        .and_then(|a| a.first())
                        .cloned()
                        .or_else(|| Some(v.clone())),
                    _ => None,
                };
                if let Some(f) = first {
                    let m = ModMeta {
                        id: non_empty(f.get("modid").and_then(|x| x.as_str()).map(String::from)),
                        name: non_empty(f.get("name").and_then(|x| x.as_str()).map(String::from)),
                        version: non_empty(f.get("version").and_then(|x| x.as_str()).map(String::from)),
                    };
                    if !m.is_empty() {
                        return m;
                    }
                }
            }
        }
    }

    ModMeta::default()
}

fn cached_mod_meta(path: &std::path::Path, size: u64) -> ModMeta {
    let key = path.to_string_lossy().to_string();
    if let Ok(cache) = meta_cache().lock() {
        if let Some((cached_size, meta)) = cache.get(&key) {
            if *cached_size == size {
                return meta.clone();
            }
        }
    }
    let meta = read_mod_meta(path);
    if let Ok(mut cache) = meta_cache().lock() {
        cache.insert(key, (size, meta.clone()));
    }
    meta
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
            let path = entry.path();
            let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
            let enabled = name.ends_with(".jar");
            let project_name = name
                .trim_end_matches(".jar.disabled")
                .trim_end_matches(".jar")
                .to_string();
            // 禁用态文件名带 .disabled，仍按同路径缓存，避免启停后重复解析
            let meta = cached_mod_meta(&path, size);
            out.push(ModInfo {
                file_name: name,
                enabled,
                size,
                project_name,
                mod_name: meta.name,
                mod_version: meta.version,
                mod_id: meta.id,
            });
        }
    }
    out.sort_by(|a, b| {
        a.mod_name
            .as_deref()
            .unwrap_or(&a.project_name)
            .to_lowercase()
            .cmp(&b.mod_name.as_deref().unwrap_or(&b.project_name).to_lowercase())
    });
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
