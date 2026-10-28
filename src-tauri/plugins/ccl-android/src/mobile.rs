//! 移动端：通过 Kotlin 插件申请 / 检测 Android 存储权限。

use serde::de::DeserializeOwned;
use tauri::{AppHandle, Manager, Runtime, plugin::{PluginApi, PluginHandle}};

use crate::Result;

#[cfg(target_os = "android")]
const PLUGIN_IDENTIFIER: &str = "com.caiwenjun.ccl.android";

pub struct StoragePerm<R: Runtime>(PluginHandle<R>);

pub fn init<R: Runtime, C: DeserializeOwned>(
    _app: &AppHandle<R>,
    api: PluginApi<R, C>,
) -> Result<StoragePerm<R>> {
    #[cfg(target_os = "android")]
    {
        let handle = api
            .register_android_plugin(PLUGIN_IDENTIFIER, "StoragePermissionPlugin")
            .map_err(|e| crate::Error::Any(e.to_string()))?;
        Ok(StoragePerm(handle))
    }
    #[cfg(not(target_os = "android"))]
    {
        Err(crate::Error::Any(
            "storage permission plugin only supports android".into(),
        ))
    }
}

impl<R: Runtime> StoragePerm<R> {
    pub fn get(app: &AppHandle<R>) -> &StoragePerm<R> {
        app.state::<StoragePerm<R>>().inner()
    }

    /// 检测权限（调用 Kotlin 的 checkStoragePermission）
    pub async fn check(&self) -> bool {
        #[cfg(target_os = "android")]
        {
            self.0
                .run_mobile_plugin::<PermResponse>("checkStoragePermission", ())
                .map(|r| r.granted)
                .unwrap_or(false)
        }
        #[cfg(not(target_os = "android"))]
        {
            true
        }
    }

    /// 申请权限（调用 Kotlin 的 requestStoragePermission，跳转系统设置）
    pub async fn request(&self) -> bool {
        #[cfg(target_os = "android")]
        {
            self.0
                .run_mobile_plugin::<PermResponse>("requestStoragePermission", ())
                .map(|r| r.granted)
                .unwrap_or(false)
        }
        #[cfg(not(target_os = "android"))]
        {
            true
        }
    }
}

#[derive(serde::Deserialize)]
struct PermResponse {
    granted: bool,
}
