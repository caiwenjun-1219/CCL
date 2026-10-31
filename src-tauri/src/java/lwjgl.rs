//! LWJGL 移植层（M2 里程碑）
//!
//! ## 这里解决什么问题
//!
//! Minecraft 的渲染全靠 LWJGL。官方 LWJGL 只有 `natives-linux/macos/windows`，
//! `natives-linux` 依赖 `libGL.so` / `libX11`，Android 的 bionic 根本加载不了。
//! Pojav 的做法是 fork 一份 LWJGL，把 GLFW 这一层改成纯 Java 实现 ——
//! 它不再调桌面 GLFW，而是通过 `libpojavexec.so` 直接走 EGL + `ANativeWindow`。
//!
//! 所以 Android 上要用的是两样东西：
//!
//! 1. **`lwjgl-classes.jar`** —— Pojav 的 fat jar，里面是 fork 版 `org.lwjgl.*`
//!    （含 `org.lwjgl.glfw.GLFW` 的纯 Java 实现、OpenAL、STB、NanoVG、Caciocavallo）
//! 2. **一组 `.so`** —— `libpojavexec`（EGL 桥 + 输入桥）、`liblwjgl*`
//!    （分别是 lwjgl 核心 / opengl / stb / tinyfd / nanovg 的 JNI 部分）、
//!    `libltw`（GLES→GL 翻译）、`libopenal`、`libfreetype`
//!
//! ## 为什么 classpath 顺序不能错
//!
//! 游戏本体还会从版本 JSON 里带进官方的 `lwjgl-3.3.3.jar` 等。Java 的类加载
//! 是「第一个命中的赢」，所以 `lwjgl-classes.jar` **必须排在官方 jar 前面**，
//! 否则拿到的还是调桌面 GLFW 的那份，`GLFW.glfwInit()` 会直接
//! `UnsatisfiedLinkError: dlopen failed: library "libglfw.so" not found`。
//!
//! ## 部署方式
//!
//! natives 用 `tar.xz` 放进 APK assets（首装解包一次，实测 xz 压到约 1.5 MB/ABI），
//! 解包到 `<data>/lwjgl/`，再通过 `-Djava.library.path` 告诉 JVM 去哪里找。
//! 不用 jniLibs 是因为那边只能按构建 ABI 打包，而我们希望同一个 APK
//! 里 arm64 与 x86_64（模拟器调试）都能跑。

use serde::Serialize;
use std::path::PathBuf;
use tauri::AppHandle;

/// 一次 M2 探针的完整结果
#[derive(Serialize, Clone, Default)]
pub struct LwjglReport {
    /// 解包目录
    pub dir: String,
    /// 设备 ABI（arm64-v8a / x86_64）
    pub abi: String,
    /// 解出来的 .so 个数
    pub native_count: usize,
    /// 解包耗时（毫秒）；命中缓存时为 0
    pub prepare_ms: u64,
    /// 是否复用了上次的解包结果
    pub prepared_cached: bool,

    /// VM 是否可用
    pub vm_ok: bool,
    pub vm_error: String,
    /// 建 VM 用的 classpath
    pub classpath: String,
    /// 建 VM 用的 java.library.path
    pub library_path: String,

    /// 探针原始输出（`key=value` 逐行，`=` 后带 `!` 表示该步抛异常）
    pub probe: String,
    /// 通过 / 失败的步骤数
    pub steps_ok: usize,
    pub steps_fail: usize,
    /// 给人看的结论
    pub verdict: String,
}

/// 当前设备对应的 Android ABI 目录名（与 APK assets 里的一致）
pub fn abi() -> &'static str {
    match std::env::consts::ARCH {
        "aarch64" => "arm64-v8a",
        "x86_64" => "x86_64",
        "arm" => "armeabi-v7a",
        "x86" => "x86",
        other => other,
    }
}

/// 解包后的路径集合
#[derive(Debug, Clone)]
pub struct LwjglFiles {
    /// 解包目录（同时是 `java.library.path`）
    pub dir: PathBuf,
    /// fork 版 lwjgl classes（含纯 Java GLFW 实现）
    pub classes_jar: PathBuf,
    /// M2 探针
    pub probe_jar: PathBuf,
    /// CallbackBridge 兼容层
    pub shim_jar: PathBuf,
    /// 是否复用了已有解包结果
    pub cached: bool,
    /// 解包耗时
    pub elapsed_ms: u64,
}

/// natives 归档在 assets 里的名字
fn bin_asset(abi: &str) -> String {
    format!("lwjgl/lwjgl-bin-{abi}.tar.xz")
}
const CLASSES_ASSET: &str = "lwjgl/lwjgl-classes.jar";
const PROBE_ASSET: &str = "lwjgl/probe.jar";
/// `CallbackBridge` 兼容实现，必须排在 classes jar 前面（见 `crate::android::shim`）
const SHIM_ASSET: &str = "lwjgl/shim.jar";

/// 解包目录的内容版本。
///
/// 只按「文件在不在」判断复用会在换资源后踩坑：新打的 jar 没被释放，
/// 设备上跑的还是上一版（我们就是这样被上一版 shim 坑过一次）。
/// 改动 assets 里的任何文件时**把这个数字加一**，标记对不上就整体重解。
const BUNDLE_REV: u32 = 2;

#[cfg(target_os = "android")]
pub fn ensure(app: &AppHandle) -> Result<LwjglFiles, String> {
    use crate::java::android::extract_tar_xz_shared;

    let started = std::time::Instant::now();
    let abi = abi();
    let dir = crate::settings::lwjgl_dir(app);
    let classes_jar = dir.join("lwjgl-classes.jar");
    let probe_jar = dir.join("probe.jar");
    let shim_jar = dir.join("shim.jar");
    let marker = dir.join(".abi");
    let stamp = format!("{abi}:{BUNDLE_REV}");

    // 命中条件：ABI 标记一致 + 关键文件都在。
    // 不做 sha 校验：这套东西跟着 APK 走，APK 一升级 ABI 标记/文件数就变。
    let marker_ok = std::fs::read_to_string(&marker)
        .map(|s| s.trim() == stamp)
        .unwrap_or(false);
    if marker_ok
        && classes_jar.exists()
        && probe_jar.exists()
        && shim_jar.exists()
        && dir.join("libpojavexec.so").exists()
    {
        println!("[ccl-lwjgl] 复用解包结果 {}", dir.display());
        return Ok(LwjglFiles {
            dir,
            classes_jar,
            probe_jar,
            shim_jar,
            cached: true,
            elapsed_ms: 0,
        });
    }

    // ---- 1. natives
    let tmp = crate::settings::data_dir(app).join(".tmp");
    std::fs::create_dir_all(&tmp).map_err(|e| format!("创建临时目录失败: {e}"))?;

    let asset = bin_asset(abi);
    if !ccl_android::has_asset(app, &asset) {
        return Err(format!(
            "APK 里没有 {asset}。当前构建只打了 arm64-v8a / x86_64 两套 natives，\
             请确认设备 ABI（当前 {abi}）在支持范围内。"
        ));
    }

    let local = tmp.join(format!("lwjgl-bin-{abi}.tar.xz"));
    let _ = std::fs::remove_file(&local);
    let bytes = ccl_android::copy_asset(app, &asset, &local.to_string_lossy())
        .map_err(|e| format!("释放 {asset} 失败：{e}"))?;
    if bytes == 0 {
        return Err(format!("{asset} 拷贝到磁盘后为 0 字节"));
    }

    // 换 ABI 或首次安装：整目录重来，避免旧 so 残留
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建 {} 失败: {e}", dir.display()))?;
    let (files, _links) = extract_tar_xz_shared(&local, &dir)?;
    let _ = std::fs::remove_file(&local);
    println!("[ccl-lwjgl] 解出 {files} 个文件到 {}", dir.display());

    // ---- 2. java 侧（jar）
    for (asset, dest) in [
        (CLASSES_ASSET, &classes_jar),
        (PROBE_ASSET, &probe_jar),
        (SHIM_ASSET, &shim_jar),
    ] {
        let _ = std::fs::remove_file(dest);
        let n = ccl_android::copy_asset(app, asset, &dest.to_string_lossy())
            .map_err(|e| format!("释放 {asset} 失败：{e}"))?;
        if n == 0 {
            return Err(format!("{asset} 拷贝到磁盘后为 0 字节"));
        }
    }

    std::fs::write(&marker, &stamp).map_err(|e| format!("写解包标记失败: {e}"))?;

    Ok(LwjglFiles {
        dir,
        classes_jar,
        probe_jar,
        shim_jar,
        cached: false,
        elapsed_ms: started.elapsed().as_millis() as u64,
    })
}

/// M2：跑一遍 LWJGL 连通性探针。
///
/// 会顺带确保内嵌 JVM 已建立（进程内唯一），所以在 M1 卡片之后再点也没问题。
#[cfg(target_os = "android")]
pub async fn run(app: AppHandle) -> Result<LwjglReport, String> {
    use crate::java::embedded;

    // 解包 + 建 VM + 跑探针全部放到阻塞线程：
    // 建 VM 的那个线程会变成 JVM main 线程，不能是异步 runtime 线程。
    tauri::async_runtime::spawn_blocking(move || -> Result<LwjglReport, String> {
        let mut r = LwjglReport::default();
        r.abi = abi().to_string();

        let files = ensure(&app)?;
        r.dir = files.dir.to_string_lossy().to_string();
        r.prepared_cached = files.cached;
        r.prepare_ms = files.elapsed_ms;
        r.native_count = std::fs::read_dir(&files.dir)
            .map(|it| {
                it.filter_map(|e| e.ok())
                    .filter(|e| e.file_name().to_string_lossy().ends_with(".so"))
                    .count()
            })
            .unwrap_or(0);

        // ---- 选 JRE（取最高主版本）
        let javas = crate::java::scan_javas(app.clone());
        let info = javas
            .into_iter()
            .max_by_key(|j| j.major)
            .ok_or_else(|| "没有已安装的 Java，先去 Java 页装一个".to_string())?;
        let bin = PathBuf::from(&info.path);
        let jre_home = bin
            .parent()
            .and_then(|p| p.parent())
            .map(|p| p.to_path_buf())
            .ok_or_else(|| format!("无法从 {} 推出 JRE 根目录", info.path))?;

        // ---- 建 VM（或复用）
        let (vm, report) = embedded::ensure_vm_for(&app, jre_home);
        println!(
            "[ccl-lwjgl] VM ok={} 预加载 JRE 库 {} 成功 / {} 失败",
            report.vm_ok, report.preload_ok, report.preload_fail
        );
        r.vm_ok = report.vm_ok;
        r.classpath = report.classpath.clone();
        r.library_path = report.library_path.clone();
        if !r.vm_ok {
            r.vm_error = report.verdict.clone();
            r.verdict = format!("内嵌 JVM 没建起来，谈不上加载 LWJGL：{}", report.verdict);
            return Ok(r);
        }
        let vm = vm.ok_or_else(|| "VM 报告可用但句柄丢失".to_string())?;

        // ---- 跑探针
        // attach 当前线程：spawn_blocking 的线程池线程不一定是建 VM 的那个
        let mut env = match vm.attach_current_thread() {
            Ok(e) => e,
            Err(e) => {
                r.verdict = format!("attach 到 JVM 失败：{e}");
                return Ok(r);
            }
        };

        let text = match call_probe(&mut env) {
            Ok(t) => t,
            Err(e) => {
                r.verdict = format!("调用探针失败：{e}");
                return Ok(r);
            }
        };
        drop(env);

        r.steps_ok = text
            .lines()
            .filter(|l| l.contains('=') && !l.contains("=!"))
            .count();
        r.steps_fail = text.lines().filter(|l| l.contains("=!")).count();
        r.verdict = summarize(&text);
        r.probe = text;
        Ok(r)
    })
    .await
    .map_err(|e| format!("探针线程异常: {e}"))?
}

/// 调 `ccl.lwjgl.Probe.run()` 取回报告文本
#[cfg(target_os = "android")]
fn call_probe(env: &mut jni::JNIEnv) -> Result<String, String> {
    let v = env
        .call_static_method(
            "ccl/lwjgl/Probe",
            "run",
            "()Ljava/lang/String;",
            &[],
        )
        .map_err(|e| match e {
            jni::errors::Error::JavaException => {
                let ex = env.exception_occurred().ok();
                if let Some(ex) = ex {
                    let _ = env.exception_clear();
                    let msg = env
                        .call_method(&ex, "toString", "()Ljava/lang/String;", &[])
                        .ok()
                        .and_then(|v| v.l().ok())
                        .map(|o| {
                            let s = jni::objects::JString::from(o);
                            env.get_string(&s)
                                .map(|x| x.to_string_lossy().to_string())
                                .unwrap_or_default()
                        })
                        .unwrap_or_default();
                    format!("Java 侧抛异常：{msg}")
                } else {
                    "Java 侧抛异常（无详情）".into()
                }
            }
            other => format!("{other}"),
        })?;

    let obj = v.l().map_err(|e| format!("返回值不是对象：{e}"))?;
    if obj.is_null() {
        return Err("探针返回 null".into());
    }
    let s = jni::objects::JString::from(obj);
    let js = env.get_string(&s).map_err(|e| format!("读字符串失败：{e}"))?;
    Ok(js.to_string_lossy().to_string())
}

/// 从探针文本里提炼一句结论
fn summarize(text: &str) -> String {
    let get = |k: &str| -> Option<&str> {
        text.lines()
            .find(|l| l.starts_with(&format!("{k}=")))
            .map(|l| &l[k.len() + 1..])
    };
    let ok = |k: &str| get(k).map(|v| !v.starts_with('!') && v != "false").unwrap_or(false);

    let native_ok = ok("native.pojavexec") && ok("native.lwjgl");
    let core_ok = get("lwjgl.version").map(|v| !v.starts_with('!')).unwrap_or(false);
    let glfw_cls = get("glfw.class").map(|v| !v.starts_with('!')).unwrap_or(false);

    if native_ok && core_ok && glfw_cls {
        let init = get("glfw.init").unwrap_or("未测");
        let glfw_ver = get("glfw.version").unwrap_or("?");
        if init == "true" {
            format!("LWJGL 移植层完全可用：natives 加载、lwjgl 核心、GLFW {glfw_ver} 初始化全部通过。")
        } else if init.starts_with("skipped") {
            format!(
                "LWJGL 移植层可用：natives 加载、lwjgl 核心、fork 版 GLFW {glfw_ver} 均正常。\
                 下一步（M3）把 Android 的 Surface 接进 GLFW，就能真正建窗口出画面。"
            )
        } else {
            format!("LWJGL 移植层基本可用，但 glfwInit 未成功：{init}")
        }
    } else if native_ok && core_ok {
        format!(
            "natives 与 lwjgl 核心正常，但 fork 版 GLFW 没能起来：{}",
            get("glfw.class").unwrap_or("未知")
        )
    } else {
        format!(
            "natives 加载失败：pojavexec={} lwjgl={}",
            get("native.pojavexec").unwrap_or("?"),
            get("native.lwjgl").unwrap_or("?")
        )
    }
}

#[cfg(not(target_os = "android"))]
pub async fn run(app: AppHandle) -> Result<LwjglReport, String> {
    let _ = app;
    Err("LWJGL 移植层只对 Android 有意义（桌面端本来就是官方 LWJGL）".into())
}

#[cfg(not(target_os = "android"))]
pub fn ensure(app: &AppHandle) -> Result<LwjglFiles, String> {
    let _ = app;
    Err("LWJGL 移植层只对 Android 有意义".into())
}
