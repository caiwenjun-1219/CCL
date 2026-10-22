//! 账户体系：微软设备码登录（Xbox Live → XSTS → Minecraft 令牌链）+ 离线模式

use md5::{Digest as Md5Digest, Md5};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::fs;
use std::path::PathBuf;
use tauri::AppHandle;

use crate::downloader;
use crate::settings;

/// Minecraft 官方启动器客户端 ID（各第三方启动器通用）
const MS_CLIENT_ID: &str = "00000000402b5328";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub uuid: String,
    pub name: String,
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: String,
    /// "msa" = 微软正版, "offline" = 离线
    pub account_type: String,
    #[serde(default)]
    pub expires_at: i64,
}

fn accounts_path(app: &AppHandle) -> PathBuf {
    settings::data_dir(app).join("accounts.json")
}

pub fn load_accounts(app: &AppHandle) -> Vec<Account> {
    match fs::read_to_string(accounts_path(app)) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
        Err(_) => vec![],
    }
}

fn save_accounts(app: &AppHandle, accounts: &[Account]) -> Result<(), String> {
    let dir = settings::data_dir(app);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let text = serde_json::to_string_pretty(accounts).map_err(|e| e.to_string())?;
    fs::write(accounts_path(app), text).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn accounts_list(app: AppHandle) -> Vec<Account> {
    load_accounts(&app)
}

#[tauri::command]
pub fn remove_account(app: AppHandle, uuid: String) -> Result<(), String> {
    let mut accounts = load_accounts(&app);
    accounts.retain(|a| a.uuid != uuid);
    save_accounts(&app, &accounts)
}

#[derive(Debug, Serialize)]
pub struct DeviceCodeStart {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub interval: u64,
    pub expires_in: u64,
}

/// 第一步：获取微软设备码
#[tauri::command]
pub async fn ms_login_start() -> Result<DeviceCodeStart, String> {
    let client = downloader::http_client();
    let resp = client
        .post("https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode")
        .form(&[
            ("client_id", MS_CLIENT_ID),
            ("scope", "XboxLive.signin offline_access"),
        ])
        .send()
        .await
        .map_err(|e| format!("请求设备码失败: {e}"))?;
    if !resp.status().is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("微软拒绝设备码请求: {text}"));
    }
    let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    Ok(DeviceCodeStart {
        device_code: json["device_code"].as_str().unwrap_or("").to_string(),
        user_code: json["user_code"].as_str().unwrap_or("").to_string(),
        verification_uri: json["verification_uri"].as_str().unwrap_or("").to_string(),
        interval: json["interval"].as_u64().unwrap_or(5),
        expires_in: json["expires_in"].as_u64().unwrap_or(900),
    })
}

#[derive(Debug, Serialize)]
pub enum LoginPoll {
    /// 等待用户授权中
    Pending,
    /// 登录成功
    Done(Account),
    /// 失败（过期/拒绝）
    Failed(String),
}

/// 第二步：轮询令牌（前端每 interval 秒调一次）
#[tauri::command]
pub async fn ms_login_poll(app: AppHandle, device_code: String) -> Result<LoginPoll, String> {
    let client = downloader::http_client();
    let resp = client
        .post("https://login.microsoftonline.com/consumers/oauth2/v2.0/token")
        .form(&[
            ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ("client_id", MS_CLIENT_ID),
            ("device_code", device_code.as_str()),
        ])
        .send()
        .await
        .map_err(|e| format!("轮询失败: {e}"))?;

    let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    if let Some(err) = json.get("error").and_then(|e| e.as_str()) {
        return match err {
            "authorization_pending" | "slow_down" => Ok(LoginPoll::Pending),
            "expired_token" => Ok(LoginPoll::Failed("设备码已过期，请重新开始".into())),
            "access_denied" => Ok(LoginPoll::Failed("用户拒绝了授权".into())),
            other => Ok(LoginPoll::Failed(format!("登录失败: {other}"))),
        };
    }

    let ms_token = json["access_token"].as_str().unwrap_or("").to_string();
    let refresh_token = json["refresh_token"].as_str().unwrap_or("").to_string();
    let expires_in = json["expires_in"].as_i64().unwrap_or(86400);
    if ms_token.is_empty() {
        return Ok(LoginPoll::Failed("未获取到访问令牌".into()));
    }

    let account = complete_ms_login(&client, &ms_token, &refresh_token, expires_in).await?;
    let mut accounts = load_accounts(&app);
    accounts.retain(|a| a.uuid != account.uuid);
    accounts.push(account.clone());
    save_accounts(&app, &accounts)?;
    Ok(LoginPoll::Done(account))
}

/// XBL → XSTS → MC 令牌链
async fn complete_ms_login(
    client: &reqwest::Client,
    ms_token: &str,
    refresh_token: &str,
    expires_in: i64,
) -> Result<Account, String> {
    // Xbox Live
    let xbl: serde_json::Value = client
        .post("https://user.auth.xboxlive.com/user/authenticate")
        .json(&json!({
            "Properties": {
                "AuthMethod": "RPS",
                "SiteName": "user.auth.xboxlive.com",
                "RpsTicket": ms_token
            },
            "RelyingParty": "http://auth.xboxlive.com",
            "Type": "JWT"
        }))
        .send()
        .await
        .map_err(|e| format!("Xbox Live 认证失败: {e}"))?
        .json()
        .await
        .map_err(|e| e.to_string())?;
    let xbl_token = xbl["Token"].as_str().unwrap_or("").to_string();
    let uhs = xbl["DisplayClaims"]["xui"][0]["uhs"]
        .as_str()
        .unwrap_or("")
        .to_string();

    // XSTS
    let xsts: serde_json::Value = client
        .post("https://xsts.auth.xboxlive.com/xsts/authorize")
        .json(&json!({
            "Properties": {
                "SandboxId": "RETAIL",
                "UserTokens": [xbl_token]
            },
            "RelyingParty": "rp://api.minecraftservices.com/",
            "Type": "JWT"
        }))
        .send()
        .await
        .map_err(|e| format!("XSTS 认证失败: {e}"))?
        .json()
        .await
        .map_err(|e| e.to_string())?;
    let xsts_token = xsts["Token"].as_str().unwrap_or("").to_string();
    if xsts_token.is_empty() {
        return Err(format!(
            "XSTS 失败: {}",
            xsts["XErr"].as_i64().map(|v| v.to_string()).unwrap_or("未知".into())
        ));
    }

    // Minecraft 令牌
    let mc: serde_json::Value = client
        .post("https://api.minecraftservices.com/authentication/login_with_xbox")
        .json(&json!({ "identityToken": format!("XSTS3 x={uhs},x={xsts_token}") }))
        .send()
        .await
        .map_err(|e| format!("Minecraft 认证失败: {e}"))?
        .json()
        .await
        .map_err(|e| e.to_string())?;
    let access_token = mc["access_token"].as_str().unwrap_or("").to_string();

    // 正版校验
    let entitle: serde_json::Value = client
        .get("https://api.minecraftservices.com/entitlements/mcstore")
        .bearer_auth(&access_token)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;
    let owns_game = entitle
        .get("items")
        .and_then(|i| i.as_array())
        .map(|a| !a.is_empty())
        .unwrap_or(false);

    // 角色名
    let profile: serde_json::Value = client
        .get("https://api.minecraftservices.com/minecraft/profile")
        .bearer_auth(&access_token)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;
    let name = profile["name"].as_str().unwrap_or("").to_string();
    let uuid = profile["id"].as_str().unwrap_or("").to_string();
    if name.is_empty() || uuid.is_empty() {
        return Err(if owns_game {
            "未找到 Minecraft 角色档案（请先在 Minecraft.net 创建角色）".into()
        } else {
            "该微软账户未拥有 Minecraft Java 版".into()
        });
    }

    Ok(Account {
        uuid,
        name,
        access_token,
        refresh_token: refresh_token.to_string(),
        account_type: "msa".into(),
        expires_at: chrono::Utc::now().timestamp() + expires_in,
    })
}

/// 刷新微软令牌
#[tauri::command]
pub async fn refresh_account(app: AppHandle, uuid: String) -> Result<Account, String> {
    let accounts = load_accounts(&app);
    let account = accounts
        .iter()
        .find(|a| a.uuid == uuid)
        .cloned()
        .ok_or("账户不存在")?;
    if account.refresh_token.is_empty() {
        return Err("无刷新令牌，请重新登录".into());
    }
    let client = downloader::http_client();
    let resp = client
        .post("https://login.microsoftonline.com/consumers/oauth2/v2.0/token")
        .form(&[
            ("grant_type", "refresh_token"),
            ("client_id", MS_CLIENT_ID),
            ("refresh_token", account.refresh_token.as_str()),
            ("scope", "XboxLive.signin offline_access"),
        ])
        .send()
        .await
        .map_err(|e| format!("刷新失败: {e}"))?;
    let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    let ms_token = json["access_token"].as_str().unwrap_or("").to_string();
    let refresh = json["refresh_token"].as_str().unwrap_or(&account.refresh_token).to_string();
    let expires_in = json["expires_in"].as_i64().unwrap_or(86400);

    let new_account =
        complete_ms_login(&client, &ms_token, &refresh, expires_in).await?;
    let mut accounts = accounts;
    accounts.retain(|a| a.uuid != new_account.uuid);
    accounts.push(new_account.clone());
    save_accounts(&app, &accounts)?;
    Ok(new_account)
}

/// 离线登录：用户名 → 离线 UUID（MD5("OfflinePlayer:"+name) 版本 3 格式）
#[tauri::command]
pub fn offline_login(app: AppHandle, name: String) -> Result<Account, String> {
    let name = name.trim().to_string();
    if name.is_empty() || name.len() > 16 || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err("用户名需为 1-16 位字母/数字/下划线".into());
    }
    let digest = Md5::digest(format!("OfflinePlayer:{name}").as_bytes());
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&digest);
    bytes[6] = (bytes[6] & 0x0f) | 0x30; // version 3
    bytes[8] = (bytes[8] & 0x3f) | 0x80; // variant
    let uuid = format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0], bytes[1], bytes[2], bytes[3],
        bytes[4], bytes[5], bytes[6], bytes[7],
        bytes[8], bytes[9], bytes[10], bytes[11],
        bytes[12], bytes[13], bytes[14], bytes[15]
    );

    let account = Account {
        uuid,
        name,
        access_token: "0".into(),
        refresh_token: String::new(),
        account_type: "offline".into(),
        expires_at: 0,
    };
    let mut accounts = load_accounts(&app);
    accounts.retain(|a| a.uuid != account.uuid);
    accounts.push(account.clone());
    save_accounts(&app, &accounts)?;
    Ok(account)
}
