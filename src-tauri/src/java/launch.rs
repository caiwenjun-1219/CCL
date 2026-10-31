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

    /// 版本 JSON 要求的 Java 主版本（选 JRE 用）
    pub java_major: u32,
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

    // Java 主版本优先取 JSON 的声明（最准），缺失时按 MC 版本号推断
    let java_major = vjson
        .pointer("/javaVersion/majorVersion")
        .and_then(|v| v.as_u64())
        .map(|v| v as u32)
        .unwrap_or_else(|| crate::java::required_java_major(&instance.minecraft_version));

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
        java_major,
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

// ---------------------------------------------------------------------------
// 真正启动（M4-b-2）
// ---------------------------------------------------------------------------

/// 一次启动的即时结果。**不含游戏运行状态** —— 游戏跑在独立线程里，
/// 后续输出走 `<gameDir>/logs/latest.log`。
#[derive(Debug, Clone, Serialize)]
pub struct StartReport {
    pub version_id: String,
    pub main_class: String,
    pub java_home: String,
    pub vm_ok: bool,
    pub vm_error: String,
    pub surface_ok: bool,
    pub surface_info: String,
    pub classpath_entries: usize,
    pub verdict: String,
}

/// 真正启动游戏。**同步函数，调用方要放到阻塞线程里**（会等 VM 建好）。
///
/// 四步顺序不能换：
///
/// 1. [`plan`] 算 classpath —— VM 只能按它建一次
/// 2. `surface::attach` 把 Activity 的 Surface 交给 `libpojavexec`
///    （`pojavInit` 要在 MC 调 `glfwInit` 之前把 native window 准备好，
///    而且 ART 必须先 dlopen 一次 so，`JNI_OnLoad` 才会记下 dalvik VM）
/// 3. 建 VM
/// 4. 起一条**普通线程**跑 `mainClass.main`
///
/// 第 4 步不能挂在 tokio 的 runtime 线程上：MC 的 main 会一直跑到游戏退出。
pub fn start(app: &AppHandle, instance_id: &str) -> Result<StartReport, String> {
    let p = plan(app, instance_id)?;
    if !p.vm_ready {
        return Err(
            "内嵌 JVM 已经用别的 classpath 建好了（多半是跑过 M2/M3 探针），重启 App 后再启动游戏"
                .into(),
        );
    }

    // MC 的 log4j 配置里文件名是相对路径 `logs/latest.log`，而 App 进程的
    // cwd 是 `/`，于是它去建 `/logs` —— 没权限，整个日志系统静默失效
    // （现象是 games 目录下永远没有 latest.log）。切到游戏目录解决。
    // 进程级 chdir，会影响同进程其它线程，但 App 里没人依赖 cwd。
    std::env::set_current_dir(&p.game_dir)
        .map_err(|e| format!("切换工作目录到 {} 失败：{e}", p.game_dir))?;
    println!("[launch] cwd={}", p.game_dir);

    let java = crate::java::auto_select_java_with(app, p.java_major, &p.version_id)?;
    let jre_home = std::path::PathBuf::from(&java.path)
        .parent()
        .and_then(|bin| bin.parent())
        .map(|p| p.to_path_buf())
        .ok_or_else(|| format!("从 {} 推不出 JRE 根目录", java.path))?;
    println!(
        "[launch] JRE={}（要求 Java {}），classpath {} 条",
        java.path,
        p.java_major,
        p.entries.len()
    );

    // 画面要盖在 WebView 之上。M4-a 的演示结束后 overlay 会被收回下层，
    // 真正启动游戏时得重新置顶 —— 否则游戏在跑，用户看到的还是启动器界面。
    //
    // `setZOrderOnTop` 是 post 到 UI 线程的，改完必须等一拍：
    // 万一 ROM 重建了 Surface（`surfaceDestroyed` → `surfaceCreated`），
    // 我们提前 attach 拿到的就是一块马上失效的 `ANativeWindow`。
    crate::java::surface::set_overlay(true);
    std::thread::sleep(std::time::Duration::from_millis(400));

    // 必须在建 VM 之前：ART 侧先 dlopen libpojavexec，JNI_OnLoad 才会占住
    // dalvikJavaVMPtr，内嵌 JVM 之后再加载时才记得下 runtimeJavaVMPtr。
    let lw = crate::java::lwjgl::ensure(app)?;
    let surface = crate::java::surface::attach(&lw.dir, 5000);
    println!(
        "[launch] surface ok={} {}x{} err={}",
        surface.ok, surface.width, surface.height, surface.error
    );
    if !surface.ok {
        return Err(format!("Surface 桥接失败：{}", surface.error));
    }

    // plan 算出来的 JVM 参数（-Xmx / -Djna.tmpdir / libltw 等）必须在这里注进去：
    // 建 VM 是唯一能让这些 -D 生效的时机。
    let (vm, report) = crate::java::embedded::ensure_vm_for_launch(
        app,
        jre_home.clone(),
        p.classpath.clone(),
        p.jvm_args.clone(),
    );
    let Some(vm) = vm else {
        return Err(format!("建 VM 失败：{}", report.verdict));
    };
    println!("[launch] VM ok，java.home={}", report.java_home);

    let main_class = p.main_class.clone();
    let args = p.game_args.clone();
    let entries = p.entries.len();
    // JVM 的 stdout/err 在 Android 上没接 logcat，落到游戏目录里
    let stdout_log = std::path::Path::new(&p.game_dir)
        .join("logs")
        .join("ccl-jvm-out.log")
        .to_string_lossy()
        .to_string();
    let log_for_thread = stdout_log.clone();
    std::thread::spawn(move || {
        println!("[launch] 主线程启动：{main_class}（{} 个参数）", args.len());
        match invoke_main(vm, &main_class, &args, &log_for_thread) {
            Ok(()) => println!("[launch] main 返回了（游戏正常退出）"),
            Err(e) => println!("[launch] main 异常：{e}"),
        }
    });

    Ok(StartReport {
        version_id: p.version_id.clone(),
        main_class: p.main_class.clone(),
        java_home: jre_home.to_string_lossy().to_string(),
        vm_ok: report.vm_ok,
        vm_error: if report.vm_ok {
            String::new()
        } else {
            report.verdict.clone()
        },
        surface_ok: surface.ok,
        surface_info: format!("{}x{}", surface.width, surface.height),
        classpath_entries: entries,
        verdict: format!(
            "已发起启动：{}（Java {}，{} 条 classpath）。游戏日志见 {}logs/latest.log",
            p.main_class, p.java_major, entries, p.game_dir.trim_end_matches('/').to_string() + "/"
        ),
    })
}

/// 在已经建好的 VM 里调 `mainClass.main(String[])`。
///
/// `stdout_log` 是把 JVM 的 `System.out/err` 引过去的文件 —— Android 上
/// 内嵌 JVM 的 fd 1/2 是丢的，不接出来就完全看不到 MC 的进度和堆栈。
fn invoke_main(
    vm: &jni::JavaVM,
    main_class: &str,
    args: &[String],
    stdout_log: &str,
) -> Result<(), String> {
    use jni::objects::{JObject, JValue};

    let mut env = vm
        .attach_current_thread()
        .map_err(|e| format!("attach 到 JVM 失败：{e}"))?;

    // **必须走 ClassLoader，不能用 `FindClass`。**
    //
    // JNI 规范说「没有关联 Java 帧时用 system class loader」，但 attach 出来的
    // 原生线程里 HotSpot 实际把它落在 bootstrap loader 上，`-Djava.class.path`
    // 上的类一个都找不到（实测报的是含糊的 `Java exception was thrown`）。
    let loader = env
        .call_static_method(
            "java/lang/ClassLoader",
            "getSystemClassLoader",
            "()Ljava/lang/ClassLoader;",
            &[],
        )
        .and_then(|v| v.l())
        .map_err(|e| format!("取系统 ClassLoader 失败：{e}"))?;

    // 先接日志，后面无论出什么事都能在文件里看到
    match load_class(&mut env, &loader, "ccl.Boot") {
        Ok(boot) => {
            let path = env
                .new_string(stdout_log)
                .map_err(|e| format!("日志路径转 jstring 失败：{e}"))?;
            match env.call_static_method(
                &boot,
                "redirect",
                "(Ljava/lang/String;)Ljava/lang/String;",
                &[JValue::Object(path.as_ref())],
            ) {
                Ok(v) => {
                    let msg = v
                        .l()
                        .ok()
                        .and_then(|o| read_string(&mut env, o))
                        .unwrap_or_default();
                    if msg.is_empty() {
                        println!("[launch] JVM 输出已重定向到 {stdout_log}");
                    } else {
                        println!("[launch] 日志重定向失败（不影响启动）：{msg}");
                    }
                }
                Err(e) => println!("[launch] Boot.redirect 调用失败：{e}"),
            }
        }
        Err(e) => println!("[launch] 加载 ccl.Boot 失败（不影响启动）：{e}"),
    }

    let class = load_class(&mut env, &loader, main_class)?;

    let arr = env
        .new_object_array(args.len() as i32, "java/lang/String", JObject::null())
        .map_err(|e| format!("建参数数组失败：{e}"))?;
    for (i, a) in args.iter().enumerate() {
        let s = env
            .new_string(a)
            .map_err(|e| format!("参数 {i} 转 jstring 失败：{e}"))?;
        env.set_object_array_element(&arr, i as i32, &s)
            .map_err(|e| format!("写参数 {i} 失败：{e}"))?;
    }

    println!("[launch] 调用 {main_class}.main(...)");
    let r = env.call_static_method(
        &class,
        "main",
        "([Ljava/lang/String;)V",
        &[JValue::Object(arr.as_ref())],
    );
    match r {
        Ok(_) => Ok(()),
        Err(e) => {
            let thrown = describe_exception(&mut env);
            Err(format!("{e}{thrown}"))
        }
    }
}

/// `ClassLoader.loadClass(name)`。
fn load_class<'l>(
    env: &mut jni::JNIEnv<'l>,
    loader: &jni::objects::JObject<'l>,
    name: &str,
) -> Result<jni::objects::JClass<'l>, String> {
    use jni::objects::{JClass, JValue};

    let jname = env
        .new_string(name)
        .map_err(|e| format!("类名转 jstring 失败：{e}"))?;
    let loaded = env
        .call_method(
            loader,
            "loadClass",
            "(Ljava/lang/String;)Ljava/lang/Class;",
            &[JValue::Object(jname.as_ref())],
        )
        .and_then(|v| v.l());
    match loaded {
        Ok(c) => Ok(JClass::from(c)),
        Err(e) => {
            let thrown = describe_exception(env);
            Err(format!("加载 {name} 失败：{e}{thrown}"))
        }
    }
}

/// `JString` → Rust `String`
fn read_string(env: &mut jni::JNIEnv, obj: jni::objects::JObject) -> Option<String> {
    let s = jni::objects::JString::from(obj);
    env.get_string(&s)
        .ok()
        .map(|v| v.to_string_lossy().to_string())
}

/// 把挂起的 Java 异常捞出来，否则只剩一句「Java exception occurred」没法排查。
///
/// **不能用 `exception_describe`**：它是把堆栈写到 `System.err`，而内嵌 JVM 的
/// fd 2 在 Android 上没接到 logcat，等于扔进黑洞。这里改成走 JNI 反射，
/// 把 `toString()` 和前几帧 `getStackTrace()` 拼成字符串由 Rust 打出来。
fn describe_exception(env: &mut jni::JNIEnv) -> String {
    use jni::objects::JObjectArray;

    if !env.exception_check().unwrap_or(false) {
        return String::new();
    }
    let Ok(ex) = env.exception_occurred() else {
        let _ = env.exception_clear();
        return String::new();
    };
    let _ = env.exception_clear();

    let to_rust = read_string;

    let mut out = String::new();
    if let Ok(v) = env.call_method(&ex, "toString", "()Ljava/lang/String;", &[]) {
        if let Ok(s) = v.l() {
            if let Some(t) = to_rust(env, s) {
                out.push_str(&format!("
  ↳ {t}"));
            }
        }
    }
    if let Ok(v) = env.call_method(&ex, "getStackTrace", "()[Ljava/lang/StackTraceElement;", &[]) {
        if let Ok(a) = v.l() {
            let arr = JObjectArray::from(a);
            let n = env.get_array_length(&arr).unwrap_or(0).min(8);
            for i in 0..n {
                let Ok(e) = env.get_object_array_element(&arr, i) else {
                    continue;
                };
                let Ok(v) = env.call_method(&e, "toString", "()Ljava/lang/String;", &[]) else {
                    continue;
                };
                let Ok(o) = v.l() else { continue };
                if let Some(t) = to_rust(env, o) {
                    out.push_str(&format!("
    at {t}"));
                }
            }
        }
    }
    out
}
