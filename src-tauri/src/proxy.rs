//! 网络代理支持
//!
//! 国内直连 GitHub Release 资产只有 ~45 KB/s（一个 190 MB 的 JDK 要 70 分钟），
//! 而走 Clash 的 mixed 端口能到 ~190 KB/s。reqwest 默认不读系统代理，
//! 所以这里补上：
//! 1. `auto` 模式：Windows 读 `HKCU\...\Internet Settings` 的 ProxyServer/ProxyEnable；
//!    其他平台读 `HTTP_PROXY` / `HTTPS_PROXY` / `ALL_PROXY` 环境变量。
//! 2. 手动模式：用户直接填 `http://host:port` 或 `socks5://host:port`。
//!
//! 注意：`ProxyServer` 里常见 `127.0.0.1:7897` 这种没有 scheme 的裸地址，
//! 统一按 `http://` 补齐（Clash 的 mixed 端口同时接受 http 与 socks5 代理协议）。

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::OnceLock;
use std::time::Duration;

use reqwest::Proxy;

/// 代理模式：0 = 未设置（跟随全局），1 = 直连，2 = 自动，3 = 自定义
static MODE: AtomicU8 = AtomicU8::new(0);
/// 自定义代理地址
static CUSTOM: OnceLock<std::sync::RwLock<String>> = OnceLock::new();

fn custom_slot() -> &'static std::sync::RwLock<String> {
    CUSTOM.get_or_init(|| std::sync::RwLock::new(String::new()))
}

/// 设置全局代理配置（应用启动时与设置保存时调用）
pub fn set_proxy(setting: &str) {
    let s = setting.trim();
    let (mode, custom) = if s.is_empty() {
        (1u8, String::new())
    } else if s.eq_ignore_ascii_case("auto") {
        (2u8, String::new())
    } else {
        (3u8, s.to_string())
    };
    MODE.store(mode, Ordering::Relaxed);
    if let Ok(mut w) = custom_slot().write() {
        *w = custom;
    }
    // 切换到自动模式时清掉探测缓存，重新读一次系统代理
    if mode == 2 {
        clear_detected();
    }
}

/// 探测到的代理地址（`scheme://host:port`）。
/// 外层 `Some` = 已探测过；内层 `None` = 探测结果为「无系统代理」。
static DETECTED: OnceLock<std::sync::RwLock<Option<Option<String>>>> = OnceLock::new();

fn detected_slot() -> &'static std::sync::RwLock<Option<Option<String>>> {
    DETECTED.get_or_init(|| std::sync::RwLock::new(None))
}

/// 清除探测缓存（切换代理模式后重新探测系统代理）
pub fn clear_detected() {
    if let Ok(mut w) = detected_slot().write() {
        *w = None;
    }
}

/// 读取 Windows 注册表中的系统代理设置
#[cfg(windows)]
fn windows_system_proxy() -> Option<String> {
    // reg query "HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings" /v ProxyEnable
    let out = std::process::Command::new("reg")
        .args([
            "query",
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings",
            "/v",
            "ProxyEnable",
        ])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    // 形如 "    ProxyEnable    REG_DWORD    0x1"
    let enabled = text
        .lines()
        .find(|l| l.contains("ProxyEnable"))
        .and_then(|l| l.split_whitespace().last())
        .map(|v| v == "0x1")
        .unwrap_or(false);
    if !enabled {
        return None;
    }

    let out = std::process::Command::new("reg")
        .args([
            "query",
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings",
            "/v",
            "ProxyServer",
        ])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let raw = text
        .lines()
        .find(|l| l.contains("ProxyServer"))
        .map(|l| l.split_whitespace().nth(2).unwrap_or("").trim())
        .unwrap_or("");
    if raw.is_empty() {
        return None;
    }
    // 形如 "127.0.0.1:7897" 或 "http=127.0.0.1:7897;https=127.0.0.1:7897"
    let candidate = if raw.contains('=') {
        raw.split(';')
            .find_map(|kv| kv.split_once('=').map(|(_, v)| v))
            .unwrap_or(raw)
    } else {
        raw
    };
    Some(candidate.trim().to_string())
}

/// 从环境变量读取代理
fn env_proxy() -> Option<String> {
    for key in ["ALL_PROXY", "HTTPS_PROXY", "HTTP_PROXY"] {
        if let Ok(v) = std::env::var(key) {
            let v = v.trim();
            if !v.is_empty() {
                return Some(v.to_string());
            }
        }
    }
    None
}

/// 自动探测系统代理，返回规范化的 `scheme://host:port`
pub fn detect_system_proxy() -> Option<String> {
    {
        if let Ok(rd) = detected_slot().read() {
            if let Some(cached) = rd.as_ref() {
                // 缓存的是「探测结果」：内层 None 表示确实没有系统代理
                return cached.clone();
            }
        }
    }
    let found = {
        let raw = {
            #[cfg(windows)]
            {
                windows_system_proxy()
            }
            #[cfg(not(windows))]
            {
                None
            }
        };
        raw.or_else(env_proxy).and_then(|v| normalize(&v))
    };
    if let Ok(mut w) = detected_slot().write() {
        *w = Some(found.clone());
    }
    found
}

/// 代理端口是否真的连得上（避免代理软件没开时全盘失败）
fn proxy_reachable(addr: &str) -> bool {
    let hostport = addr
        .split("://")
        .nth(1)
        .unwrap_or(addr)
        .split('/')
        .next()
        .unwrap_or("");
    let (host, port) = match hostport.rsplit_once(':') {
        Some((h, p)) => (h.to_string(), p.parse::<u16>().unwrap_or(0)),
        None => return false,
    };
    if port == 0 || host.is_empty() {
        return false;
    }
    // 缓存探测结果，避免每次建客户端都卡 1.5 秒超时；60 秒 TTL，
    // 这样代理软件中途启动后能自动被感知到
    static CACHE: OnceLock<std::sync::Mutex<std::collections::HashMap<String, (std::time::Instant, bool)>>> =
        OnceLock::new();
    let cache = CACHE.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()));
    if let Ok(map) = cache.lock() {
        if let Some((t, v)) = map.get(addr) {
            if t.elapsed() < Duration::from_secs(60) {
                return *v;
            }
        }
    }
    use std::net::{SocketAddr, ToSocketAddrs};
    let addrs: Vec<SocketAddr> = match (host.as_str(), port).to_socket_addrs() {
        Ok(it) => it.collect(),
        Err(_) => return false,
    };
    let ok = addrs
        .iter()
        .any(|a| std::net::TcpStream::connect_timeout(a, Duration::from_millis(1500)).is_ok());
    if let Ok(mut map) = cache.lock() {
        map.insert(addr.to_string(), (std::time::Instant::now(), ok));
    }
    ok
}

/// 把裸地址（`127.0.0.1:7897`）补成带 scheme 的 URL
fn normalize(raw: &str) -> Option<String> {
    let s = raw.trim();
    if s.is_empty() {
        return None;
    }
    if s.contains("://") {
        return Some(s.to_string());
    }
    // 裸 host:port
    if s.contains(':') && !s.contains('/') {
        return Some(format!("http://{s}"));
    }
    None
}

/// 依据全局设置构造代理对象
pub fn build_proxy() -> Option<Proxy> {
    match MODE.load(Ordering::Relaxed) {
        1 => None,
        2 => {
            let addr = detect_system_proxy()?;
            // 系统代理开着但代理软件没运行 → 宁可直连也不要全盘失败
            if !proxy_reachable(&addr) {
                return None;
            }
            Proxy::all(addr).ok()
        }
        3 => {
            let s = custom_slot().read().ok()?.trim().to_string();
            let addr = normalize(&s)?;
            if !proxy_reachable(&addr) {
                return None;
            }
            Proxy::all(addr).ok()
        }
        _ => None,
    }
}

/// 构造带代理的 HTTP 客户端
pub fn client() -> reqwest::Client {
    let mut builder = reqwest::Client::builder()
        .user_agent("CwjCraftLauncher/1.0 (+https://caiwenjun.top)")
        .connect_timeout(Duration::from_secs(15));
    if let Some(p) = build_proxy() {
        builder = builder.proxy(p);
    }
    builder.build().unwrap_or_default()
}

/// 无代理客户端（用于纯本地/内网请求）
pub fn plain_client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent("CwjCraftLauncher/1.0 (+https://caiwenjun.top)")
        .connect_timeout(Duration::from_secs(15))
        .build()
        .unwrap_or_default()
}

/// 当前生效的代理描述（供前端展示）
pub fn current_description() -> String {
    match MODE.load(Ordering::Relaxed) {
        1 => "直连（不使用代理）".to_string(),
        2 => match detect_system_proxy() {
            Some(a) => format!("自动（系统代理 {a}）"),
            None => "自动（未检测到系统代理，实际直连）".to_string(),
        },
        3 => {
            let s = custom_slot().read().map(|g| g.clone()).unwrap_or_default();
            s.trim().to_string()
        }
        _ => "未设置".to_string(),
    }
}

/// 实际使用的代理地址（None = 直连）
pub fn effective_addr() -> Option<String> {
    match MODE.load(Ordering::Relaxed) {
        1 => None,
        2 => detect_system_proxy(),
        3 => {
            let s = custom_slot().read().ok()?.trim().to_string();
            normalize(&s)
        }
        _ => None,
    }
}

/// 检测系统代理（暴露给前端命令）
#[tauri::command]
pub fn proxy_detect() -> Option<String> {
    detect_system_proxy()
}

/// 连通性测试：走当前代理配置请求一个 URL，返回耗时与状态码
#[tauri::command]
pub async fn proxy_test(url: Option<String>) -> Result<String, String> {
    let target = url
        .filter(|u| !u.trim().is_empty())
        .unwrap_or_else(|| "https://api.adoptium.net/v3/info/available_releases".to_string());
    let started = std::time::Instant::now();
    let resp = client()
        .get(&target)
        .send()
        .await
        .map_err(|e| format!("请求失败：{e}"))?;
    let status = resp.status();
    let ms = started.elapsed().as_millis();
    Ok(format!(
        "HTTP {} · {} ms · 代理：{}",
        status.as_u16(),
        ms,
        current_description()
    ))
}
