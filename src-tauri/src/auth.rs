//! 账户体系：微软登录 + 离线模式
//!
//! 2026-10-24 调查结论：
//! - 微软已对官方启动器 client_id `00000000402b5328` **关闭设备码流**
//!   （`devicecode` 端点返回 AADSTS700016，authorize 端点正常）。
//! - 该 client_id 的 redirect_uri 白名单里**不含 localhost / 127.0.0.1**，
//!   所以 loopback 自动收码方案同样被微软拒绝。
//! - 白名单内可用：`https://login.microsoftonline.com/common/oauth2/nativeclient`、
//!   `https://login.live.com/oauth20_desktop.srf`、`urn:ietf:wg:oauth:2.0:oob`。
//!
//! 因此本模块实现**双模式**登录：
//! - `device` 模式：用户在设置里填入自建 Azure 应用的 client_id，走标准设备码流
//!   （体验最顺：显示验证码 + 打开验证页 + 自动轮询）。
//! - `code` 模式：默认（官方 client_id），走授权码流，用户在浏览器完成授权后
//!   从地址栏手动复制授权码粘贴回来。

use md5::{Digest as Md5Digest, Md5};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::fs;
use std::path::PathBuf;
use tauri::AppHandle;

use crate::downloader;
use crate::settings;

/// Minecraft 官方启动器客户端 ID（授权码流实测可用；设备码流已被微软关闭）
const MS_CLIENT_ID: &str = "00000000402b5328";
/// 授权端点（consumers tenant = 个人微软账户）
const MS_AUTHORIZE_URL: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/authorize";
/// 设备码端点（仅自建应用可用）
const MS_DEVICE_CODE_URL: &str =
    "https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode";
/// v2 令牌端点（设备码 grant 使用）
const MS_V2_TOKEN_URL: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/token";
/// 令牌端点（MSA v1，对官方 client_id 同时支持授权码与刷新令牌）
const MS_TOKEN_URL: &str = "https://login.live.com/oauth20_token.srf";
/// 微软白名单内的回调地址（loopback / localhost 会被拒）
const MS_REDIRECT_URI: &str = "https://login.live.com/oauth20_desktop.srf";
const MS_SCOPE: &str = "XboxLive.signin offline_access";

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
    fs::write(accounts_path(app), text).map_err(|e| e.to_string())
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
pub struct MsLoginStart {
    /// 登录模式："device" = 设备码（需自建 app） / "code" = 授权码（官方 client_id）
    pub mode: String,
    /// 设备码模式：用户需在浏览器输入的验证码
    pub user_code: String,
    /// 设备码模式：轮询用的 device_code
    pub device_code: String,
    /// 设备码模式：验证页地址
    pub verification_uri: String,
    /// 设备码模式：轮询间隔（秒）
    pub interval: u64,
    /// 授权码模式：用户需要打开的授权页地址
    pub login_url: String,
    pub expires_in: u64,
}

/// 从 query string 中提取 code / error 参数
fn parse_callback(query: &str) -> (Option<String>, Option<String>) {
    let mut code = None;
    let mut error = None;
    for pair in query.split('&') {
        let mut kv = pair.splitn(2, '=');
        let key = kv.next().unwrap_or("");
        let value = kv.next().unwrap_or("");
        let decoded = percent_decode(&value.replace('+', " "));
        match key {
            "code" => code = Some(decoded),
            "error" => {
                error = Some(match decoded.as_str() {
                    "access_denied" => "用户拒绝了授权".to_string(),
                    other => format!("授权失败: {other}"),
                });
            }
            _ => {}
        }
    }
    (code, error)
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("");
            if let Ok(b) = u8::from_str_radix(hex, 16) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

/// query 参数用的 percent-encode（仅编码非保留字符）
fn url_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// 读取生效的微软客户端 ID：优先用户自建应用，其次官方启动器
fn client_id(app: &AppHandle) -> String {
    let custom = settings::load(app).microsoft_client_id.trim().to_string();
    if custom.is_empty() {
        MS_CLIENT_ID.to_string()
    } else {
        custom
    }
}

/// 第一步：开始登录
/// - 自建 Azure 应用（client_id 可用）→ 设备码流，体验最顺
/// - 官方 client_id → 授权码流（微软已关闭其设备码流），需手动复制授权码
#[tauri::command]
pub async fn ms_login_start(app: AppHandle) -> Result<MsLoginStart, String> {
    let cid = client_id(&app);
    let client = downloader::http_client();

    // 先试设备码流：官方 client_id 会返回 AADSTS700016，此时回退到授权码流
    let resp = client
        .post(MS_DEVICE_CODE_URL)
        .form(&[
            ("client_id", cid.as_str()),
            ("scope", MS_SCOPE),
            ("tenant", "/consumers"),
        ])
        .send()
        .await
        .map_err(|e| format!("请求设备码失败: {e}"))?;

    if resp.status().is_success() {
        let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
        return Ok(MsLoginStart {
            mode: "device".into(),
            user_code: json["user_code"].as_str().unwrap_or("").to_string(),
            device_code: json["device_code"].as_str().unwrap_or("").to_string(),
            verification_uri: json["verification_uri"].as_str().unwrap_or("").to_string(),
            interval: json["interval"].as_u64().unwrap_or(5),
            login_url: String::new(),
            expires_in: json["expires_in"].as_u64().unwrap_or(900),
        });
    }

    // 设备码流不可用 → 授权码流（redirect_uri 用微软白名单内的 desktop.srf）
    let login_url = format!(
        "{MS_AUTHORIZE_URL}?client_id={}&scope={}&response_type=code&redirect_uri={}&ui_locales=zh-CN",
        cid,
        url_encode(MS_SCOPE),
        url_encode(MS_REDIRECT_URI),
    );
    Ok(MsLoginStart {
        mode: "code".into(),
        user_code: String::new(),
        device_code: String::new(),
        verification_uri: String::new(),
        interval: 5,
        login_url,
        expires_in: 900,
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

/// 设备码流：轮询令牌
#[tauri::command]
pub async fn ms_login_poll(app: AppHandle, device_code: String) -> Result<LoginPoll, String> {
    let client = downloader::http_client();
    let resp = client
        .post(MS_V2_TOKEN_URL)
        .form(&[
            ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ("client_id", client_id(&app).as_str()),
            ("device_code", device_code.as_str()),
            ("scope", MS_SCOPE),
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

/// 授权码流：用户手动粘贴授权码后换取令牌
#[tauri::command]
pub async fn ms_login_exchange(app: AppHandle, code: String) -> Result<LoginPoll, String> {
    let mut code = code.trim().to_string();
    // 兼容整条回调 URL 粘贴（?code=xxx）
    if let Some(c) = parse_callback(code.split('?').nth(1).unwrap_or("")).0 {
        if !c.is_empty() {
            code = c;
        }
    } else if code.contains("code=") {
        code = code
            .split("code=")
            .nth(1)
            .map(|s| s.split('&').next().unwrap_or(s))
            .unwrap_or(&code)
            .to_string();
    }
    if code.is_empty() {
        return Ok(LoginPoll::Failed("授权码为空".into()));
    }

    let client = downloader::http_client();
    let (ms_token, refresh_token, expires_in) =
        exchange_code(&client, &client_id(&app), &code).await?;
    let account = complete_ms_login(&client, &ms_token, &refresh_token, expires_in).await?;
    let mut accounts = load_accounts(&app);
    accounts.retain(|a| a.uuid != account.uuid);
    accounts.push(account.clone());
    save_accounts(&app, &accounts)?;
    Ok(LoginPoll::Done(account))
}

/// 授权码 → MSA access/refresh token
async fn exchange_code(
    client: &reqwest::Client,
    cid: &str,
    code: &str,
) -> Result<(String, String, i64), String> {
    let resp = client
        .post(MS_TOKEN_URL)
        .form(&[
            ("client_id", cid),
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", MS_REDIRECT_URI),
            ("scope", MS_SCOPE),
        ])
        .send()
        .await
        .map_err(|e| format!("请求令牌失败: {e}"))?;
    let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    if let Some(err) = json.get("error").and_then(|e| e.as_str()) {
        let desc = json
            .get("error_description")
            .and_then(|d| d.as_str())
            .unwrap_or(err);
        return Err(format!("微软拒绝授权码: {desc}"));
    }
    let access = json["access_token"].as_str().unwrap_or("").to_string();
    let refresh = json["refresh_token"].as_str().unwrap_or("").to_string();
    let expires_in = json["expires_in"].as_i64().unwrap_or(86400);
    if access.is_empty() {
        return Err("未获取到访问令牌".into());
    }
    Ok((access, refresh, expires_in))
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
        .post(MS_TOKEN_URL)
        .form(&[
            ("client_id", client_id(&app).as_str()),
            ("grant_type", "refresh_token"),
            ("refresh_token", account.refresh_token.as_str()),
            ("scope", MS_SCOPE),
        ])
        .send()
        .await
        .map_err(|e| format!("刷新失败: {e}"))?;
    let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    if let Some(err) = json.get("error").and_then(|e| e.as_str()) {
        let desc = json
            .get("error_description")
            .and_then(|d| d.as_str())
            .unwrap_or(err);
        return Err(format!("刷新失败: {desc}"));
    }
    let ms_token = json["access_token"].as_str().unwrap_or("").to_string();
    let refresh = json["refresh_token"].as_str().unwrap_or(&account.refresh_token).to_string();
    let expires_in = json["expires_in"].as_i64().unwrap_or(86400);

    let new_account = complete_ms_login(&client, &ms_token, &refresh, expires_in).await?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_callback_extracts_code() {
        let (code, err) = parse_callback("code=abc123&session=xyz");
        assert_eq!(code.as_deref(), Some("abc123"));
        assert!(err.is_none());
    }

    #[test]
    fn parse_callback_ignores_other_params() {
        let (code, err) = parse_callback("session=xyz&code=abc&scope=full");
        assert_eq!(code.as_deref(), Some("abc"));
        assert!(err.is_none());
    }

    #[test]
    fn parse_callback_percent_decodes_code() {
        let (code, _) = parse_callback("code=a%2Fb%2Bc%3D");
        assert_eq!(code.as_deref(), Some("a/b+c="));
    }

    #[test]
    fn parse_callback_reports_access_denied() {
        let (code, err) = parse_callback("error=access_denied");
        assert!(code.is_none());
        assert_eq!(err.as_deref(), Some("用户拒绝了授权"));
    }

    #[test]
    fn parse_callback_reports_other_error() {
        let (_, err) = parse_callback("error=invalid_request");
        assert!(err.unwrap().contains("invalid_request"));
    }

    #[test]
    fn url_encode_escapes_scope() {
        assert_eq!(
            url_encode("XboxLive.signin offline_access"),
            "XboxLive.signin%20offline_access"
        );
        assert_eq!(
            url_encode("https://login.live.com/oauth20_desktop.srf"),
            "https%3A%2F%2Flogin.live.com%2Foauth20_desktop.srf"
        );
    }

    #[test]
    fn percent_decode_roundtrip() {
        let raw = "a/b+c=d e&f";
        assert_eq!(percent_decode(&url_encode(raw)), raw);
    }

    /// 授权 URL 必须使用微软白名单内的回调地址，且不得出现 loopback
    #[test]
    fn authorize_url_uses_whitelisted_redirect() {
        let url = format!(
            "{MS_AUTHORIZE_URL}?client_id={}&scope={}&response_type=code&redirect_uri={}",
            MS_CLIENT_ID,
            url_encode(MS_SCOPE),
            url_encode(MS_REDIRECT_URI),
        );
        assert!(url.contains(
            "redirect_uri=https%3A%2F%2Flogin.live.com%2Foauth20_desktop.srf"
        ));
        assert!(!url.contains("127.0.0.1"));
        assert!(!url.contains("localhost"));
    }

    /// 手动粘贴整条回调 URL 时应能正确抽出 code
    #[test]
    fn manual_exchange_extracts_code_from_full_url() {
        let url = "https://login.live.com/oauth20_desktop.srf?code=AbC%2FdE&session=zzz";
        let (code, _) = parse_callback(url.split('?').nth(1).unwrap_or(""));
        assert_eq!(code.as_deref(), Some("AbC/dE"));
    }
}
