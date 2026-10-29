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

impl<R: Runtime> StoragePerm<R> {
    /// assets 里是否存在某个内置资源（如 `jre/jre17-bin-arm64.tar.xz`）
    pub fn has_asset(&self, src: &str) -> bool {
        #[cfg(target_os = "android")]
        {
            let args = serde_json::json!({ "src": src, "dest": "" });
            match self
                .0
                .run_mobile_plugin::<ExistsResponse>("hasBundledAsset", args)
            {
                Ok(r) => {
                    // 打日志：参数被静默解析成空串这类问题，只有在日志里看得见
                    println!(
                        "[ccl-android] hasBundledAsset({src}) -> exists={} echoed_src={:?}",
                        r.exists, r.src
                    );
                    r.exists
                }
                Err(e) => {
                    println!("[ccl-android] hasBundledAsset({src}) 调用失败: {e:?}");
                    false
                }
            }
        }
        #[cfg(not(target_os = "android"))]
        {
            let _ = src;
            false
        }
    }

    /// 把 assets 里的内置资源流式拷到 `dest`（APK 内的 assets 无法被 Rust 直接读）
    pub fn copy_asset(&self, src: &str, dest: &str) -> std::result::Result<u64, String> {
        #[cfg(target_os = "android")]
        {
            let args = serde_json::json!({ "src": src, "dest": dest });
            self.0
                .run_mobile_plugin::<CopyResponse>("copyBundledAsset", args)
                .map(|r| r.bytes)
                .map_err(|e| format!("{e:?}"))
        }
        #[cfg(not(target_os = "android"))]
        {
            let _ = (src, dest);
            Err("内置资源仅在 Android 上可用".to_string())
        }
    }
}

#[derive(serde::Deserialize)]
struct ExistsResponse {
    #[serde(default)]
    exists: bool,
    /// 回声字段：Kotlin 侧把解析到的 src 原样返回，空串即代表参数没解析出来
    #[serde(default)]
    src: String,
}

#[derive(serde::Deserialize)]
struct CopyResponse {
    #[serde(default)]
    bytes: u64,
}
