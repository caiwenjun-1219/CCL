//! Java 运行时管理 —— 平台分流入口
//!
//! - `common.rs`：桌面与 Android 共用的数据结构与选版逻辑
//! - `desktop.rs`：Windows / macOS / Linux —— Adoptium 官方 JDK
//! - `android.rs`：Android —— EclipseJRE 交叉编译的 Android JRE（Pojav 格式）
//!
//! 两个平台对前端暴露的 tauri command 名称与签名完全一致：
//! `scan_javas` / `download_java` / `resolve_java`，前端无需分支。
//!
//! 平台实现里**不**加 `#[tauri::command]`，command 一律在本文件统一声明，
//! 这样宏生成的 `__cmd__*` 辅助项只存在于 `java` 模块内，不会因为
//! `cfg` 分流 + `pub use` 再导出而丢失。

mod common;
/// JVM 冒烟自检（跨平台，桌面端用于确认 java 可执行，Android 端用于判定走 exec 还是 dlopen）
pub mod smoke;
/// 内嵌 JVM 实验（仅 Android 有意义）：验证能否在 App 进程内 JNI_CreateJavaVM。
/// 这是"安卓能不能真的出画面"的前置条件 —— 子进程拿不到 Surface，必须同进程。
pub mod embedded;

#[cfg(not(target_os = "android"))]
pub mod desktop;
#[cfg(target_os = "android")]
pub mod android;

#[cfg(not(target_os = "android"))]
pub use desktop as platform;
#[cfg(target_os = "android")]
pub use android as platform;

pub use common::{parse_mc_version, required_java_major, select_java, JavaInfo};
pub use platform::auto_select_java;

use tauri::AppHandle;

/// Java 运行环境信息，供前端区分平台展示不同的版本列表与说明文案
#[derive(serde::Serialize)]
pub struct JavaEnv {
    /// `desktop` | `android`
    pub platform: String,
    /// 当前平台可下载的 Java 主版本
    pub supported_majors: Vec<u32>,
    /// 下载源名称（桌面 = Adoptium / Eclipse Temurin，安卓 = EclipseJRE）
    pub source: String,
    /// 是否为 Android 上的 Pojav 格式 JRE
    pub is_android_jre: bool,
    /// APK 里内置的 JRE 主版本（有则该版本首启无需联网下载）
    pub bundled_major: Option<u32>,
}

/// 获取当前平台的 Java 运行环境信息
#[tauri::command]
pub fn java_env(app: AppHandle) -> JavaEnv {
    JavaEnv {
        platform: if cfg!(target_os = "android") {
            "android".into()
        } else {
            "desktop".into()
        },
        supported_majors: platform::supported_majors().to_vec(),
        source: platform::SOURCE_NAME.to_string(),
        is_android_jre: cfg!(target_os = "android"),
        bundled_major: platform::bundled_java_major(&app),
    }
}

/// 列出可用的 Java 运行时
#[tauri::command]
pub fn scan_javas(app: AppHandle) -> Vec<JavaInfo> {
    platform::scan_javas(app)
}

/// 下载并安装指定主版本的 Java
#[tauri::command]
pub async fn download_java(app: AppHandle, major: u32) -> Result<JavaInfo, String> {
    platform::download_java(app, major).await
}

/// 解析启动用的 Java：设置里手动指定 > 自动选择
#[tauri::command]
pub fn resolve_java(app: AppHandle, mc_version: String) -> Result<JavaInfo, String> {
    platform::resolve_java(app, mc_version)
}

/// 按主版本要求选择 Java（满足 ≥ need 的最小者）。
///
/// 平台差异只体现在「怎么扫描到已装的 Java」上，选版规则统一在 `common::select_java`。
pub fn auto_select_java_with(app: &AppHandle, need: u32, label: &str) -> Result<JavaInfo, String> {
    select_java(&platform::scan_javas(app.clone()), need, label)
}

/// JVM 冒烟自检：只读地验证「这台设备能不能把 Java 跑起来」。
///
/// 桌面端返回 `exec_ok=true` 基本必然；Android 端的关键结论在
/// `exec_ok` / `dlopen_ok` 两个字段 —— 前者决定能不能沿用子进程启动，
/// 后者决定能不能改走内嵌 JVM。详见 [`smoke`] 模块注释。
#[tauri::command]
pub async fn java_smoke_test(app: AppHandle) -> Result<smoke::SmokeReport, String> {
    smoke::run(app).await
}

/// 内嵌 JVM 实验：在 App 进程内 dlopen libjvm.so 并 JNI_CreateJavaVM。
///
/// 只验证 VM 能否建立，不加载任何游戏代码。结果按进程缓存
/// （`JNI_CreateJavaVM` 一个进程只能成功一次）。
#[tauri::command]
pub async fn java_embedded_test(app: AppHandle) -> Result<embedded::EmbeddedReport, String> {
    embedded::run(app).await
}
