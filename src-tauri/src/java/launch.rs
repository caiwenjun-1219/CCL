//! Android 启动编排（M4-b）
//!
//! ## 为什么不复用 `launcher.rs`
//!
//! 桌面是 `Command::new(java)` 起子进程。Android 上这条路走不通：游戏要的
//! `ANativeWindow` 来自 Activity 的 `SurfaceView`，子进程既拿不到它，
//! 也够不到 `libpojavexec` 的 JNI 桥。所以必须**在 App 进程内**用内嵌 JVM
//! 调 `mainClass`（M1 已经把 `JNI_CreateJavaVM` 打通了）。
//!
//! ## classpath 一旦建好就改不了
//!
//! `JNI_CreateJavaVM` 的 `-Djava.class.path` 只能在**建 VM 那一刻**给。
//! 所以调用顺序必须是：
//!
//! 1. [`plan`] 算出完整 classpath
//! 2. 用这份 classpath 建 VM（`-Djava.class.path=<plan.classpath>`）
//! 3. 在 VM 里反射调 `mainClass.main(String[])`
//!
//! 顺序反了（比如先跑了 M2 探针，VM 已用那个三 jar 的 classpath 建好），
//! 游戏类就永远找不到 —— 只能重启进程。`plan()` 会检查这点并给出提示。
//!
//! ## 排序规则
//!
//! `lwjgl-classes.jar`（Pojav 的 fork 版）必须压过版本 JSON 带来的官方
//! `org.lwjgl:*`。做法不是「排在前面」而是**直接剔掉**被 fork 覆盖的那几个
//! 模块 —— 官方 jar 里躺的是桌面 natives 的绑定，留在 classpath 里只是
//! 一颗定时炸弹（谁先加载谁说了算，而 `-Djava.class.path` 的顺序在不同
//! JVM 实现上并不保证被尊重）。
//!
//! 但**不能一刀切**：fork jar 里没有 `org.lwjgl.freetype` 与
//! `org.lwjgl.util.tinyfd` 的类，这两个还得用官方 jar；它们的 native
//! 分别是 `libfreetype.so` / `liblwjgl_tinyfd.so`，正好都在我们的
//! natives 目录里。

use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use tauri::AppHandle;

use crate::launcher::{
    arg_strings, collect_classpath_entries, is_replaced_lwjgl, join_classpath, substitute,
};

/// 桌面专属、Android 上必须丢掉的 JVM 参数
fn is_desktop_only_jvm_arg(a: &str) -> bool {
    a == "-XstartOnFirstThread"
        || a.starts_with("-Djava.library.path=")
        || a.starts_with("-Dorg.lwjgl.opengl.libname=")
        || a.starts_with("-Xmx")
        || a.starts_with("-Xms")
}

/// 一次启动的完整计划。**只描述「该怎么启动」，不执行。**
#[derive(Debug, Clone, Serialize)]
pub struct LaunchPlan {
    pub instance_id: String,
    pub version_id: String,
    pub main_class: String,

    /// 按加载顺序拼好的 classpath（建 VM 时原样用）
    pub classpath: String,
    /// 同一份 classpath 的逐条形式，前端用来展示「谁排在谁前面」
    pub entries: Vec<String>,

    pub jvm_args: Vec<String>,
    pub game_args: Vec<String>,

    pub game_dir: String,
    pub natives_dir: String,
    pub assets_dir: String,
    pub library_path: String,

    /// 生成过程中的取舍（剔了哪些库、改了哪些参数），排查时最先看这里
    pub notes: Vec<String>,
    /// 是否还能建 VM。VM 已建过时为 `false`，此时 classpath 已锁死
    pub vm_ready: bool,
}

/// 算出启动计划。**不建 VM、不改状态**，纯读。
pub fn plan(app: &AppHandle, instance_id: &str) -> Result<LaunchPlan, String> {
    let instance = crate::instances::get_instance(app, instance_id)?;
    let s = crate::settings::load(app);
    let lw = crate::java::lwjgl::ensure(app)?;
    let mut notes: Vec<String> = vec![];

    let mc_root = crate::paths::mc_root(app, &instance);
    let game_dir = crate::paths::resolve_game_dir(app, &instance);
    let version_id = instance
        .installed_version_id
        .clone()
        .unwrap_or_else(|| instance.minecraft_version.clone());

    let vjson_path = mc_root
        .join("versions")
        .join(&version_id)
        .join(format!("{version_id}.json"));
    let text = std::fs::read_to_string(&vjson_path)
        .map_err(|e| format!("读不到版本 JSON {}：{e}", vjson_path.display()))?;
    let vjson: Value =
        serde_json::from_str(&text).map_err(|e| format!("版本 JSON 解析失败：{e}"))?;

    let main_class = vjson
        .get("mainClass")
        .and_then(|v| v.as_str())
        .ok_or("版本 JSON 里没有 mainClass")?
        .to_string();

    // ---- classpath -------------------------------------------------------
    // 我们的两个 jar 无条件排最前：shim 压过 fat jar 里的 v2 CallbackBridge，
    // fat jar 压过官方 lwjgl。
    let mut entries: Vec<PathBuf> = vec![lw.shim_jar.clone(), lw.classes_jar.clone()];

    // 统计取舍：`natives-*` 分类器也各算一条，所以「条数」远多于「模块数」，
    // 两个数都给出来，排查时不容易被 35 这个数字吓到。
    let mut dropped_total = 0usize;
    let mut dropped_modules: Vec<String> = vec![];
    let mut kept_lwjgl: Vec<String> = vec![];
    if let Some(libs) = vjson.get("libraries").and_then(|v| v.as_array()) {
        for l in libs {
            let Some(name) = l.get("name").and_then(|n| n.as_str()) else {
                continue;
            };
            let ga = name.split(':').take(2).collect::<Vec<_>>().join(":");
            if is_replaced_lwjgl(name) {
                dropped_total += 1;
                if !dropped_modules.contains(&ga) {
                    dropped_modules.push(ga);
                }
            } else if ga.starts_with("org.lwjgl") && !kept_lwjgl.contains(&ga) {
                kept_lwjgl.push(ga);
            }
        }
    }
    if dropped_total > 0 {
        notes.push(format!(
            "移出 {dropped_total} 条官方 LWJGL 条目（含 natives 分类器），覆盖 {} 个模块：{}",
            dropped_modules.len(),
            dropped_modules.join(", ")
        ));
    }
    if !kept_lwjgl.is_empty() {
        notes.push(format!(
            "保留官方 LWJGL 模块（fork jar 里没有对应类，native 由我们自己的 .so 顶上）：{}",
            kept_lwjgl.join(", ")
        ));
    }

    entries.extend(collect_classpath_entries(
        &mc_root,
        &version_id,
        &instance.minecraft_version,
        &vjson,
        &is_replaced_lwjgl,
    ));
    if entries.len() <= 2 {
        return Err(format!(
            "classpath 里只有移植层，没有游戏本体 —— 版本 {version_id} 可能没装完"
        ));
    }
    let classpath = join_classpath(&entries);

    // ---- 变量表 ----------------------------------------------------------
    let natives_dir = mc_root.join("versions").join(&version_id).join("natives");
    let assets_dir = mc_root.join("assets");
    let asset_index = vjson
        .get("assetIndex")
        .and_then(|a| a.get("id"))
        .and_then(|i| i.as_str())
        .unwrap_or("legacy")
        .to_string();

    // 账户：和桌面启动一样，取第一个账户，没有就临时建一个离线账户。
    // Android 上目前只支持离线（微软登录要开浏览器，属于后续工作）。
    let account = match crate::auth::load_accounts(app).into_iter().next() {
        Some(a) => a,
        None => {
            let raw = std::env::var("USERNAME").unwrap_or_default();
            let name: String = raw
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .take(16)
                .collect();
            let name = if name.is_empty() { "Steve".into() } else { name };
            notes.push(format!("没有账户，临时使用离线名 {name}"));
            crate::auth::offline_login(app.clone(), name)?
        }
    };

    let (rw, rh) = match (instance.window_width, instance.window_height) {
        (Some(w), Some(h)) if w >= 320 && h >= 240 => (w.to_string(), h.to_string()),
        _ => ("854".into(), "480".into()),
    };

    let mut vars: HashMap<String, String> = HashMap::new();
    for (k, v) in [
        ("auth_player_name", account.name.clone()),
        ("auth_uuid", account.uuid.clone()),
        ("auth_access_token", account.access_token.clone()),
        ("user_type", account.account_type.clone()),
        ("version_type", "release".to_string()),
        ("game_directory", game_dir.to_string_lossy().to_string()),
        ("assets_root", assets_dir.to_string_lossy().to_string()),
        ("assets_index_name", asset_index),
        ("natives_directory", natives_dir.to_string_lossy().to_string()),
        ("classpath", classpath.clone()),
        ("version", version_id.clone()),
        ("version_name", version_id.clone()),
        ("resolution_width", rw),
        ("resolution_height", rh),
        ("clientid", "ccl-client".into()),
        ("auth_xuid", String::new()),
        ("user_properties", "{}".into()),
        ("quickPlayPath", String::new()),
        ("quickPlaySingleplayer", String::new()),
        ("quickPlayMultiplayer", String::new()),
        ("launcher_name", "CwjCraftLauncher".into()),
        ("launcher_version", env!("CARGO_PKG_VERSION").into()),
    ] {
        vars.insert(k.to_string(), v);
    }

    // ---- JVM 参数 --------------------------------------------------------
    // 我们的 natives 目录必须排最前：里面有 libltw / libpojavexec / libopenal，
    // 版本 JSON 里的 `${natives_directory}` 是桌面 dll 的落点，帮不上忙。
    let library_path = format!("{}:{}", lw.dir.display(), natives_dir.display());
    let memory = instance.max_memory.unwrap_or(s.max_memory);
    let mut jvm_args: Vec<String> = vec![format!("-Xmx{memory}M")];
    if let Some(min) = instance.min_memory {
        if min > 0 {
            jvm_args.push(format!("-Xms{min}M"));
        }
    }
    jvm_args.push(format!("-Djava.library.path={library_path}"));
    jvm_args.push("-Dorg.lwjgl.opengl.libname=libltw.so".into());
    // 不必设 `-Dos.name`：HotSpot 在 Android 内核上本来就报 Linux，
    // 版本 JSON 的 rules 判定也已经按 linux 走（见 `launcher::rule_os_name`）。

    let mut drop_next = false;
    let mut from_json = 0usize;
    if let Some(arr) = vjson.pointer("/arguments/jvm").and_then(|v| v.as_array()) {
        for a in arr {
            for piece in arg_strings(a) {
                if drop_next {
                    drop_next = false;
                    continue;
                }
                if piece == "-cp" || piece == "-classpath" {
                    drop_next = true;
                    continue;
                }
                if is_desktop_only_jvm_arg(&piece) {
                    continue;
                }
                jvm_args.push(substitute(&piece, &vars));
                from_json += 1;
            }
        }
    }
    if from_json > 0 {
        notes.push(format!("沿用版本 JSON 的 {from_json} 个 JVM 参数"));
    }

    // 用户自定义 JVM 参数（实例级优先）
    let extra = instance
        .jvm_args
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| s.jvm_args.trim());
    if !extra.is_empty() {
        for part in extra.split_whitespace() {
            jvm_args.push(part.to_string());
        }
        notes.push(format!("追加自定义 JVM 参数：{extra}"));
    }

    // ---- 游戏参数 --------------------------------------------------------
    let mut game_args: Vec<String> = vec![];
    if let Some(arr) = vjson.pointer("/arguments/game").and_then(|v| v.as_array()) {
        for a in arr {
            for piece in arg_strings(a) {
                game_args.push(substitute(&piece, &vars));
            }
        }
    } else if let Some(legacy) = vjson.get("minecraftArguments").and_then(|v| v.as_str()) {
        for part in legacy.split(' ') {
            game_args.push(substitute(part, &vars));
        }
    }
    if game_args.is_empty() {
        notes.push("版本 JSON 没有 game 参数，启动会缺 --username/--gameDir".into());
    }

    if let Some(extra) = instance
        .game_args
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        for part in extra.split_whitespace() {
            game_args.push(substitute(part, &vars));
        }
    }
    if instance.fullscreen {
        game_args.push("--fullscreen".into());
    }

    // ---- 建 VM 的可行性 --------------------------------------------------
    // VM 一旦建好，`-Djava.class.path` 就定死了；这时再用这份计划必然
    // NoClassDefFoundError。可以在 UI 上提示「重启 App 后再启动游戏」。
    let vm_ready = crate::java::embedded::vm_alive_for_launch(&classpath);
    if !vm_ready {
        notes.push(
            "内嵌 JVM 已用别的 classpath 建好了，本次启动会缺游戏类。重启 App 再试。".into(),
        );
    }

    Ok(LaunchPlan {
        instance_id: instance_id.to_string(),
        version_id,
        main_class,
        entries: entries
            .iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect(),
        classpath,
        jvm_args,
        game_args,
        game_dir: game_dir.to_string_lossy().to_string(),
        natives_dir: natives_dir.to_string_lossy().to_string(),
        assets_dir: assets_dir.to_string_lossy().to_string(),
        library_path,
        notes,
        vm_ready,
    })
}
