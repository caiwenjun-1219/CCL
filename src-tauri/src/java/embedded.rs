//! 内嵌 JVM（M1 起）：在 App 进程内 dlopen libjvm.so + JNI_CreateJavaVM
//!
//! ## 为什么必须同进程
//!
//! Android 上 `Command::new(java)` 起的是独立进程。它没有 ART、没有 Binder，
//! 拿不到 SurfaceView 的 `ANativeWindow`，因此 MC 的图形层（EGL）根本无从建立。
//! Pojav / FCL 走的是另一条路：把 `lib/server/libjvm.so` 映射进 App 自己的进程，
//! 调 `JNI_CreateJavaVM` 建 VM，这样渲染、输入、Surface 全在同进程内。
//!
//! ## 一个进程只能建一次
//!
//! `JNI_CreateJavaVM` 第二次调用返回 `JNI_EEXIST`。而这次调用必须带上
//! **完整且正确**的 classpath / java.library.path —— 建完之后改不了（能改的只有
//! `-D` 属性里的少数几个，classpath 不在其中）。所以所有要用 JVM 的业务
//! （M1 自检、M2 探针、以后真正启动 MC）都必须走同一个 [`ensure_vm`]，
//! 由它一次性把配置定死。
//!
//! ## 线程
//!
//! 调用 `JNI_CreateJavaVM` 的线程会变成 JVM 的 main 线程，且该线程的 `JNIEnv`
//! 只在这次调用里有效。因此建 VM 必须放在 `spawn_blocking`（不能占异步 runtime
//! 线程），后续业务线程各自 `attach_current_thread`。
//!
//! VM 建起来之后**不销毁**（`DestroyJavaVM` 会阻塞到所有非 daemon 线程退出），
//! App 退出即回收。

use serde::Serialize;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::AppHandle;

/// 一次内嵌 JVM 实验的完整结果
#[derive(Serialize, Clone, Default)]
pub struct EmbeddedReport {
    /// 选中的 JRE 根目录
    pub java_dir: String,
    /// libjvm.so 绝对路径
    pub libjvm_path: String,
    pub libjvm_exists: bool,

    /// dlopen 是否成功
    pub dlopen_ok: bool,
    pub dlopen_error: String,
    /// 是否找到 JNI_CreateJavaVM 符号
    pub symbol_ok: bool,

    /// JNI_CreateJavaVM 的返回码（0 = JNI_OK）
    pub create_rc: i32,
    /// 返回码的可读名，如 JNI_OK / JNI_EEXIST
    pub create_rc_name: String,
    /// VM 是否可用
    pub vm_ok: bool,

    /// 从 VM 里读出来的系统属性
    pub java_version: String,
    pub java_vm_name: String,
    pub java_vm_version: String,
    pub os_arch: String,
    /// VM 里报告的 java.home（验证 -Djava.home 是否生效）
    pub java_home: String,

    /// 建 VM 时用的 classpath（排查类找不到时最有用）
    pub classpath: String,
    /// 建 VM 时用的 java.library.path
    pub library_path: String,

    /// 预加载成功的 JRE 自带 .so 个数
    pub preload_ok: usize,
    /// 预加载失败的（缺依赖的库，比如需要 X11 的 libawt）
    pub preload_fail: usize,

    /// 创建耗时（毫秒）—— 冷启动体验的关键指标
    pub elapsed_ms: u64,
    /// 是否为缓存的旧结果（VM 只能建一次）
    pub cached: bool,
    /// 给人看的结论
    pub verdict: String,
}

/// 建 VM 需要的一次性配置。
///
/// 字段刻意做成「调用方填字符串」而不是自己拼，因为 classpath 的顺序
/// 有语义（fork 版 glfw classes 必须排在官方 jar 前面才能生效）。
#[derive(Clone, Default)]
pub struct VmConfig {
    /// JRE 根目录（含 `lib/server/libjvm.so`）
    pub jre_home: PathBuf,
    /// 完整 classpath，已按 `:` 拼好
    pub classpath: String,
    /// `-Djava.library.path`：**必须包含 JRE 自己的 `lib` 与 `lib/server`**，
    /// 否则 JRE 内部模块（libnio → libnet）都加载不出来。
    pub library_path: String,
    /// `-Dorg.lwjgl.librarypath`：LWJGL 找我们自己那套 natives 的地方
    pub lwjgl_path: String,
    /// 额外的 `-D` / `-X` 选项
    pub extra_opts: Vec<String>,
}

#[cfg(target_os = "android")]
mod imp {
    use super::{EmbeddedReport, VmConfig};

    use std::ffi::{CStr, CString};
    use std::os::raw::{c_char, c_int, c_void};
    use std::path::{Path, PathBuf};
    use std::ptr::null_mut;
    use std::sync::OnceLock;

    /// 进程内唯一的一次实验结果。失败不写缓存，允许换 JRE 重试。
    static VM: OnceLock<jni::JavaVM> = OnceLock::new();
    static REPORT: OnceLock<EmbeddedReport> = OnceLock::new();

    extern "C" {
        fn dlopen(filename: *const c_char, flag: c_int) -> *mut c_void;
        fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
        fn dlerror() -> *mut c_char;
    }

    const RTLD_NOW: c_int = 0x0002;
    /// libjvm.so 导出的 JNI 符号需要被后续 so（lwjgl / pojavexec）看见，必须 GLOBAL
    const RTLD_GLOBAL: c_int = 0x0100;

    /// `jint JNI_CreateJavaVM(JavaVM **pvm, void **penv, void *args)`
    type CreateJavaVmFn =
        unsafe extern "system" fn(*mut *mut c_void, *mut *mut c_void, *mut c_void) -> c_int;

    fn rc_name(rc: i32) -> &'static str {
        match rc {
            0 => "JNI_OK",
            -1 => "JNI_ERR",
            -2 => "JNI_EDETACHED",
            -3 => "JNI_EVERSION",
            -4 => "JNI_ENOMEM",
            -5 => "JNI_EEXIST",
            -6 => "JNI_EINVAL",
            _ => "UNKNOWN",
        }
    }

    /// 失败时取 dlerror 原文（调用前要清空，否则拿到的是上一次的陈旧消息）
    unsafe fn take_dlerror() -> String {
        let e = dlerror();
        if e.is_null() {
            "（dlerror 为空）".into()
        } else {
            CStr::from_ptr(e).to_string_lossy().to_string()
        }
    }

    /// 从 JRE 根目录定位 libjvm.so
    pub fn libjvm_of(jre_home: &Path) -> PathBuf {
        jre_home.join("lib").join("server").join("libjvm.so")
    }

    /// 预加载 JRE 自带的 JNI 库。
    ///
    /// ## 为什么必须这么做
    ///
    /// Android 的 linker 解析 `DT_NEEDED` 时**只在自己 namespace 的搜索路径**里找，
    /// 而 JRE 的 `lib/` 不在其中。于是 `libnio.so`（NEEDED `libnet.so`）用绝对路径
    /// dlopen 时，linker 找不到同目录的 `libnet.so`，直接报
    /// `dlopen failed: library "libnet.so" not found: needed by ...libnio.so`。
    ///
    /// Pojav 的做法是 `liblinkerhook.so` 劫持 `dlopen` 改搜索路径。我们换个更省事的：
    /// 启动前把 `lib/*.so` 全部用绝对路径 dlopen 进 namespace（`RTLD_GLOBAL`），
    /// 之后 linker 按 soname 查找就会命中这些已加载的库。
    ///
    /// 依赖有先后（libnet 要早于 libnio），一轮扫不干净，所以多轮直到没有新进展。
    /// 加载不了的（需要 X11 / CUPS 的 libawt 之类）直接跳过，不影响 JVM 本身。
    ///
    /// **`libjsig.so` 必须排除**：它是 HotSpot 的「信号链」库，设计用途是启动前
    /// `LD_PRELOAD`，会 interpose `sigaction`/`sigprocmask`。App 进程里已经由 ART 的
    /// `libsigchain` 占了这个位置，再 dlopen 一个 libjsig 进去会两家抢同一批信号，
    /// 表现为 JVM 自己的 SIGSEGV/SIGILL 处理链崩在 `__rt_sigprocmask` 里。
    fn preload_jre_libs(lib: &Path) -> (usize, usize) {
        const SKIP: &[&str] = &["libjsig.so"];
        let mut pending: Vec<PathBuf> = match std::fs::read_dir(lib) {
            Ok(it) => it
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.extension().map(|x| x == "so").unwrap_or(false))
                .filter(|p| {
                    let name = p.file_name().map(|n| n.to_string_lossy().to_string());
                    !name.map(|n| SKIP.contains(&n.as_str())).unwrap_or(false)
                })
                .collect(),
            Err(_) => return (0, 0),
        };
        let total = pending.len();
        pending.sort();

        let mut ok = 0usize;
        loop {
            let mut rest = Vec::new();
            let mut progress = 0usize;
            for p in pending {
                let success = match CString::new(p.to_string_lossy().as_bytes()) {
                    Ok(c) => unsafe { !dlopen(c.as_ptr(), RTLD_NOW | RTLD_GLOBAL).is_null() },
                    Err(_) => false,
                };
                if success {
                    progress += 1;
                } else {
                    rest.push(p);
                }
            }
            ok += progress;
            pending = rest;
            if progress == 0 || pending.is_empty() {
                break;
            }
        }
        (ok, total.saturating_sub(ok))
    }

    /// 已经建好的 VM（用于后续 attach，不重复建）
    pub fn cached_vm() -> Option<&'static jni::JavaVM> {
        VM.get()
    }

    pub fn cached_report() -> Option<EmbeddedReport> {
        REPORT.get().cloned()
    }

    /// 真正建 VM 的逻辑（阻塞，必须跑在独立线程）
    pub fn create_vm(cfg: &VmConfig) -> EmbeddedReport {
        let mut r = EmbeddedReport::default();
        let jre_home = &cfg.jre_home;
        r.java_dir = jre_home.to_string_lossy().to_string();
        r.classpath = cfg.classpath.clone();
        r.library_path = cfg.library_path.clone();

        let libjvm = libjvm_of(jre_home);
        r.libjvm_path = libjvm.to_string_lossy().to_string();
        r.libjvm_exists = libjvm.exists();
        if !r.libjvm_exists {
            r.verdict = format!(
                "找不到 {}。先确认这台设备上装了 Java，且 JRE 架构与设备 ABI 匹配。",
                r.libjvm_path
            );
            return r;
        }

        let started = std::time::Instant::now();

        // ---- 1. dlopen
        let c_path = match CString::new(libjvm.to_string_lossy().as_bytes()) {
            Ok(c) => c,
            Err(_) => {
                r.dlopen_error = "路径含 NUL 字节".into();
                return r;
            }
        };
        let handle = unsafe {
            let _ = dlerror();
            dlopen(c_path.as_ptr(), RTLD_NOW | RTLD_GLOBAL)
        };
        if handle.is_null() {
            r.dlopen_error = unsafe { take_dlerror() };
            r.verdict = format!("dlopen libjvm.so 失败：{}", r.dlopen_error);
            return r;
        }
        r.dlopen_ok = true;

        // ---- 1.5 先把 JRE 自己的 JNI 库全部拉进 linker namespace
        // （不做这步，libnio 找不到同目录的 libnet，后面任何用到 NIO 的代码都会炸）
        let (ok, fail) = preload_jre_libs(&jre_home.join("lib"));
        r.preload_ok = ok;
        r.preload_fail = fail;

        // ---- 2. 找函数
        let sym = unsafe {
            let name = CString::new("JNI_CreateJavaVM").unwrap();
            dlsym(handle, name.as_ptr())
        };
        if sym.is_null() {
            r.verdict =
                "libjvm.so 里没有 JNI_CreateJavaVM 符号，这个 JRE 可能不是标准 HotSpot 布局".into();
            return r;
        }
        r.symbol_ok = true;

        // ---- 3. 组装 JavaVMInitArgs
        //
        // 这些选项不是随便挑的：
        // - `-Djava.home` 必须给，否则 JVM 会去 `/` 找运行时，读不到 modules/classlist
        // - `-Djava.library.path` 决定 `System.loadLibrary` 能不能找到我们的 .so
        // - `-Xmx` 限住，避免先把手机内存吃满掩盖真正的问题
        let mut opt_strings = vec![
            format!("-Djava.home={}", jre_home.display()),
            format!("-Djava.class.path={}", cfg.classpath),
            format!("-Djava.library.path={}", cfg.library_path),
            // LWJGL 自己也会读这个属性（Library.loadNative 的第二条查找路径）
            format!("-Dorg.lwjgl.librarypath={}", cfg.lwjgl_path),
            format!("-Djava.io.tmpdir={}", jre_home.join("..").display()),
            "-Xmx512m".to_string(),
            // 只到 C1：C2 在这类环境里编出来的代码更容易踩 CPU/hypervisor 的坑。
            // 想放开不用改代码 —— `jvm-opts.txt` 里的选项排在默认值之后，
            // 写一行 `-XX:TieredStopAtLevel=4` 就能覆盖（重复的 -XX 后者生效）。
            "-XX:TieredStopAtLevel=1".to_string(),
            // 关掉 AVX：HotSpot 的**手写汇编桩**（CRC32、arraycopy、字符串比较）
            // 会按 CPUID 自动选 AVX-512 版本，而模拟器的 vCPU 常常「声明了但存不住
            // 512 位寄存器状态」，一执行就 #UD（SIGILL，`si_addr` 还是垃圾值，
            // 栈上看不出任何库名，只有 `<anonymous>`）。
            //
            // 实测：MuMu 上不加这个，MC 会在启动 0.5 秒左右随机 SIGILL；加上之后
            // 该崩溃消失（换成另一个 MuMu GLES 编码器的问题，见 docs）。
            // aarch64 上 HotSpot 不认这个开关，会被 `ignoreUnrecognized` 忽略。
            "-XX:UseAVX=0".to_string(),
        ];
        opt_strings.extend(cfg.extra_opts.iter().cloned());

        // 调试逃生口：`<data>/jvm-opts.txt` 里一行一个选项，追加到默认选项之后。
        // 安卓上改一行 JVM 参数要重新打 250 MB 的包、装 8 分钟，这个文件让
        // 「换参数 → 重启 App」变成 adb push + am force-stop 两步。
        let extra_file = jre_home.join("..").join("jvm-opts.txt");
        if let Ok(text) = std::fs::read_to_string(&extra_file) {
            for line in text.lines() {
                let s = line.trim();
                if s.is_empty() || s.starts_with('#') {
                    continue;
                }
                println!("[ccl-jvm] 追加选项 {s}");
                opt_strings.push(s.to_string());
            }
        }

        let mut c_opts: Vec<CString> = opt_strings
            .iter()
            .filter_map(|s| CString::new(s.as_str()).ok())
            .collect();
        let mut options: Vec<jni::sys::JavaVMOption> = c_opts
            .iter_mut()
            .map(|s| jni::sys::JavaVMOption {
                optionString: s.as_ptr() as *mut c_char,
                extraInfo: null_mut(),
            })
            .collect();

        let mut args = jni::sys::JavaVMInitArgs {
            version: jni::sys::JNI_VERSION_1_8,
            nOptions: options.len() as i32,
            options: options.as_mut_ptr(),
            ignoreUnrecognized: 1,
        };

        // ---- 4. 建 VM
        let create: CreateJavaVmFn = unsafe { std::mem::transmute(sym) };
        let mut vm_ptr: *mut c_void = null_mut();
        let mut env_ptr: *mut c_void = null_mut();
        let rc = unsafe { create(&mut vm_ptr, &mut env_ptr, &mut args as *mut _ as *mut c_void) };

        r.create_rc = rc;
        r.create_rc_name = rc_name(rc).to_string();
        if rc != 0 || vm_ptr.is_null() {
            r.verdict = format!(
                "JNI_CreateJavaVM 返回 {}（{}），VM 没建起来。常见原因：JRE 与设备 ABI 不符、\
                 JRE 不完整（缺 modules）、或内存不足。",
                rc, r.create_rc_name
            );
            return r;
        }
        r.elapsed_ms = started.elapsed().as_millis() as u64;

        // ---- 5. 通过 JNI 读几个属性
        let vm = match unsafe { jni::JavaVM::from_raw(vm_ptr as *mut jni::sys::JavaVM) } {
            Ok(v) => v,
            Err(e) => {
                r.verdict = format!("VM 指针包装失败：{e}");
                return r;
            }
        };
        let mut env = match unsafe { jni::JNIEnv::from_raw(env_ptr as *mut jni::sys::JNIEnv) } {
            Ok(e) => e,
            Err(e) => {
                r.verdict = format!("JNIEnv 包装失败：{e}");
                return r;
            }
        };

        fn prop(env: &mut jni::JNIEnv, key: &str) -> String {
            let read = |env: &mut jni::JNIEnv| -> Result<String, jni::errors::Error> {
                let k = env.new_string(key)?;
                let ko: jni::objects::JObject = k.into();
                let v = env.call_static_method(
                    "java/lang/System",
                    "getProperty",
                    "(Ljava/lang/String;)Ljava/lang/String;",
                    &[jni::objects::JValue::Object(&ko)],
                )?;
                let obj = v.l()?;
                if obj.is_null() {
                    return Ok(String::new());
                }
                let js = jni::objects::JString::from(obj);
                let s = env.get_string(&js)?;
                Ok(s.to_str().unwrap_or("<非 UTF-8>").to_string())
            };
            read(env).unwrap_or_else(|e| format!("<读取失败: {e}>"))
        }

        r.java_version = prop(&mut env, "java.version");
        r.java_vm_name = prop(&mut env, "java.vm.name");
        r.java_vm_version = prop(&mut env, "java.vm.version");
        r.os_arch = prop(&mut env, "os.arch");
        r.java_home = prop(&mut env, "java.home");
        r.vm_ok = !r.java_version.is_empty() && !r.java_version.starts_with('<');

        if r.vm_ok {
            r.verdict = format!(
                "内嵌 JVM 可用（{}，{} ms）。",
                r.java_version, r.elapsed_ms
            );
        } else {
            r.verdict = format!(
                "VM 建成但属性读不出来：{}。JNIEnv 或类加载可能有问题。",
                r.java_version
            );
        }

        // 保持 vm 存活：不调 DestroyJavaVM（会阻塞到所有非 daemon 线程退出）
        if r.vm_ok {
            let _ = VM.set(vm);
        } else {
            std::mem::forget(vm);
        }
        r
    }
}

/// 串行化建 VM 的尝试：两个线程同时进来会一个成功一个 JNI_EEXIST，
/// 加锁后第二个直接拿到缓存。
static CREATE_LOCK: Mutex<()> = Mutex::new(());

/// 拿到进程内唯一的 VM；第一次调用会真正创建。
///
/// 无论成功失败都返回报告（失败时 `vm` 为 `None`），这样 UI 总能显示细节。
/// `report.cached=true` 表示这是之前建好的。
#[cfg(target_os = "android")]
pub fn ensure_vm(cfg: VmConfig) -> (Option<&'static jni::JavaVM>, EmbeddedReport) {
    let _guard = match CREATE_LOCK.lock() {
        Ok(g) => g,
        Err(_) => {
            let mut r = EmbeddedReport::default();
            r.verdict = "VM 创建锁中毒".into();
            return (None, r);
        }
    };

    if let (Some(vm), Some(mut report)) = (imp::cached_vm(), imp::cached_report()) {
        report.cached = true;
        return (Some(vm), report);
    }

    let report = imp::create_vm(&cfg);
    let vm = imp::cached_vm();
    (vm, report)
}

/// 用这份 classpath 还能不能建 VM。
///
/// - VM 没建过 → `true`，随时可建
/// - VM 已建、classpath 一致 → `true`，可以直接用
/// - VM 已建、classpath 不同 → `false`，游戏类永远加载不到（classpath 在建 VM
///   那一刻就定死了），只能重启进程
///
/// 这是 M4-b 的硬约束：探针（M2/M3）一旦跑过，VM 就用「三 jar」的 classpath
/// 建好了。之后想真启动 MC，必须走「先 [`crate::java::launch::plan`]、再建 VM」
/// 的顺序，中间不能插探针。
pub fn vm_alive_for_launch(classpath: &str) -> bool {
    #[cfg(target_os = "android")]
    {
        match imp::cached_report() {
            None => true,
            Some(r) => r.vm_ok && r.classpath == classpath,
        }
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = classpath;
        false
    }
}

/// 把当前 JRE 与 LWJGL 路径拼成一份 VM 配置。
///
/// 所有需要 JVM 的命令都应该经这里，保证 classpath 顺序一致：
/// **fork 版 glfw classes 与探针必须排在最前面**，否则官方的
/// `lwjgl-glfw-3.3.3.jar` 会先被找到，Android 上根本跑不起来。
#[cfg(target_os = "android")]
pub fn build_config(app: &AppHandle, jre_home: PathBuf) -> Result<VmConfig, String> {
    build_config_with(app, jre_home, None, None)
}

/// 同 [`build_config`]，但允许注入完整 classpath。
///
/// 真启动 MC 时必须走这条：classpath 由 [`crate::java::launch::plan`] 算出来，
/// 里面有版本 jar 与全套 libraries。传 `None` 走探针那套「三个 jar」。
///
/// **顺序不能乱**：`-Djava.class.path` 只在建 VM 这一刻生效，之后改不了。
#[cfg(target_os = "android")]
pub fn build_config_with(
    app: &AppHandle,
    jre_home: PathBuf,
    launch_classpath: Option<String>,
    launch_opts: Option<Vec<String>>,
) -> Result<VmConfig, String> {
    let lw = crate::java::lwjgl::ensure(app)?;
    // 顺序有语义：shim 的 CallbackBridge 必须压过 fat jar 里那份 v2 实现，
    // 否则 libpojavexec 的 JNI_OnLoad 找不到 v3 回调，直接 NoSuchMethodError。
    let classpath = launch_classpath.unwrap_or_else(|| {
        [
            lw.shim_jar.to_string_lossy().to_string(),
            lw.probe_jar.to_string_lossy().to_string(),
            lw.classes_jar.to_string_lossy().to_string(),
        ]
        .join(":")
    });
    let lwjgl_path = lw.dir.to_string_lossy().to_string();
    // 顺序有讲究：JRE 自己的 lib 必须在内，libnio/libnet/libzip 这些都在那儿；
    // 我们的 natives 放最后，避免同名库盖掉 JRE 的。
    let library_path = format!(
        "{}/lib:{}/lib/server:{}",
        jre_home.display(),
        jre_home.display(),
        lwjgl_path
    );
    Ok(VmConfig {
        jre_home,
        classpath,
        library_path,
        lwjgl_path,
        // MC 1.17+ 要 GL 3.2，Android 上唯一能提供的是 LTW（GLES→GL 翻译层）。
        // LWJGL 会在 createCapabilities 时按这个名字去 loadNative，必须提前定死；
        // 探针里也设了一遍，但真实启动走的是这里。
        extra_opts: launch_opts
            .unwrap_or_else(|| vec!["-Dorg.lwjgl.opengl.libname=libltw.so".to_string()]),
    })
}

/// 由 JRE 根目录反推配置并确保 VM 已建。
/// 返回 `(vm, report)`，`vm` 为 `None` 表示建失败（`report` 里有原因）。
#[cfg(target_os = "android")]
pub fn ensure_vm_for(
    app: &AppHandle,
    jre_home: PathBuf,
) -> (Option<&'static jni::JavaVM>, EmbeddedReport) {
    match build_config(app, jre_home) {
        Ok(cfg) => ensure_vm(cfg),
        Err(e) => {
            let mut r = EmbeddedReport::default();
            r.verdict = e;
            (None, r)
        }
    }
}

/// 按「启动计划」的 classpath 建 VM。`launch_classpath` 来自
/// [`crate::java::launch::plan`]，必须与 VM 已经建成的 classpath 一致，
/// 否则这里返回的 `vm` 是 `None`（VM 只能建一次）。
#[cfg(target_os = "android")]
pub fn ensure_vm_for_launch(
    app: &AppHandle,
    jre_home: PathBuf,
    launch_classpath: String,
    launch_opts: Vec<String>,
) -> (Option<&'static jni::JavaVM>, EmbeddedReport) {
    match build_config_with(app, jre_home, Some(launch_classpath), Some(launch_opts)) {
        Ok(cfg) => ensure_vm(cfg),
        Err(e) => {
            let mut r = EmbeddedReport::default();
            r.verdict = e;
            (None, r)
        }
    }
}

/// 执行内嵌 JVM 实验（M1 页面上的那张卡）。
///
/// 结果按进程缓存：`JNI_CreateJavaVM` 只能成功一次，第二次必然 `JNI_EEXIST`，
/// 直接返回上次结论比返回一个误导性的错误码更有用。
pub async fn run(app: AppHandle) -> Result<EmbeddedReport, String> {
    #[cfg(not(target_os = "android"))]
    {
        let _ = app;
        return Err("内嵌 JVM 实验只对 Android 有意义".into());
    }

    #[cfg(target_os = "android")]
    {
        if let Some(mut prev) = imp::cached_report() {
            prev.cached = true;
            return Ok(prev);
        }

        let report = tokio::task::spawn_blocking(move || {
            let javas = crate::java::scan_javas(app.clone());
            let info = javas
                .into_iter()
                .max_by_key(|j| j.major)
                .ok_or_else(|| "没有已安装的 Java，先去 Java 页装一个".to_string())?;
            let bin = std::path::PathBuf::from(&info.path);
            let jre_home = bin
                .parent()
                .and_then(|p| p.parent())
                .map(|p| p.to_path_buf())
                .ok_or_else(|| format!("无法从 {} 推出 JRE 根目录", info.path))?;
            Ok::<_, String>(ensure_vm_for(&app, jre_home))
        })
        .await
        .map_err(|e| format!("实验线程异常: {e}"))??;

        Ok(report.1)
    }
}
