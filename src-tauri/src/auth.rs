//! 账户体系：微软登录（授权码流，loopback 收 code）+ 离线模式
//!
//! 2026-10-24：微软已对官方启动器 client_id 关闭设备码流（devicecode 端点
//! 返回 AADSTS700016，而 authorize 端点正常），故改用授权码流：
//! 本地起 loopback HTTP 服务 → 浏览器登录后 302 回 localhost → 自动拿到 code
//! → 用 https://login.live.com/oauth20_token.srf 换 access/refresh token。
//! 同时保留手动粘贴授权码的兜底命令（ms_login_exchange）。

use md5::{Digest as Md5Digest, Md5};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use tauri::AppHandle;

use crate::downloader;
use crate::settings;

/// Minecraft 官方启动器客户端 ID（授权码流实测可用）
const MS_CLIENT_ID: &str = "00000000402b5328";
/// 授权端点（consumers tenant = 个人微软账户）
const MS_AUTHORIZE_URL: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/authorize";
/// 令牌端点（MSA v1，对官方 client_id 同时支持授权码与刷新令牌）
const MS_TOKEN_URL: &str = "https://login.live.com/oauth20_token.srf";
const MS_SCOPE: &str = "XboxLive.signin offline_access";
/// 单次登录会话有效期
const LOGIN_TIMEOUT: Duration = Duration::from_secs(600);

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
    /// 用户在浏览器中打开的授权地址
    pub login_url: String,
    /// loopback 回调地址
    pub redirect_uri: String,
    pub expires_in: u64,
}

/// 待完成的登录会话（loopback 回调或用户手动粘贴后填充）
struct PendingLogin {
    code: Option<String>,
    error: Option<String>,
    /// 授权时使用的 loopback 地址（换 token 必须一致）
    redirect_uri: String,
    deadline: Instant,
}

fn pending_slot() -> &'static Mutex<Option<PendingLogin>> {
    static SLOT: OnceLock<Mutex<Option<PendingLogin>>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(None))
}

/// 取出待处理的回调结果；超时/被拒返回 Err
fn take_pending() -> Result<Option<(String, String)>, String> {
    let mut guard = pending_slot().lock().unwrap();
    let Some(p) = guard.as_ref() else {
        return Ok(None);
    };
    if Instant::now() > p.deadline {
        *guard = None;
        return Err("登录超时，请重新开始".into());
    }
    if let Some(err) = &p.error {
        let msg = err.clone();
        *guard = None;
        return Err(msg);
    }
    Ok(p.code.clone().map(|c| (c, p.redirect_uri.clone())))
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

/// 第一步：起 loopback 服务并生成授权 URL（前端打开浏览器即可）
#[tauri::command]
pub async fn ms_login_start() -> Result<MsLoginStart, String> {
    let listener =
        TcpListener::bind(("127.0.0.1", 0)).map_err(|e| format!("无法启动本地登录服务: {e}"))?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let redirect_uri = format!("http://127.0.0.1:{port}");

    *pending_slot().lock().unwrap() = Some(PendingLogin {
        code: None,
        error: None,
        redirect_uri: redirect_uri.clone(),
        deadline: Instant::now() + LOGIN_TIMEOUT,
    });

    // 后台等待浏览器回调（登录成功后 302 回 loopback）
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let mut stream = match stream {
                Ok(s) => s,
                Err(_) => continue,
            };
            stream.set_read_timeout(Some(Duration::from_secs(10))).ok();
            let mut buf = [0u8; 4096];
            let n = stream.read(&mut buf).unwrap_or(0);
            if n == 0 {
                continue;
            }
            let request = String::from_utf8_lossy(&buf[..n]).to_string();
            let path = request
                .lines()
                .next()
                .and_then(|l| l.split(' ').nth(1))
                .unwrap_or("")
                .to_string();
            let query = path.split('?').nth(1).unwrap_or("").to_string();

            let (code, error) = parse_callback(&query);
            if code.is_some() || error.is_some() {
                let body = if error.is_some() {
                    "<html><body style=\"font-family:sans-serif;text-align:center;padding-top:80px\">\
                     <h2>授权未完成</h2><p>请返回 CCL 重试。</p></body></html>"
                } else {
                    "<html><body style=\"font-family:sans-serif;text-align:center;padding-top:80px\">\
                     <h2>登录成功</h2><p>请返回 CCL 启动器。</p></body></html>"
                };
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = stream.write_all(response.as_bytes());
                let _ = stream.flush();
                if let Some(p) = pending_slot().lock().unwrap().as_mut() {
                    p.code = code;
                    p.error = error;
                }
                break;
            }

            // favicon 等无关请求，回 404 继续等
            let response =
                "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
            let _ = stream.write_all(response.as_bytes());
        }
    });

    let login_url = format!(
        "{MS_AUTHORIZE_URL}?client_id={MS_CLIENT_ID}&scope={}&response_type=code&redirect_uri={}&ui_locales=zh-CN",
        url_encode(MS_SCOPE),
        url_encode(&redirect_uri),
    );

    Ok(MsLoginStart {
        login_url,
        redirect_uri,
        expires_in: LOGIN_TIMEOUT.as_secs(),
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

/// 第二步：轮询登录结果（loopback 收到 code 后自动完成后续链路）
#[tauri::command]
pub async fn ms_login_poll(app: AppHandle) -> Result<LoginPoll, String> {
    let (code, redirect_uri) = match take_pending() {
        Ok(Some(v)) => v,
        Ok(None) => return Ok(LoginPoll::Pending),
        Err(e) => return Ok(LoginPoll::Failed(e)),
    };
    finish_login(&app, &code, &redirect_uri).await
}

/// 兜底：用户从浏览器地址栏手动复制授权码后粘贴进来
#[tauri::command]
pub async fn ms_login_exchange(
    app: AppHandle,
    code: String,
    redirect_uri: String,
) -> Result<LoginPoll, String> {
    let mut code = code.trim().to_string();
    // 兼容整条回调 URL 粘贴（http://127.0.0.1:port/?code=xxx）
    if code.contains("code=") {
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
    let redirect_uri = redirect_uri.trim();
    if redirect_uri.is_empty() {
        return Ok(LoginPoll::Failed("缺少回调地址".into()));
    }
    // 清理 loopback 会话，避免与手动流程互相干扰
    *pending_slot().lock().unwrap() = None;
    finish_login(&app, &code, redirect_uri).await
}

/// code → token → XBL → XSTS → MC 档案，落盘账户
async fn finish_login(app: &AppHandle, code: &str, redirect_uri: &str) -> Result<LoginPoll, String> {
    let client = downloader::http_client();
    let (ms_token, refresh_token, expires_in) = exchange_code(&client, code, redirect_uri).await?;
    let account = complete_ms_login(&client, &ms_token, &refresh_token, expires_in).await?;
    let mut accounts = load_accounts(app);
    accounts.retain(|a| a.uuid != account.uuid);
    accounts.push(account.clone());
    save_accounts(app, &accounts)?;
    Ok(LoginPoll::Done(account))
}

/// 授权码 → MSA access/refresh token
async fn exchange_code(
    client: &reqwest::Client,
    code: &str,
    redirect_uri: &str,
) -> Result<(String, String, i64), String> {
    let resp = client
        .post(MS_TOKEN_URL)
        .form(&[
            ("client_id", MS_CLIENT_ID),
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", redirect_uri),
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
            ("client_id", MS_CLIENT_ID),
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
