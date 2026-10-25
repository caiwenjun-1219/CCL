//! 应用设置持久化（JSON 文件，存于 AppData/com.caiwenjun.ccl/settings.json）

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

fn default_true() -> bool {
    true
}
fn default_memory() -> u32 {
    4096
}
fn default_source() -> String {
    "bmclapi".to_string()
}
fn default_close_action() -> String {
    "minimize".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    /// 下载首选源: "bmclapi"(国内镜像) 或 "mojang"(官方)
    #[serde(default = "default_source")]
    pub download_source: String,
    /// 最大内存(MB)
    #[serde(default = "default_memory")]
    pub max_memory: u32,
    /// 额外 JVM 参数
    #[serde(default)]
    pub jvm_args: String,
    /// 启动时自动检查云控更新
    #[serde(default = "default_true")]
    pub auto_check_update: bool,
    /// 关闭窗口行为: "minimize" 托盘最小化 / "exit" 退出
    #[serde(default = "default_close_action")]
    pub close_action: String,
    /// CurseForge API Key（模组搜索/整合包导入需要）
    #[serde(default)]
    pub curseforge_api_key: String,
    /// 手动指定 Java 路径（空 = 自动选择）
    #[serde(default)]
    pub java_path: String,
    /// 窗口宽度/高度记忆
    #[serde(default)]
    pub window_width: Option<u32>,
    #[serde(default)]
    pub window_height: Option<u32>,
    /// 自定义微软应用 ID（Azure AD client id）
    /// 留空 = 使用 Minecraft 官方启动器 client_id（仅支持手动复制授权码）
    /// 填写自建应用（需勾选"允许公共客户端流"）= 走设备码流，体验最顺
    #[serde(default)]
    pub microsoft_client_id: String,

    // ---- PCL CE 风格：游戏目录与版本隔离 ----
    /// Minecraft 文件夹路径（.minecraft 根），空 = 使用 %APPDATA%\.minecraft
    #[serde(default)]
    pub game_folder: String,
    /// 版本隔离策略（对齐 PCL CE 的 LaunchArgumentIndieV2）：
    /// 0 关闭 / 1 仅可装模组的实例 / 2 仅非正式版 / 3 两者 / 4 全部（默认）
    #[serde(default = "default_isolation")]
    pub version_isolation: u32,
    /// 自定义 Microsoft 文件夹列表，格式「名称>绝对路径」
    #[serde(default)]
    pub custom_folders: Vec<String>,
    /// 网络代理："" = 不使用；"auto" = 自动读系统代理；否则形如
    /// "http://127.0.0.1:7897" 或 "socks5://127.0.0.1:7897"
    #[serde(default = "default_proxy")]
    pub proxy: String,
}

/// 默认使用「自动」：读取系统代理设置（Clash 等），未开启则等价于直连
fn default_proxy() -> String {
    "auto".to_string()
}

/// PCL CE 默认值为 4：隔离所有实例
fn default_isolation() -> u32 {
    4
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            download_source: default_source(),
            max_memory: default_memory(),
            jvm_args: String::new(),
            auto_check_update: true,
            close_action: default_close_action(),
            curseforge_api_key: String::new(),
            java_path: String::new(),
            window_width: None,
            window_height: None,
            microsoft_client_id: String::new(),
            game_folder: String::new(),
            version_isolation: default_isolation(),
            custom_folders: vec![],
            proxy: default_proxy(),
        }
    }
}

/// 应用数据根目录: %APPDATA%/com.caiwenjun.ccl
pub fn data_dir(app: &AppHandle) -> PathBuf {
    app.path()
        .app_data_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
}

pub fn settings_path(app: &AppHandle) -> PathBuf {
    data_dir(app).join("settings.json")
}

pub fn instances_dir(app: &AppHandle) -> PathBuf {
    data_dir(app).join("instances")
}

pub fn java_dir(app: &AppHandle) -> PathBuf {
    data_dir(app).join("java")
}

pub fn load(app: &AppHandle) -> AppSettings {
    let path = settings_path(app);
    match fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
        Err(_) => AppSettings::default(),
    }
}

pub fn save(app: &AppHandle, settings: &AppSettings) -> Result<(), String> {
    let dir = data_dir(app);
    fs::create_dir_all(&dir).map_err(|e| format!("创建数据目录失败: {e}"))?;
    let text = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    fs::write(settings_path(app), text).map_err(|e| format!("写入设置失败: {e}"))?;
    Ok(())
}

#[tauri::command]
pub fn get_settings(app: AppHandle) -> AppSettings {
    load(&app)
}

#[tauri::command]
pub fn save_settings(app: AppHandle, settings: AppSettings) -> Result<(), String> {
    save(&app, &settings)?;
    // 保存后热更新全局代理，立即生效
    crate::proxy::set_proxy(&settings.proxy);
    Ok(())
}
