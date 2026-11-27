//! PCL CE 风格的目录模型
//!
//! 参照 PCL CE 的 `ModFolder` / `McInstance.PathIndie` 设计：
//!
//! ```text
//! <Minecraft 文件夹>\                默认 %APPDATA%\.minecraft，可切换/自定义
//! ├─ launcher_profiles.json          自动创建，让官启也能识别这个目录
//! ├─ assets\                         共享资源索引与对象
//! ├─ libraries\                      共享库
//! ├─ versions\
//! │  ├─ 26.2\                        原版本体（共享，多个实例可复用）
//! │  ├─ fabric-loader-0.19.5-26.2\   加载器版本（共享）
//! │  └─ 我的生存档\                   实例文件夹（PCL 的 PathInstance）
//! │     ├─ CCL\instance.json         启动器私有数据
//! │     ├─ saves\                    版本隔离开启时的游戏数据
//! │     ├─ mods\
//! │     └─ resourcepacks\
//! ├─ saves\                          版本隔离关闭时的游戏数据
//! └─ mods\
//! ```
//!
//! `--gameDir` 的取值就是「版本隔离」的结果：
//! 隔离 → 实例文件夹；不隔离 → Minecraft 文件夹根。
//!
//! 与 PCL CE 的一处差异：PCL 把版本 jar/json 也放进实例文件夹（每个实例一份 jar），
//! CCL 沿用了共享 `versions\<版本 id>\` 的做法，避免重复下载。

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};

use crate::instances::Instance;
use crate::settings;

// ------------------------------------------------------------------ 候选文件夹

/// 一个可选的 Minecraft 文件夹（对应 PCL CE 的 `ModFolder.McFolder`）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameFolder {
    /// 显示名，如「启动器文件夹」「官方启动器文件夹」
    pub name: String,
    /// 绝对路径，统一以分隔符结尾
    pub location: String,
    /// original（自动扫描到的）/ custom（用户添加的）
    pub kind: String,
}

/// Windows 文件名非法字符
const ILLEGAL: &[char] = &['<', '>', ':', '"', '/', '\\', '|', '?', '*'];

/// 把实例名转成安全的文件夹名（PCL CE 依赖 FolderNameValidator 做同样的事）
pub fn sanitize_folder_name(name: &str) -> String {
    let mut s: String = name
        .trim()
        .chars()
        .filter(|c| !ILLEGAL.contains(c) && !c.is_control())
        .collect();
    // 去掉结尾的点与空格（Windows 会自动去掉，会导致路径对不上）
    while s.ends_with('.') || s.ends_with(' ') {
        s.pop();
    }
    // Windows 保留设备名
    let upper = s.to_uppercase();
    let stem = upper.split('.').next().unwrap_or("");
    if matches!(
        stem,
        "CON" | "PRN" | "AUX" | "NUL" | "COM1" | "COM2" | "COM3" | "COM4" | "COM5" | "COM6"
        | "COM7" | "COM8" | "COM9" | "LPT1" | "LPT2" | "LPT3" | "LPT4" | "LPT5" | "LPT6"
        | "LPT7" | "LPT8" | "LPT9"
    ) {
        s.insert(0, '_');
    }
    if s.is_empty() {
        s = "Minecraft".to_string();
    }
    // 过长会撞上 260 路径限制，截到 60 字符
    if s.chars().count() > 60 {
        s = s.chars().take(60).collect();
        while s.ends_with('.') || s.ends_with(' ') {
            s.pop();
        }
    }
    s
}

/// 默认 Minecraft 文件夹：Windows 为 %APPDATA%\.minecraft（与官方启动器一致）；
/// Android 落在应用私有数据目录（免存储权限）
pub fn default_game_folder(app: &AppHandle) -> PathBuf {
    if let Ok(appdata) = std::env::var("APPDATA") {
        return PathBuf::from(appdata).join(".minecraft");
    }
    #[cfg(target_os = "android")]
    {
        if let Ok(p) = app.path().data_dir() {
            return p.join(".minecraft");
        }
    }
    dirs_home().join("AppData").join("Roaming").join(".minecraft")
}

fn dirs_home() -> PathBuf {
    std::env::var("USERPROFILE")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}

/// 启动器可执行文件所在目录
pub fn exe_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
}

// ------------------------------------------------------------------ 当前选择的文件夹

/// 当前生效的 Minecraft 文件夹（对应 PCL CE 的 `ModFolder.mcFolderSelected`）
pub fn mc_folder(app: &AppHandle) -> PathBuf {
    let s = settings::load(app);
    let raw = s.game_folder.trim();
    if raw.is_empty() {
        default_game_folder(app)
    } else {
        PathBuf::from(raw)
    }
}

pub fn versions_root(app: &AppHandle) -> PathBuf {
    mc_folder(app).join("versions")
}

/// 实例文件夹：`<.minecraft>\versions\<文件夹名>`
pub fn instance_folder(app: &AppHandle, folder_name: &str) -> PathBuf {
    versions_root(app).join(folder_name)
}

/// 启动器私有数据目录：`<实例文件夹>\CCL`
pub fn ccl_meta_dir(app: &AppHandle, folder_name: &str) -> PathBuf {
    instance_folder(app, folder_name).join("CCL")
}

/// 生成一个未被占用的实例文件夹名：我的生存档 → 我的生存档-2 → 我的生存档-3…
pub fn unique_folder_name(app: &AppHandle, base: &str) -> String {
    let base = sanitize_folder_name(base);
    let root = versions_root(app);
    if !root.join(&base).exists() {
        return base;
    }
    for n in 2..999 {
        let cand = format!("{base}-{n}");
        if !root.join(&cand).exists() {
            return cand;
        }
    }
    format!("{base}-{}", &uuid::Uuid::new_v4().to_string()[..8])
}

// ------------------------------------------------------------------ 版本隔离

/// 版本隔离策略（对齐 PCL CE 的 `LaunchArgumentIndieV2`）
pub const INDIE_OPTIONS: &[(&str, &str, u32)] = &[
    ("全部关闭", "所有实例共用 Minecraft 文件夹，与官方启动器行为一致", 0),
    ("仅可安装模组的实例", "只有 Fabric / Forge / NeoForge / Quilt 类实例会被隔离", 1),
    ("仅非正式版", "只隔离快照版、远古版、愚人节版", 2),
    ("非正式版与模组实例", "上面两种情况的并集", 3),
    ("隔离所有实例", "推荐：每个实例拥有独立的存档、模组与设置", 4),
];

/// 该实例的加载器是否「可装模组」（PCL CE 的 `Modable`）
fn is_modable(inst: &Instance) -> bool {
    !matches!(inst.loader.as_str(), "vanilla" | "")
}

/// 判断 MC 版本是否为正式版（用于「仅隔离非正式版」策略）
fn is_release_version(app: &AppHandle, inst: &Instance) -> bool {
    // 优先读已安装版本 JSON 里声明的 type，最准
    let mc_root = mc_root(app, inst);
    let vid = inst
        .installed_version_id
        .clone()
        .unwrap_or_else(|| inst.minecraft_version.clone());
    let p = mc_root
        .join("versions")
        .join(&vid)
        .join(format!("{vid}.json"));
    if let Ok(text) = fs::read_to_string(&p) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
            if let Some(t) = v.get("type").and_then(|t| t.as_str()) {
                return t == "release";
            }
        }
    }
    // 兜底：按版本号字符串猜。正式版形如 1.21.11 / 26.2
    let v = inst.minecraft_version.trim().to_lowercase();
    if v.contains("pre")
        || v.contains("rc")
        || v.contains("snapshot")
        || v.contains("beta")
        || v.contains("alpha")
        || v.contains("-w")
    {
        return false;
    }
    // 25w02a 这类周快照
    let bytes = v.as_bytes();
    if bytes.len() >= 4 && bytes[2] == b'w' && bytes[0].is_ascii_digit() && bytes[1].is_ascii_digit()
    {
        return false;
    }
    true
}

/// 实例文件夹下已经有 mods 或 saves 内容 → 自动判为已隔离（PCL CE 的同款启发式）
fn folder_has_game_data(app: &AppHandle, inst: &Instance) -> bool {
    let Some(folder) = inst.folder_name.as_deref() else {
        return false;
    };
    let base = instance_folder(app, folder);
    for sub in ["mods", "saves"] {
        let d = base.join(sub);
        if let Ok(rd) = fs::read_dir(&d) {
            if rd.flatten().next().is_some() {
                return true;
            }
        }
    }
    false
}

/// 计算某个实例是否启用版本隔离
///
/// 实例级覆盖优先：`Some(1)` 强制隔离 / `Some(2)` 强制不隔离 / 其余按全局策略推断。
pub fn isolation_enabled(app: &AppHandle, inst: &Instance) -> bool {
    match inst.version_isolation {
        Some(1) => true,
        Some(2) => false,
        _ => {
            if folder_has_game_data(app, inst) {
                return true;
            }
            let s = settings::load(app);
            match s.version_isolation {
                0 => false,
                1 => is_modable(inst),
                2 => !is_release_version(app, inst),
                3 => is_modable(inst) || !is_release_version(app, inst),
                _ => true,
            }
        }
    }
}

/// 隔离状态的文字描述，给界面显示用
pub fn isolation_label(app: &AppHandle, inst: &Instance) -> String {
    let on = isolation_enabled(app, inst);
    let src = match inst.version_isolation {
        Some(1) => "手动开启",
        Some(2) => "手动关闭",
        _ => {
            if folder_has_game_data(app, inst) {
                "自动（实例文件夹已有数据）"
            } else {
                "跟随全局设置"
            }
        }
    };
    format!("{}{}", if on { "已隔离" } else { "未隔离" }, format!(" · {src}"))
}

/// `--gameDir`：隔离 → 实例文件夹；不隔离 → Minecraft 文件夹根
pub fn resolve_game_dir(app: &AppHandle, inst: &Instance) -> PathBuf {
    match inst.folder_name.as_deref() {
        Some(folder) if isolation_enabled(app, inst) => instance_folder(app, folder),
        Some(_) => mc_folder(app),
        // 旧版实例（instances/<uuid>/game）：整包结构，game_dir 就是全部
        None => PathBuf::from(&inst.game_dir),
    }
}

/// MC 根文件夹：versions / libraries / assets 所在的位置。
/// 新布局下恒为 `.minecraft`；旧实例照旧用它的 game 目录。
pub fn mc_root(app: &AppHandle, inst: &Instance) -> PathBuf {
    match inst.folder_name.as_deref() {
        Some(_) => mc_folder(app),
        None => PathBuf::from(&inst.game_dir),
    }
}

// ------------------------------------------------------------------ 文件夹维护

/// 确保 Minecraft 文件夹结构完整，并创建 launcher_profiles.json（PCL CE 行为）
pub fn ensure_folder_layout(app: &AppHandle) -> PathBuf {
    let root = mc_folder(app);
    let _ = fs::create_dir_all(root.join("versions"));
    let _ = fs::create_dir_all(root.join("libraries"));
    let _ = fs::create_dir_all(root.join("assets"));
    ensure_launcher_profiles_json(&root);
    root
}

/// 让官方启动器也能识别这个目录（PCL CE 的 `McFolderLauncherProfilesJsonCreate`）
pub fn ensure_launcher_profiles_json(folder: &Path) {
    let p = folder.join("launcher_profiles.json");
    if p.exists() {
        return;
    }
    let now = chrono::Local::now().format("%Y-%m-%dT%H:%M:%S.0000Z").to_string();
    let content = format!(
        r#"{{
    "profiles": {{
        "CCL": {{
            "icon": "Grass",
            "name": "CCL",
            "lastVersionId": "latest-release",
            "type": "latest-release",
            "lastUsed": "{now}"
        }}
    }},
    "selectedProfile": "CCL",
    "clientToken": "23323323323323323323323323323333"
}}"#
    );
    let _ = fs::write(p, content);
}

/// 扫描可选的 Minecraft 文件夹（PCL CE 的 `McFolderListLoadSub`）
#[tauri::command]
pub fn scan_game_folders(app: AppHandle) -> Vec<GameFolder> {
    let mut out: Vec<GameFolder> = vec![];
    let s = settings::load(&app);
    // 1. 用户自定义文件夹（格式「名称>路径」，与 PCL CE 一致）
    for raw in &s.custom_folders {
        let Some((name, path)) = raw.split_once('>') else {
            continue;
        };
        if path.trim().is_empty() {
            continue;
        }
        out.push(GameFolder {
            name: name.trim().to_string(),
            location: with_sep(Path::new(path.trim())),
            kind: "custom".into(),
        });
    }

    // 2. 启动器所在目录（若里面有 versions）
    let exe = exe_dir();
    if exe.join("versions").exists() {
        out.push(GameFolder {
            name: "启动器文件夹".into(),
            location: with_sep(&exe),
            kind: "original".into(),
        });
    }

    // 3. 启动器目录下的子文件夹：含 versions 子目录，或本身就是 .minecraft
    if let Ok(entries) = fs::read_dir(&exe) {
        for entry in entries.flatten() {
            let p = entry.path();
            if !p.is_dir() {
                continue;
            }
            let is_mc = p.join("versions").exists() || p.file_name().and_then(|n| n.to_str()) == Some(".minecraft");
            if is_mc && p != exe {
                out.push(GameFolder {
                    name: p.file_name().and_then(|n| n.to_str()).unwrap_or("文件夹").to_string(),
                    location: with_sep(&p),
                    kind: "original".into(),
                });
            }
        }
    }

    // 4. 官方启动器文件夹 %APPDATA%\.minecraft
    let official = default_game_folder(&app);
    if !out.iter().any(|f| same_path(&f.location, &official)) {
        out.push(GameFolder {
            name: "官方启动器文件夹".into(),
            location: with_sep(&official),
            kind: "original".into(),
        });
    }

    // 5. 当前设置指向的文件夹，若不在列表里也补上
    let cur = mc_folder(&app);
    if !out.iter().any(|f| same_path(&f.location, &cur)) {
        out.push(GameFolder {
            name: "当前设置".into(),
            location: with_sep(&cur),
            kind: "custom".into(),
        });
    }

    out
}

fn with_sep(p: &Path) -> String {
    let s = p.to_string_lossy().to_string();
    if s.ends_with(std::path::MAIN_SEPARATOR) {
        s
    } else {
        format!("{s}{}", std::path::MAIN_SEPARATOR)
    }
}

fn same_path(a: &str, b: &Path) -> bool {
    let a = a.trim_end_matches(|c| c == '/' || c == '\\');
    let b = b.to_string_lossy().to_string();
    let b = b.trim_end_matches(|c| c == '/' || c == '\\');
    a.eq_ignore_ascii_case(b)
}

/// 当前 Minecraft 文件夹的绝对路径（前端装配版本相关命令参数用）
#[tauri::command]
pub fn get_mc_folder(app: AppHandle) -> String {
    mc_folder(&app).to_string_lossy().to_string()
}

/// 添加一个自定义 Minecraft 文件夹（返回更新后的列表）
#[tauri::command]
pub fn add_custom_game_folder(app: AppHandle, name: String, path: String) -> Result<Vec<String>, String> {
    let mut s = settings::load(&app);
    let entry = format!("{}>{}", name.trim(), path.trim());
    if !s.custom_folders.iter().any(|e| {
        e.split_once('>')
            .map(|(_, p)| same_path(p, Path::new(path.trim())))
            .unwrap_or(false)
    }) {
        s.custom_folders.push(entry);
    }
    settings::save(&app, &s)?;
    Ok(s.custom_folders)
}

#[tauri::command]
pub fn remove_custom_game_folder(app: AppHandle, path: String) -> Result<Vec<String>, String> {
    let mut s = settings::load(&app);
    s.custom_folders.retain(|e| {
        !e.split_once('>')
            .map(|(_, p)| same_path(p, Path::new(path.trim())))
            .unwrap_or(false)
    });
    settings::save(&app, &s)?;
    Ok(s.custom_folders)
}

/// 切换 Minecraft 文件夹后立即补齐目录结构
#[tauri::command]
pub fn switch_game_folder(app: AppHandle, path: String) -> Result<String, String> {
    let p = PathBuf::from(path.trim());
    // 写权限探测：PCL CE 用 CheckPermissionWithException，这里用建目录 + 删目录代替
    let probe = p.join(".ccl-write-test");
    fs::create_dir_all(&probe).map_err(|e| format!("无法写入该文件夹：{e}"))?;
    let _ = fs::remove_dir_all(&probe);
    fs::create_dir_all(p.join("versions")).map_err(|e| format!("创建 versions 目录失败：{e}"))?;

    let mut s = settings::load(&app);
    s.game_folder = p.to_string_lossy().to_string();
    settings::save(&app, &s)?;
    ensure_launcher_profiles_json(&p);
    Ok(p.to_string_lossy().to_string())
}
