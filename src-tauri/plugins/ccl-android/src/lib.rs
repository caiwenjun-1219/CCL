//! CCL Android 原生插件
//!
//! 目前仅提供一项能力：**存储权限（MANAGE_EXTERNAL_STORAGE）的检测与申请**。
//! 参考 FCL 的首启流程：首次启动必须授予「所有文件访问」权限，才能把 Java 运行时与
//! 游戏本体下载到共享外部存储（/storage/emulated/0/CCL），方便用户直接在文件管理器里
//! 浏览 / 添加模组与存档。
//!
//! - Android：Kotlin 端通过系统设置 Intent 申请权限，并用 `@ActivityCallback` 取回结果。
//! - 桌面 / iOS：无需该权限，直接返回 `granted = true`。

use tauri::{Manager, Runtime, plugin::{Builder, TauriPlugin}};

mod commands;
mod error;
#[cfg(desktop)]
mod desktop;
#[cfg(mobile)]
mod mobile;

pub use error::{Error, Result};

#[cfg(desktop)]
use desktop::StoragePerm;
#[cfg(mobile)]
use mobile::StoragePerm;

/// APK assets 里是否存在某个内置资源（Android 专用，其余平台恒 false）
pub fn has_asset<R: Runtime>(app: &tauri::AppHandle<R>, src: &str) -> bool {
    StoragePerm::get(app).has_asset(src)
}

/// 把 APK assets 里的内置资源拷到磁盘（Android 专用）
pub fn copy_asset<R: Runtime>(
    app: &tauri::AppHandle<R>,
    src: &str,
    dest: &str,
) -> std::result::Result<u64, String> {
    StoragePerm::get(app).copy_asset(src, dest)
}

/// 初始化插件
pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("ccl-android")
        .invoke_handler(tauri::generate_handler![
            commands::check_storage_permission,
            commands::request_storage_permission,
        ])
        .setup(|app, api| {
            // 各平台 init 返回的结构体（桌面为占位单元类型，移动端为 PluginHandle）
            // 需以 managed state 形式登记，供命令侧 StoragePerm::get 取回。
            #[cfg(desktop)]
            let handle = desktop::init(app, api)?;
            #[cfg(mobile)]
            let handle = mobile::init(app, api)?;
            app.manage(handle);
            Ok(())
        })
        .build()
}
