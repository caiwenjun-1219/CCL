//! 桌面 / iOS 端：无需存储权限，恒为已授予。
//! 仅用于让命令在桌面编译通过并保持统一接口。
//!
//! 关键点：桌面端使用零大小占位类型 `StoragePerm`，它本身 `Send + Sync`，
//! 不会破坏 Tauri 命令 future 要求的 `Send` 约束（此前用 `PhantomData<R>` 会引入
//! `R: Sync` 要求而导致编译失败）。

use serde::de::DeserializeOwned;
use tauri::{AppHandle, Manager, Runtime, plugin::PluginApi};

use crate::Result;

/// 桌面 / iOS 不需要真实存储权限，使用单元占位类型。
#[derive(Default)]
pub struct StoragePerm;

pub fn init<R: Runtime, C: DeserializeOwned>(
    _app: &AppHandle<R>,
    _api: PluginApi<R, C>,
) -> Result<StoragePerm> {
    Ok(StoragePerm)
}

impl StoragePerm {
    pub fn get<R: Runtime>(app: &AppHandle<R>) -> &StoragePerm {
        app.state::<StoragePerm>().inner()
    }

    /// 桌面 / iOS 不需要该权限
    pub async fn check(&self) -> bool {
        true
    }

    /// 桌面 / iOS 不需要该权限
    pub async fn request(&self) -> bool {
        true
    }
}
