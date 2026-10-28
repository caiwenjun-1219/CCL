use serde::Serialize;
use tauri::{AppHandle, Runtime};

use crate::{Result, StoragePerm};

/// 权限状态
#[derive(Debug, Serialize)]
pub struct PermState {
    /// 是否已获得「所有文件访问」权限（Android 11+）或读写权限（旧版）
    pub granted: bool,
}

/// 检测当前存储权限是否已授予
#[tauri::command]
pub async fn check_storage_permission<R: Runtime>(app: AppHandle<R>) -> Result<PermState> {
    Ok(PermState {
        granted: StoragePerm::get(&app).check().await,
    })
}

/// 申请存储权限（会跳转到系统设置页，用户返回后回传结果）
#[tauri::command]
pub async fn request_storage_permission<R: Runtime>(app: AppHandle<R>) -> Result<PermState> {
    Ok(PermState {
        granted: StoragePerm::get(&app).request().await,
    })
}
