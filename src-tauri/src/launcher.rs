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
fn build_classpath(game_dir: &PathBuf, version_id: &str, vjson: &Value) -> String {
    let mut paths: Vec<String> = vec![];
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
    paths.join(";")
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

    let java_info = if !s.java_path.is_empty() {
        java::JavaInfo {
            path: s.java_path.clone(),
            major: java::required_java_major(&instance.minecraft_version),
            version: String::new(),
            vendor: "手动指定".into(),
            managed: false,
        }
    } else {
        java::auto_select_java(&app, &instance.minecraft_version)?
    };

    let game_dir = PathBuf::from(&instance.game_dir);
    let version_id = instance
        .installed_version_id
        .clone()
        .unwrap_or_else(|| instance.minecraft_version.clone());

    let vjson_path = game_dir
        .join("versions")
        .join(&version_id)
        .join(format!("{version_id}.json"));
    if !vjson_path.exists() {
        return Err(format!("版本 {version_id} 未安装，请先在版本页安装"));
    }
    let vjson_text = std::fs::read_to_string(&vjson_path).map_err(|e| e.to_string())?;
    let vjson: Value = serde_json::from_str(&vjson_text).map_err(|e| e.to_string())?;
    let parsed: VersionJson = serde_json::from_value(vjson.clone()).map_err(|e| e.to_string())?;

    let classpath = build_classpath(&game_dir, &version_id, &vjson);
    if classpath.is_empty() {
        return Err("classpath 为空，版本可能未完整安装".into());
    }
    let natives_dir = game_dir.join("versions").join(&version_id).join("natives");
    let assets_root = game_dir.join("assets");
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
    // 初始窗口尺寸：取启动器窗口尺寸，失败则回退 854x480
    let (rw, rh) = app
        .get_webview_window("main")
        .and_then(|w| w.inner_size().ok())
        .map(|s| (s.width.to_string(), s.height.to_string()))
        .unwrap_or_else(|| ("854".into(), "480".into()));
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

    // ---- JVM 参数
    let memory = instance.max_memory.unwrap_or(s.max_memory);
    let mut jvm_args: Vec<String> = vec![format!("-Xmx{memory}M")];
    if !s.jvm_args.trim().is_empty() {
        for part in s.jvm_args.split_whitespace() {
            jvm_args.push(part.to_string());
        }
    }
    jvm_args.push(format!(
        "-Djava.library.path={}",
        natives_dir.to_string_lossy()
    ));
    if let Some(args) = &parsed.arguments {
        for a in &args.jvm {
            jvm_args.extend(arg_strings(a).into_iter().map(|s| substitute(&s, &vars)));
        }
    } else {
        jvm_args.push("-cp".into());
        jvm_args.push(classpath.clone());
    }

    // ---- 游戏参数
    let mut game_args: Vec<String> = vec![];
    if let Some(args) = &parsed.arguments {
        for a in &args.game {
            game_args.extend(arg_strings(a).into_iter().map(|s| substitute(&s, &vars)));
        }
    } else if let Some(legacy) = &parsed.minecraft_arguments {
        for part in legacy.split(' ') {
            game_args.push(substitute(part, &vars));
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

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let mut child = cmd.spawn().map_err(|e| format!("启动进程失败: {e}"))?;

    // 启动诊断日志：完整命令行 + 退出码，便于排查启动失败
    {
        let dbg_log = game_dir.join("ccl-launch.log");
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
        std::thread::spawn(move || {
            for line in BufReader::new(err).lines().map_while(Result::ok) {
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
                let dbg_log = std::path::Path::new(&inst.game_dir).join("ccl-launch.log");
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
