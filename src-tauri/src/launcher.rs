//! 游戏启动：classpath 组装 / 参数规则 / natives / 进程管理 / 日志流

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::auth::{self, Account};
use crate::java;
use crate::settings;

#[derive(Debug, Clone, Serialize)]
pub struct GameLogEvent {
    pub instance_id: String,
    pub line: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct GameExitEvent {
    pub instance_id: String,
    pub code: Option<i32>,
}

pub struct GameProcesses(pub Arc<Mutex<HashMap<String, Child>>>);

#[derive(Debug, Deserialize)]
struct VersionJson {
    id: String,
    #[serde(rename = "mainClass")]
    main_class: String,
    #[serde(default)]
    arguments: Option<Arguments>,
    #[serde(rename = "minecraftArguments", default)]
    minecraft_arguments: Option<String>,
    #[serde(rename = "assetIndex", default)]
    asset_index: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct Arguments {
    #[serde(default)]
    game: Vec<Value>,
    #[serde(default)]
    jvm: Vec<Value>,
}

/// rules 判定（Windows x64，无自定义分辨率/演示模式）
fn rule_allows(value: &Value) -> bool {
    match value.get("rules") {
        None => true,
        Some(Value::Array(rules)) => {
            // 规范：存在 rules 时默认不允许，由匹配的 rule 决定
            // （修复 --demo / -XstartOnFirstThread / quickPlay 等 features/os 参数未被过滤的问题）
            let mut allowed = false;
            for rule in rules {
                let action = rule.get("action").and_then(|a| a.as_str()).unwrap_or("allow");
                let os_match = rule
                    .get("os")
                    .map(|os| os.get("name").and_then(|n| n.as_str()) == Some("windows"))
                    .unwrap_or(true);
                let features_ok = rule
                    .get("features")
                    .map(|f| f.as_object().map(|m| m.is_empty()).unwrap_or(false))
                    .unwrap_or(true);
                if os_match && features_ok {
                    allowed = action == "allow";
                }
            }
            allowed
        }
        _ => true,
    }
}

/// 从 argument 条目提取字符串列表（支持 "str" 与 {rules, value: [...]}）
fn arg_strings(value: &Value) -> Vec<String> {
    if !rule_allows(value) {
        return vec![];
    }
    match value {
        Value::String(s) => vec![s.clone()],
        Value::Object(o) => {
            if let Some(Value::Array(arr)) = o.get("value") {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            } else if let Some(Value::String(s)) = o.get("value") {
                vec![s.clone()]
            } else {
                vec![]
            }
        }
        _ => vec![],
    }
}

/// 组装 classpath：版本 jar + 所有允许的 libraries
fn build_classpath(game_dir: &PathBuf, version_id: &str, mc_version: &str, vjson: &Value) -> String {
    let mut paths: Vec<String> = vec![];
    // 加载器实例的原版 client jar 落在父版本目录（versions/<mc>/<mc>.jar），
    // 合并安装时不会复制到 loader 目录，但启动必须带上，否则找不到 net.minecraft.*
    if version_id != mc_version {
        let parent_jar = game_dir
            .join("versions")
            .join(mc_version)
            .join(format!("{mc_version}.jar"));
        if parent_jar.exists() {
            paths.push(parent_jar.to_string_lossy().to_string());
        }
    }
    let version_jar = game_dir
        .join("versions")
        .join(version_id)
        .join(format!("{version_id}.jar"));
    if version_jar.exists() {
        paths.push(version_jar.to_string_lossy().to_string());
    }
    if let Some(Value::Array(libs)) = vjson.get("libraries") {
        for lib in libs {
            if !rule_allows(lib) {
                continue;
            }
            let name = lib.get("name").and_then(|n| n.as_str()).unwrap_or("");
            // 新格式 natives（org.lwjgl:lwjgl:3.3.3:natives-windows）只含 dll，不进 classpath
            let name_suffix = name.split(':').last().unwrap_or("");
            if name_suffix.starts_with("natives-") {
                continue;
            }
            let artifact_path = lib
                .get("downloads")
                .and_then(|d| d.get("artifact"))
                .and_then(|a| a.get("path"))
                .and_then(|p| p.as_str())
                .map(|p| game_dir.join("libraries").join(p.replace('\\', "/")));
            let path = match artifact_path {
                Some(p) if p.exists() => p,
                _ => {
                    let guessed = game_dir
                        .join("libraries")
                        .join(maven_path_from_name(name));
                    if guessed.exists() {
                        guessed
                    } else {
                        continue;
                    }
                }
            };
            paths.push(path.to_string_lossy().to_string());
        }
    }
    // classpath 分隔符必须跟平台一致：Windows 是 ';'，Linux / Android / macOS 是 ':'。
    // 之前写死 ';' 在桌面上没问题，但 Android 上 java 会把整串当成一个文件名，
    // 表现为 `Could not find or load main class net.minecraft.client.main.Main`。
    paths.join(if cfg!(windows) { ";" } else { ":" })
}

fn maven_path_from_name(name: &str) -> PathBuf {
    let parts: Vec<&str> = name.split(':').collect();
    if parts.len() < 3 {
        return PathBuf::from(name.replace(':', "/"));
    }
    let group = parts[0].replace('.', "/");
    let (artifact, version, classifier) = (parts[1], parts[2], parts.get(3).copied());
    let file = match classifier {
        Some(c) => format!("{artifact}-{version}-{c}.jar"),
        None => format!("{artifact}-{version}.jar"),
    };
    PathBuf::from(format!("{group}/{artifact}/{version}/{file}"))
}

fn substitute(arg: &str, vars: &HashMap<String, String>) -> String {
    let mut out = arg.to_string();
    for (k, v) in vars {
        out = out.replace(&format!("${{{k}}}"), v);
    }
    out
}

#[derive(Debug, Clone, Serialize)]
pub struct LaunchInfo {
    pub instance_id: String,
    pub version_id: String,
    pub java_major: u32,
}

/// 读取原版（父）版本 JSON 作为启动参数模板；本地没有就从版本清单拉一份并缓存。
/// 用于修复早期安装的加载器实例缺失原版启动参数（--username 等）的问题。
/// 缓存放 CCL 数据目录而不是 versions/，避免被当成「已安装版本」。
async fn ensure_parent_version_json(
    app: &AppHandle,
    mc_version: &str,
) -> Option<Value> {
    let cache_dir = crate::settings::data_dir(app).join("vanilla-json");
    let cache = cache_dir.join(format!("{mc_version}.json"));
    if let Ok(text) = std::fs::read_to_string(&cache) {
        if let Ok(v) = serde_json::from_str::<Value>(&text) {
            return Some(v);
        }
    }
    let manifest = crate::minecraft::mc_version_manifest(app.clone()).await.ok()?;
    let entry = manifest.into_iter().find(|e| e.id == mc_version)?;
    let client = crate::downloader::http_client();
    let resp = client
        .get(&entry.url)
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await
        .ok()?;
    let v: Value = resp.json().await.ok()?;
    let _ = std::fs::create_dir_all(&cache_dir);
    if let Ok(text) = serde_json::to_string(&v) {
        let _ = std::fs::write(&cache, text);
    }
    Some(v)
}

/// 启动实例
#[tauri::command]
pub async fn launch_instance(
    app: AppHandle,
    processes: State<'_, GameProcesses>,
    instance_id: String,
    account_uuid: Option<String>,
) -> Result<LaunchInfo, String> {
    // 已在运行则直接返回
    {
        let mut guard = processes.0.lock().unwrap();
        if let Some(child) = guard.get_mut(&instance_id) {
            if child.try_wait().ok().flatten().is_none() {
                return Err("该实例正在运行中".into());
            }
        }
    }

    let instance = crate::instances::get_instance(&app, &instance_id)?;
    let s = settings::load(&app);

    // 账户：指定 UUID → 该账户；否则第一个账户；一个都没有 → 自动创建离线账户（不打断启动）
    let mut auto_created = false;
    let account: Account = {
        let accounts = accounts_list(&app);
        match account_uuid {
            Some(u) => accounts
                .iter()
                .find(|a| a.uuid == u)
                .cloned()
                .or_else(|| accounts.first().cloned())
                .unwrap_or_else(|| {
                    auto_created = true;
                    make_offline_account(&app)
                }),
            None => accounts.first().cloned().unwrap_or_else(|| {
                auto_created = true;
                make_offline_account(&app)
            }),
        }
    };
    if auto_created {
        let _ = app.emit(
            "game-log",
            GameLogEvent {
                instance_id: instance_id.clone(),
                line: format!(
                    "[CCL] 未检测到账户，已自动创建离线账户 {}（可在账户页管理）",
                    account.name
                ),
            },
        );
    }

    // 令牌将过期则刷新
    let account = if account.account_type == "msa"
        && account.expires_at > 0
        && account.expires_at - chrono::Utc::now().timestamp() < 300
    {
        auth::refresh_account(app.clone(), account.uuid.clone())
            .await
            .unwrap_or(account)
    } else {
        account
    };

    // Java 选择推迟到读完版本 JSON 之后，才能按 JSON 里声明的 javaVersion 精确匹配
    let instance_java = instance
        .java_path
        .as_ref()
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty());

    // MC 根：versions / libraries / assets 都在这里（新布局恒为 .minecraft）
    let mc_root = crate::paths::mc_root(&app, &instance);
    // --gameDir：由版本隔离策略决定（PCL CE 的 PathIndie）
    let game_dir = crate::paths::resolve_game_dir(&app, &instance);
    let version_id = instance
        .installed_version_id
        .clone()
        .unwrap_or_else(|| instance.minecraft_version.clone());

    let vjson_path = mc_root
        .join("versions")
        .join(&version_id)
        .join(format!("{version_id}.json"));
    if !vjson_path.exists() {
        return Err(format!("版本 {version_id} 未安装，请先在版本页安装"));
    }
    let vjson_text = std::fs::read_to_string(&vjson_path).map_err(|e| e.to_string())?;
    let vjson: Value = serde_json::from_str(&vjson_text).map_err(|e| e.to_string())?;
    let parsed: VersionJson = serde_json::from_value(vjson.clone()).map_err(|e| e.to_string())?;

    // Java 选择优先级：实例指定 → 全局指定 → 自动。
    // 需要的主版本优先取版本 JSON 的 javaVersion.majorVersion（最准，PCL 也这么做），
    // 缺失时才按 MC 版本号推断（26.x 这类新命名靠推断会翻车）。
    let need_java = vjson
        .get("javaVersion")
        .and_then(|j| j.get("majorVersion"))
        .and_then(|m| m.as_u64())
        .map(|m| m as u32)
        .unwrap_or_else(|| java::required_java_major(&instance.minecraft_version));
    let java_info = if let Some(path) = instance_java {
        java::JavaInfo {
            path,
            major: need_java,
            version: String::new(),
            vendor: "实例指定".into(),
            managed: false,
        }
    } else if !s.java_path.is_empty() {
        java::JavaInfo {
            path: s.java_path.clone(),
            major: need_java,
            version: String::new(),
            vendor: "手动指定".into(),
            managed: false,
        }
    } else {
        java::auto_select_java_with(&app, need_java, &instance.minecraft_version)?
    };

    // 兼容旧版合并缺陷：早期安装的加载器版本 JSON 只保留了 loader 自己的 arguments，
    // 原版的 game 参数（--username / --gameDir / --assetsDir 等）被整体覆盖丢失。
    // 这里若发现 game 参数为空，就从同实例下的原版版本 JSON 补回一份。
    let mut base_args: Option<(Vec<Value>, Vec<Value>)> = None;
    let game_args_empty = vjson
        .pointer("/arguments/game")
        .and_then(|g| g.as_array())
        .map(|a| a.is_empty())
        .unwrap_or(false);
    if game_args_empty && version_id != instance.minecraft_version {
        // 父版本 JSON 可能没被装上（早期整合包导入只装了 loader 版本），必要时补拉一份
        let parent = ensure_parent_version_json(&app, &instance.minecraft_version).await;
        if let Some(pv) = parent {
            let g = pv
                .pointer("/arguments/game")
                .and_then(|x| x.as_array())
                .cloned()
                .unwrap_or_default();
            let j = pv
                .pointer("/arguments/jvm")
                .and_then(|x| x.as_array())
                .cloned()
                .unwrap_or_default();
            if !g.is_empty() {
                base_args = Some((g, j));
            }
        }
    }

    // 加载器实例必须先有原版本体（client jar），否则一定启动失败
    if version_id != instance.minecraft_version {
        let parent_jar = mc_root
            .join("versions")
            .join(&instance.minecraft_version)
            .join(format!("{}.jar", instance.minecraft_version));
        if !parent_jar.exists() {
            return Err(format!(
                "缺少游戏本体：{} 的客户端 jar 不存在。请在「下载 → 游戏本体」重新下载该版本",
                instance.minecraft_version
            ));
        }
    }

    let classpath = build_classpath(&mc_root, &version_id, &instance.minecraft_version, &vjson);
    if classpath.is_empty() {
        return Err("classpath 为空，版本可能未完整安装".into());
    }
    let natives_dir = mc_root.join("versions").join(&version_id).join("natives");
    let assets_root = mc_root.join("assets");
    let asset_index_name = parsed
        .asset_index
        .as_ref()
        .and_then(|a| a.get("id"))
        .and_then(|i| i.as_str())
        .unwrap_or("legacy")
        .to_string();

    let mut vars: HashMap<String, String> = HashMap::new();
    vars.insert("auth_player_name".into(), account.name.clone());
    vars.insert("auth_uuid".into(), account.uuid.clone());
    vars.insert("auth_access_token".into(), account.access_token.clone());
    vars.insert("user_type".into(), account.account_type.clone());
    vars.insert("version_type".into(), "release".into());
    vars.insert("game_directory".into(), game_dir.to_string_lossy().to_string());
    vars.insert("assets_root".into(), assets_root.to_string_lossy().to_string());
    vars.insert("assets_index_name".into(), asset_index_name);
    vars.insert("natives_directory".into(), natives_dir.to_string_lossy().to_string());
    vars.insert("classpath".into(), classpath.clone());
    vars.insert("version".into(), version_id.clone());
    vars.insert("version_name".into(), version_id.clone());
    // 初始窗口尺寸：实例设置优先，否则取启动器窗口尺寸，失败则回退 854x480
    let (rw, rh) = match (instance.window_width, instance.window_height) {
        (Some(w), Some(h)) if w >= 320 && h >= 240 => (w.to_string(), h.to_string()),
        _ => app
            .get_webview_window("main")
            .and_then(|w| w.inner_size().ok())
            .map(|s| (s.width.to_string(), s.height.to_string()))
            .unwrap_or_else(|| ("854".into(), "480".into())),
    };
    vars.insert("resolution_width".into(), rw);
    vars.insert("resolution_height".into(), rh);
    vars.insert("clientid".into(), "ccl-client".into());
    vars.insert("auth_xuid".into(), String::new());
    vars.insert("user_properties".into(), "{}".into());
    vars.insert("quickPlayPath".into(), String::new());
    vars.insert("quickPlaySingleplayer".into(), String::new());
    vars.insert("quickPlayMultiplayer".into(), String::new());
    vars.insert("quickPlayRealms".into(), String::new());
    vars.insert("launcher_name".into(), "CwjCraftLauncher".into());
    vars.insert("launcher_version".into(), env!("CARGO_PKG_VERSION").into());

    // ---- JVM 参数（实例设置 > 全局设置）
    let memory = instance.max_memory.unwrap_or(s.max_memory);
    let mut jvm_args: Vec<String> = Vec::new();
    if let Some(min) = instance.min_memory {
        if min > 0 {
            jvm_args.push(format!("-Xms{min}M"));
        }
    }
    jvm_args.push(format!("-Xmx{memory}M"));
    let extra_jvm = instance
        .jvm_args
        .as_deref()
        .map(|v| v.trim())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| s.jvm_args.trim());
    if !extra_jvm.is_empty() {
        for part in extra_jvm.split_whitespace() {
            jvm_args.push(part.to_string());
        }
    }
    jvm_args.push(format!(
        "-Djava.library.path={}",
        natives_dir.to_string_lossy()
    ));
    if let Some((_, base_jvm)) = &base_args {
        for a in base_jvm {
            jvm_args.extend(arg_strings(a).into_iter().map(|s| substitute(&s, &vars)));
        }
    }
    if let Some(args) = &parsed.arguments {
        for a in &args.jvm {
            jvm_args.extend(arg_strings(a).into_iter().map(|s| substitute(&s, &vars)));
        }
    }
    if base_args.is_none() && parsed.arguments.is_none() {
        jvm_args.push("-cp".into());
        jvm_args.push(classpath.clone());
    }

    // ---- 游戏参数
    let mut game_args: Vec<String> = vec![];
    if let Some((base_game, _)) = &base_args {
        for a in base_game {
            game_args.extend(arg_strings(a).into_iter().map(|s| substitute(&s, &vars)));
        }
    }
    if let Some(args) = &parsed.arguments {
        for a in &args.game {
            game_args.extend(arg_strings(a).into_iter().map(|s| substitute(&s, &vars)));
        }
    } else if let Some(legacy) = &parsed.minecraft_arguments {
        for part in legacy.split(' ') {
            game_args.push(substitute(part, &vars));
        }
    }

    // ---- 实例级游戏参数追加（附加参数 → 全屏 → 自动进服）
    if let Some(extra) = instance.game_args.as_deref().map(|v| v.trim()).filter(|v| !v.is_empty()) {
        for part in extra.split_whitespace() {
            game_args.push(substitute(part, &vars));
        }
    }
    if instance.fullscreen {
        game_args.push("--fullscreen".into());
    }
    if let Some(addr) = instance
        .auto_join_server
        .as_deref()
        .map(|v| v.trim())
        .filter(|v| !v.is_empty())
    {
        // 1.20 起 --server/--port 被移除，改用 --quickPlayMultiplayer
        // 注意版本号有两种命名：1.21.11（major=1）与 26.2（major=26），都不能只看第二段
        let modern = {
            let mut it = instance.minecraft_version.split('.');
            let major: u32 = it.next().and_then(|s| s.parse().ok()).unwrap_or(1);
            if major > 1 {
                true
            } else {
                it.next().and_then(|s| s.parse::<u32>().ok()).unwrap_or(0) >= 20
            }
        };
        if modern {
            game_args.push("--quickPlayMultiplayer".into());
            game_args.push(addr.to_string());
        } else {
            let (host, port) = match addr.rsplit_once(':') {
                Some((h, p)) if p.chars().all(|c| c.is_ascii_digit()) => (h, p),
                _ => (addr, "25565"),
            };
            game_args.push("--server".into());
            game_args.push(host.to_string());
            game_args.push("--port".into());
            game_args.push(port.to_string());
        }
    }

    let _ = app.emit(
        "game-log",
        GameLogEvent {
            instance_id: instance_id.clone(),
            line: format!(
                "[CCL] 启动 {} | Java {} | 账户 {}",
                version_id, java_info.major, account.name
            ),
        },
    );

    let mut cmd = Command::new(&java_info.path);
    cmd.args(&jvm_args)
        .arg(&parsed.main_class)
        .args(&game_args)
        .current_dir(&game_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    // Android：JRE 的 `bin/java` 动态依赖 `lib/libjli.so`，而 Android 的 linker
    // 不按 rpath 搜索，必须显式给 LD_LIBRARY_PATH，否则 exec 成功但立刻退出，
    // 报 `CANNOT LINK EXECUTABLE ... library "libjli.so" not found`。
    #[cfg(target_os = "android")]
    {
        let java_path = std::path::Path::new(&java_info.path);
        if let Some(root) = java_path.parent().and_then(|p| p.parent()) {
            let lib = root.join("lib");
            let server = lib.join("server");
            let ld = format!("{}:{}", lib.display(), server.display());
            cmd.env("LD_LIBRARY_PATH", ld);
        }
    }

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let mut child = cmd.spawn().map_err(|e| format!("启动进程失败: {e}"))?;

    // 启动诊断日志：完整命令行 + 退出码，便于排查启动失败
    {
        let dbg_log = mc_root.join("ccl-launch.log");
        let mut line = format!(
            "[{}] java={} cwd={}\n  {}\n",
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
            java_info.path,
            game_dir.display(),
            std::iter::once(java_info.path.as_str())
                .chain(jvm_args.iter().map(|s| s.as_str()))
                .chain(std::iter::once(parsed.main_class.as_str()))
                .chain(game_args.iter().map(|s| s.as_str()))
                .collect::<Vec<_>>()
                .join(" ")
        );
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&dbg_log) {
            let _ = f.write_all(line.as_bytes());
        }
        line.clear();
        let _ = line;
    }

    // 日志流
    if let Some(out) = child.stdout.take() {
        let iid = instance_id.clone();
        let app_out = app.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(out).lines().map_while(Result::ok) {
                let _ = app_out.emit(
                    "game-log",
                    GameLogEvent { instance_id: iid.clone(), line },
                );
            }
        });
    }
    if let Some(err) = child.stderr.take() {
        let iid = instance_id.clone();
        let app_err = app.clone();
        let err_log = mc_root.join("ccl-launch.log");
        std::thread::spawn(move || {
            // 前若干行同时落盘，启动失败时可直接看 ccl-launch.log 定位原因
            let mut logged = 0;
            for line in BufReader::new(err).lines().map_while(Result::ok) {
                if logged < 40 {
                    logged += 1;
                    use std::io::Write;
                    if let Ok(mut f) = std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(&err_log)
                    {
                        let _ = writeln!(f, "[stderr] {line}");
                    }
                }
                let _ = app_err.emit(
                    "game-log",
                    GameLogEvent { instance_id: iid.clone(), line },
                );
            }
        });
    }

    // 退出监听（try_lock 轮询，避免阻塞 stop_instance）
    let iid3 = instance_id.clone();
    let app_exit = app.clone();
    let processes_arc = processes.0.clone();
    std::thread::spawn(move || {
        let code = loop {
            {
                let mut guard = match processes_arc.try_lock() {
                    Ok(g) => g,
                    Err(_) => {
                        std::thread::sleep(Duration::from_millis(300));
                        continue;
                    }
                };
                match guard.get_mut(&iid3) {
                    Some(child) => match child.try_wait() {
                        Ok(Some(status)) => {
                            let code = status.code();
                            guard.remove(&iid3);
                            break code;
                        }
                        _ => {}
                    },
                    None => break None,
                }
            }
            std::thread::sleep(Duration::from_millis(500));
        };
        let _ = app_exit.emit("game-exit", GameExitEvent { instance_id: iid3.clone(), code });
        // 退出码追加到诊断日志
        {
            use std::io::Write;
            if let Some(dir) = app_exit.path().app_data_dir().ok() {
                // 从进程表反查 game_dir 不可靠，直接读实例数据
                let _ = dir;
            }
            if let Ok(inst) = crate::instances::get_instance(&app_exit, &iid3) {
                let dbg_log = crate::paths::mc_root(&app_exit, &inst).join("ccl-launch.log");
                if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&dbg_log) {
                    let _ = writeln!(
                        f,
                        "[{}] 进程退出 code={:?}",
                        chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
                        code
                    );
                }
            }
        }
    });

    processes.0.lock().unwrap().insert(instance_id.clone(), child);

    // 记录最近游玩时间（主页按此排序）
    {
        let mut updated = instance.clone();
        updated.last_played = Some(chrono::Utc::now().to_rfc3339());
        let _ = crate::instances::save_instance(&app, &updated);
    }

    Ok(LaunchInfo {
        instance_id,
        version_id,
        java_major: java_info.major,
    })
}

/// 停止实例
#[tauri::command]
pub fn stop_instance(
    processes: State<'_, GameProcesses>,
    instance_id: String,
) -> Result<(), String> {
    let mut guard = processes.0.lock().unwrap();
    if let Some(child) = guard.get_mut(&instance_id) {
        child.kill().map_err(|e| format!("结束进程失败: {e}"))?;
    }
    guard.remove(&instance_id);
    Ok(())
}

#[tauri::command]
pub fn is_instance_running(
    processes: State<'_, GameProcesses>,
    instance_id: String,
) -> bool {
    let mut guard = processes.0.lock().unwrap();
    if let Some(child) = guard.get_mut(&instance_id) {
        child.try_wait().ok().flatten().is_none()
    } else {
        false
    }
}

fn accounts_list(app: &AppHandle) -> Vec<Account> {
    auth::load_accounts(app)
}

/// 没有任何账户时自动创建离线账户：优先取系统用户名（过滤非法字符），失败则 Steve
fn make_offline_account(app: &AppHandle) -> Account {
    let raw = std::env::var("USERNAME").unwrap_or_default();
    let candidate: String = raw
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .take(16)
        .collect();
    let name = if candidate.is_empty() { "Steve".to_string() } else { candidate };
    auth::offline_login(app.clone(), name)
        .or_else(|_| auth::offline_login(app.clone(), "Steve".to_string()))
        .expect("创建离线账户失败")
}
