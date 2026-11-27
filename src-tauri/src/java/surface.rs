//! Surface 桥接（M3 里程碑）
//!
//! ## 这个模块解决什么
//!
//! `libpojavexec` 里所有渲染都依赖一个进程级的全局窗口指针：
//!
//! ```c
//! // input_bridge_v3.c 简化示意
//! void Java_..._setupBridgeWindow(JNIEnv *env, jclass c, jobject surface) {
//!     pojav_environ->window = ANativeWindow_fromSurface(env, surface);
//! }
//!
//! jint pojavInit(void) {
//!     ANativeWindow_acquire(pojav_environ->window);   // window == NULL 就崩
//!     ...
//! }
//! ```
//!
//! 也就是说：**没有 window，`glfwInit` 必然 SIGSEGV**（M2 里就是这么崩的）。
//!
//! ## 两个 VM 的顺序问题（最容易踩的坑）
//!
//! `pojav_environ` 是一个用 `POJAV_ENVIRON` 环境变量在两个 VM 之间共享的结构体。
//! `JNI_OnLoad` 按「谁先加载」分流：
//!
//! ```c
//! if (dalvikJavaVMPtr == NULL) {            // 第一次加载 = ART
//!     dalvikJavaVMPtr = vm;
//!     bridgeClazz = FindClass("org/lwjgl/glfw/CallbackBridge");
//!     ...缓存 accessAndroidClipboard / onGrabStateChanged / onDirectInputEnable...
//! } else if (dalvikJavaVMPtr != vm) {       // 第二次加载 = 内嵌 JVM
//!     runtimeJavaVMPtr = vm;
//!     ...缓存 GLFW 的 internalChangeMonitorSize 等方法...
//! }
//! ```
//!
//! `pojavInit` 用的是 `runtimeJavaVMPtr`。所以**必须 ART 先加载、JVM 后加载**，
//! 顺序反了 `pojavInit` 会拿 ART 的 env 去调 JVM 的类，直接崩。
//!
//! ## 为什么要在 Java 帧里做
//!
//! 第一次加载时 `FindClass("org/lwjgl/glfw/CallbackBridge")` 必须命中 APK 的
//! dex。JNI 规范：**原生线程**的 `FindClass` 用系统类加载器（找不到），
//! 只有从声明 native 方法的类的帧进来才用 App 的类加载器。
//! 所以入口是 [`Java_com_caiwenjun_ccl_NativeBridge_setupWindowNative`]，
//! 由 Java 侧的 `NativeBridge.setupWindow()` 调起 —— 见
//! `gen/android/app/src/main/java/com/caiwenjun/ccl/NativeBridge.java`。

use serde::Serialize;

/// 一次 Surface 桥接的结果（给前端和日志看）
#[derive(Serialize, Clone, Default)]
pub struct SurfaceInfo {
    /// 窗口指针是否成功交给 pojavexec
    pub ok: bool,
    /// SurfaceView 的宽高（= pojavexec 认为的「显示器尺寸」）
    pub width: i32,
    pub height: i32,
    /// 失败原因
    pub error: String,
    /// 等待 + 桥接耗时
    pub elapsed_ms: u64,
}

#[cfg(not(target_os = "android"))]
pub fn attach(_dir: &std::path::Path, _timeout_ms: i64) -> SurfaceInfo {
    SurfaceInfo {
        ok: false,
        error: "Surface 桥接只对 Android 有意义".into(),
        ..Default::default()
    }
}

/// 把 `dir/libpojavexec.so` 接上当前 Activity 的 Surface。
///
/// 必须在建内嵌 JVM **之前**调用：只有 ART 先加载 so，
/// `JNI_OnLoad` 才会把 `dalvikJavaVMPtr` 占住，JVM 加载时才能正确记下
/// `runtimeJavaVMPtr`。
#[cfg(target_os = "android")]
pub fn attach(dir: &std::path::Path, timeout_ms: i64) -> SurfaceInfo {
    use std::panic::{catch_unwind, AssertUnwindSafe};
    match catch_unwind(AssertUnwindSafe(|| imp::attach(dir, timeout_ms))) {
        Ok(info) => info,
        Err(_) => SurfaceInfo {
            ok: false,
            error: "桥接过程 panic（见 logcat）".into(),
            ..Default::default()
        },
    }
}

// ---------------------------------------------------------------------------
// 图层控制（M4-a）
// ---------------------------------------------------------------------------

#[cfg(not(target_os = "android"))]
pub fn set_overlay(_on_top: bool) {}

/// 把游戏画面收回 WebView 下面（M4-a 演示结束后把 UI 还给用户）。
///
/// **只能在渲染结束之后调**：改 z-order 会让 SurfaceView 重建 Surface，
/// 已经交给 `libpojavexec` 的 `ANativeWindow` 就此失效。
/// Java 侧的 `GameSurface.setOverlay` 会先比对状态，没变就什么都不做。
#[cfg(target_os = "android")]
pub fn set_overlay(on_top: bool) {
    use jni::objects::{JObject, JValue};
    use std::panic::{catch_unwind, AssertUnwindSafe};

    let r = catch_unwind(AssertUnwindSafe(|| -> Result<(), String> {
        let ctx = ndk_context::android_context();
        let vm = unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) }
            .map_err(|e| format!("包装 ART JavaVM 失败：{e}"))?;
        let mut env = vm
            .attach_current_thread()
            .map_err(|e| format!("attach ART 失败：{e}"))?;
        let activity = unsafe { JObject::from_raw(ctx.context() as jni::sys::jobject) };
        let cls = imp::load_app_class(&mut env, &activity, "com.caiwenjun.ccl.GameSurface")?;
        env.call_static_method(&cls, "setOverlay", "(Z)V", &[JValue::Bool(on_top as u8)])
            .map_err(|e| format!("调 GameSurface.setOverlay 失败：{e}"))?;
        Ok(())
    }));

    match r {
        Ok(Ok(())) => println!("[ccl-surface] setOverlay({on_top}) ok"),
        Ok(Err(e)) => println!("[ccl-surface] setOverlay({on_top}) 失败：{e}"),
        Err(_) => println!("[ccl-surface] setOverlay({on_top}) panic"),
    }
}

#[cfg(not(target_os = "android"))]
pub fn prepare_game_surface(_timeout_ms: i64) -> String {
    "ERR:Surface 桥接只对 Android 有意义".into()
}

/// 切「游戏置顶」并等到一块可用 Surface，返回 `OK|...` / `ERR:...`。
///
/// **必须在 [`attach`] 之前调**。原因见 `GameSurface.prepareGameSurface` 的注释：
/// vivo 等 ROM 上 `setZOrderOnTop` 会重建 SurfaceView 的 Surface，而重建是异步的
/// （实测 `surfaceDestroyed` 之后 4 秒才来新的 `surfaceCreated`）。
/// 早期实现只是「置顶 → sleep(400ms) → attach」，在真机上必然拿到死 Surface。
#[cfg(target_os = "android")]
pub fn prepare_game_surface(timeout_ms: i64) -> String {
    use jni::objects::{JObject, JValue};
    use std::panic::{catch_unwind, AssertUnwindSafe};

    let r = catch_unwind(AssertUnwindSafe(|| -> Result<String, String> {
        let ctx = ndk_context::android_context();
        let vm = unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) }
            .map_err(|e| format!("包装 ART JavaVM 失败：{e}"))?;
        let mut env = vm
            .attach_current_thread()
            .map_err(|e| format!("attach ART 失败：{e}"))?;
        let activity = unsafe { JObject::from_raw(ctx.context() as jni::sys::jobject) };
        let cls = imp::load_app_class(&mut env, &activity, "com.caiwenjun.ccl.GameSurface")?;
        let v = env
            .call_static_method(
                &cls,
                "prepareGameSurface",
                "(J)Ljava/lang/String;",
                &[JValue::Long(timeout_ms)],
            )
            .map_err(|e| format!("调 GameSurface.prepareGameSurface 失败：{e}"))?;
        let obj = v.l().map_err(|e| format!("返回值不是对象：{e}"))?;
        if obj.is_null() {
            return Ok("ERR:prepareGameSurface 返回 null".into());
        }
        let s: String = env
            .get_string(&jni::objects::JString::from(obj))
            .map_err(|e| format!("读返回字符串失败：{e}"))?
            .to_string_lossy()
            .to_string();
        Ok(s)
    }));

    match r {
        Ok(Ok(s)) => {
            println!("[ccl-surface] prepareGameSurface -> {s}");
            s
        }
        Ok(Err(e)) => {
            println!("[ccl-surface] prepareGameSurface 失败：{e}");
            format!("ERR:{e}")
        }
        Err(_) => "ERR:prepareGameSurface panic".into(),
    }
}

// ---------------------------------------------------------------------------
// 输入就绪（M4-b-3）
// ---------------------------------------------------------------------------

#[cfg(not(target_os = "android"))]
pub fn enable_input() -> String {
    "输入开关只对 Android 有意义".into()
}

/// 保险：把 `libpojavexec` 的 `input_ready` 再确认一次，返回一句话结果。
///
/// **不是必需的**：`JNI_OnLoad` 的 JVM 分支自己会把它设成 1（logcat 里
/// `input_bridge_v3.c: Input ready: 1`）。这里只是启动时再断言一次并留一条日志，
/// 顺便覆盖「以后换了一版 so、行为变了」的情况。
///
/// ## 为什么绕过 JNI 方法查找
///
/// `Java_org_lwjgl_glfw_CallbackBridge_nativeSetInputReady` 在 so 里**有导出**，
/// 但它不在 `RegisterNatives` 那张表里。而我们的 so 是 Rust `dlopen` 进来的，
/// ART 的 ClassLoader 并不知道它 —— 非注册的 native 方法要靠
/// 「类加载器 → 已加载库」这条链查符号，链断了就报：
///
/// ```text
/// UnsatisfiedLinkError: No implementation found for
///   boolean org.lwjgl.glfw.CallbackBridge.nativeSetInputReady(boolean)
/// ```
///
/// 所以这里直接 `dlsym` 拿函数指针调（反汇编看它只做
/// `movb %cl, 0x271d0(%rax)` 一件小事）。
#[cfg(target_os = "android")]
pub fn enable_input() -> String {
    use std::ffi::{CStr, CString};
    use std::os::raw::{c_char, c_int, c_void};

    extern "C" {
        fn dlopen(filename: *const c_char, flag: c_int) -> *mut c_void;
        fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
        fn dlerror() -> *mut c_char;
    }
    const RTLD_NOW: c_int = 0x0002;

    /// `JNIEXPORT jboolean JNICALL JavaCritical_...(jboolean)`
    ///
    /// 「critical native」不做 JNI 参数搬运，直接是 `bool -> bool` 的 C ABI。
    type SetReadyFn = unsafe extern "C" fn(u8) -> u8;

    let Some(path) = imp::so_path() else {
        return "ERR:还没桥接过 Surface（不知道 so 在哪）".into();
    };
    let c_path = match CString::new(path.clone()) {
        Ok(c) => c,
        Err(_) => return "ERR:so 路径含 NUL".into(),
    };

    unsafe {
        let _ = dlerror();
        // 同一路径的第二次 dlopen 只是引用计数 +1，拿到的是同一个句柄
        let handle = dlopen(c_path.as_ptr(), RTLD_NOW);
        if handle.is_null() {
            let e = dlerror();
            let msg = if e.is_null() {
                "（无详情）".to_string()
            } else {
                CStr::from_ptr(e).to_string_lossy().to_string()
            };
            return format!("ERR:dlopen {path} 失败：{msg}");
        }
        let sym = dlsym(
            handle,
            CString::new("JavaCritical_org_lwjgl_glfw_CallbackBridge_nativeSetInputReady")
                .unwrap()
                .as_ptr(),
        );
        if sym.is_null() {
            return "ERR:so 里没有 nativeSetInputReady（这版 pojavexec 不支持？）".into();
        }
        let f: SetReadyFn = std::mem::transmute(sym);
        let ret = f(1);
        format!("OK|input_ready=1（nativeSetInputReady 返回 {ret}）")
    }
}

// ---------------------------------------------------------------------------
// 输入自检（M4-b-3 联调）
// ---------------------------------------------------------------------------

#[cfg(not(target_os = "android"))]
pub fn debug_send_cursor(_x: f32, _y: f32) -> String {
    "只对 Android 有意义".into()
}

/// **联调专用**：绕过 Java native，直接 `dlsym` 调 so 里的
/// `critical_send_cursor_pos(float, float)`。
///
/// 用途是二分「ART 侧注册的 native 到底有没有跑到这段 C 代码」：
/// - 直调有效、Java 调用无效 → 问题在 ART 的绑定/门禁，不在事件通路
/// - 直调也无效 → 门禁在 `pojav_environ` 的某个字段上
#[cfg(target_os = "android")]
pub fn debug_send_cursor(x: f32, y: f32) -> String {
    use std::ffi::{CStr, CString};
    use std::os::raw::{c_char, c_int, c_void};

    extern "C" {
        fn dlopen(filename: *const c_char, flag: c_int) -> *mut c_void;
        fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
        fn dlerror() -> *mut c_char;
    }
    const RTLD_NOW: c_int = 0x0002;

    let Some(path) = imp::so_path() else {
        return "ERR:还没有 so 路径".into();
    };
    let c_path = match CString::new(path) {
        Ok(c) => c,
        Err(_) => return "ERR:路径含 NUL".into(),
    };
    unsafe {
        let handle = dlopen(c_path.as_ptr(), RTLD_NOW);
        if handle.is_null() {
            let e = dlerror();
            return format!(
                "ERR:dlopen 失败：{}",
                if e.is_null() { "?".into() } else { CStr::from_ptr(e).to_string_lossy().to_string() }
            );
        }
        let sym = dlsym(handle, CString::new("critical_send_cursor_pos").unwrap().as_ptr());
        if sym.is_null() {
            return "ERR:没有 critical_send_cursor_pos".into();
        }
        type Fn2 = unsafe extern "C" fn(f32, f32);
        let f: Fn2 = std::mem::transmute(sym);
        f(x, y);
        format!("OK|直调 critical_send_cursor_pos({x},{y})")
    }
}

#[cfg(not(target_os = "android"))]
pub fn input_self_test() -> String {
    "输入自检只对 Android 有意义".into()
}

/// 让 ART 侧的 `GameSurface.inputSelfTest()` 发一串输入，返回它的一句话结果。
///
/// 为什么由 Rust 触发而不是 Kotlin 自己在启动时跑：自检必须在
/// `libpojavexec` 被 ART 加载**之后**（否则是 `UnsatisfiedLinkError`），
/// 而那个时机只有启动 / 探针流程知道。开关文件：`<data>/input-test`。
#[cfg(target_os = "android")]
pub fn input_self_test() -> String {
    use jni::objects::{JObject, JString};
    use std::panic::{catch_unwind, AssertUnwindSafe};

    let r = catch_unwind(AssertUnwindSafe(|| -> Result<String, String> {
        let ctx = ndk_context::android_context();
        let vm = unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) }
            .map_err(|e| format!("包装 ART JavaVM 失败：{e}"))?;
        let mut env = vm
            .attach_current_thread()
            .map_err(|e| format!("attach ART 失败：{e}"))?;
        let activity = unsafe { JObject::from_raw(ctx.context() as jni::sys::jobject) };
        let cls = imp::load_app_class(&mut env, &activity, "com.caiwenjun.ccl.GameSurface")?;
        let v = env
            .call_static_method(&cls, "inputSelfTest", "()Ljava/lang/String;", &[])
            .map_err(|e| format!("调 inputSelfTest 失败：{e}"))?;
        let obj = v.l().map_err(|e| format!("返回值不是对象：{e}"))?;
        let s: String = env
            .get_string(&JString::from(obj))
            .map_err(|e| format!("读返回字符串失败：{e}"))?
            .to_string_lossy()
            .to_string();
        Ok(s)
    }));

    match r {
        Ok(Ok(s)) => s,
        Ok(Err(e)) => format!("ERR:{e}"),
        Err(_) => "ERR:inputSelfTest panic".into(),
    }
}

// ---------------------------------------------------------------------------
// JNI 导出：由 Java 的 NativeBridge.setupWindowNative 调进来
// ---------------------------------------------------------------------------

/// 真正干活的地方。**必须从 Java 帧调用**（`FindClass` 依赖调用方的类加载器）。
///
/// 返回 `OK|<宽>x<高>` 或 `ERR:<原因>`；JNI 侧再包成 `String` 返回。
#[cfg(target_os = "android")]
#[no_mangle]
pub extern "system" fn Java_com_caiwenjun_ccl_NativeBridge_setupWindowNative(
    raw_env: *mut jni::sys::JNIEnv,
    _class: jni::sys::jclass,
    so_path: jni::sys::jstring,
    surface: jni::sys::jobject,
) -> jni::sys::jstring {
    use std::panic::{catch_unwind, AssertUnwindSafe};
    use std::ptr::null_mut;

    let result = catch_unwind(AssertUnwindSafe(|| {
        imp::bridge_window(raw_env, so_path, surface)
    }))
    .unwrap_or_else(|_| "ERR:bridge_window panic".to_string());

    println!("[ccl-surface] {result}");

    // 把结果字符串传回 Java；转换失败就返回 null（Java 侧会看到 null）
    match unsafe { jni::JNIEnv::from_raw(raw_env) } {
        Ok(mut env) => match env.new_string(&result) {
            Ok(s) => s.into_raw(),
            Err(_) => null_mut(),
        },
        Err(_) => null_mut(),
    }
}

#[cfg(target_os = "android")]
mod imp {
    use super::SurfaceInfo;

    use jni::objects::{JClass, JObject, JString, JValue};
    use std::ffi::{CStr, CString};
    use std::os::raw::{c_char, c_int, c_void};
    use std::path::Path;
    use std::ptr::null_mut;

    extern "C" {
        fn dlopen(filename: *const c_char, flag: c_int) -> *mut c_void;
        fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
        fn dlerror() -> *mut c_char;
    }

    const RTLD_NOW: c_int = 0x0002;
    const RTLD_GLOBAL: c_int = 0x0100;

    /// `bridge_window` 里 dlopen 过的 so 绝对路径（M4-b-3 的 `enable_input` 要用）
    static SO_PATH: std::sync::OnceLock<String> = std::sync::OnceLock::new();

    /// 取桥接时记下的 so 路径
    pub(super) fn so_path() -> Option<String> {
        SO_PATH.get().cloned()
    }

    type JniOnLoadFn =
        unsafe extern "system" fn(*mut jni::sys::JavaVM, *mut c_void) -> jni::sys::jint;
    /// `JNIEXPORT void JNICALL Java_..._setupBridgeWindow(JNIEnv*, jclass, jobject)`
    type SetupWindowFn =
        unsafe extern "system" fn(*mut jni::sys::JNIEnv, jni::sys::jclass, jni::sys::jobject);

    fn take_dlerror() -> String {
        unsafe {
            let e = dlerror();
            if e.is_null() {
                "（dlerror 为空）".into()
            } else {
                CStr::from_ptr(e).to_string_lossy().to_string()
            }
        }
    }

    /// Rust 侧入口：借 ART 的 ClassLoader 找到 `com.caiwenjun.ccl.NativeBridge`，
    /// 然后让 **Java** 去调 `setupWindowNative`（这样栈上才有 App 类的帧）。
    pub fn attach(dir: &Path, timeout_ms: i64) -> SurfaceInfo {
        let started = std::time::Instant::now();
        let mut info = SurfaceInfo::default();

        // 渲染器必须在 pojavInit 之前定下来：它内部 getenv("POJAV_RENDERER")，
        // 匹配 "opengles" 前缀才 set_gl_bridge_tbl()，不设就是空函数表，后头必崩。
        std::env::set_var("POJAV_RENDERER", "opengles");
        // pojavInitOpenGL 里有 `strcmp(getenv("FORCE_VSYNC"), "true")`，
        // **没有 NULL 判断** —— 变量不存在就直接在 strcmp 里 SIGSEGV。
        // Pojav 是 Java 侧 setenv 的，我们得补上。
        std::env::set_var("FORCE_VSYNC", "false");
        // gl_init_context 里 `strtol(getenv("LIBGL_ES"), NULL, 0)` 同样不判空。
        // 语义是要 GLES 主版本：MC 1.17+ 要 GL 3.2，**只能是 3**。
        std::env::set_var("LIBGL_ES", "3");

        // ★ 关键：让 **EGL 也走 LTW**，而不是系统 libEGL。
        //
        // `egl_loader.c` 里：
        //     void* dl_handle = loader_dlopen(getenv("POJAVEXEC_EGL"), "libEGL.so", ...);
        //
        // 变量为空就退回系统 `libEGL.so`。那样 EGL 上下文是系统建的，
        // 而 LTW 自己完全不知道 —— 因为 LTW 的 `egl.c` 里是
        //
        //     thread_local context_t *current_context;   // 只有 LTW 自己的
        //     EGLBoolean eglMakeCurrent(...) { host_eglMakeCurrent(...); current_context = tw_context; }
        //
        // 而 LTW 的每个 GL 入口第一行都是 `if(!current_context) return NULL;`。
        // 于是现象就是：GL 上下文明明建好了、`eglMakeCurrent` 也没报错，
        // 但 `glGetString(GL_VERSION)` 返回 NULL，LWJGL 报
        // "There is no OpenGL context current in the current thread."
        //
        // 反过来也不能只把 EGL 指给 LTW 而让 LWJGL 去加载别的库：LTW 的
        // `eglCreateContext` 会把物理 context 记进自己的 context_map，
        // 之后 `eglMakeCurrent` 找不到就 `abort()`（"Failed to find context"）。
        // 两边必须是同一个 libltw.so —— 用绝对路径传给 dlopen，
        // 后面 LWJGL 再 `System.load` 同一路径时只是引用计数 +1。
        std::env::set_var("POJAVEXEC_EGL", dir.join("libltw.so"));

        let ctx = ndk_context::android_context();
        let vm = match unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) } {
            Ok(v) => v,
            Err(e) => {
                info.error = format!("包装 ART JavaVM 失败：{e}");
                return info;
            }
        };
        let mut env = match vm.attach_current_thread() {
            Ok(e) => e,
            Err(e) => {
                info.error = format!("attach 到 ART 失败：{e}");
                return info;
            }
        };

        let activity = unsafe { JObject::from_raw(ctx.context() as jni::sys::jobject) };
        let cls = match load_app_class(&mut env, &activity, "com.caiwenjun.ccl.NativeBridge") {
            Ok(c) => c,
            Err(e) => {
                info.error = e;
                return info;
            }
        };

        let so = dir.join("libpojavexec.so");
        let so_js = match env.new_string(so.to_string_lossy()) {
            Ok(s) => s,
            Err(e) => {
                info.error = format!("造 so 路径字符串失败：{e}");
                return info;
            }
        };
        let so_obj: JObject = so_js.into();

        let ret = env.call_static_method(
            &cls,
            "setupWindow",
            "(Ljava/lang/String;J)Ljava/lang/String;",
            &[JValue::Object(&so_obj), JValue::Long(timeout_ms)],
        );
        let ret = match ret.and_then(|v| v.l()) {
            Ok(o) if !o.is_null() => o,
            Ok(_) => {
                info.error = "NativeBridge.setupWindow 返回 null".into();
                return info;
            }
            Err(e) => {
                info.error = format!("调 NativeBridge.setupWindow 失败：{e}");
                return info;
            }
        };
        let text = match env.get_string(&JString::from(ret)) {
            Ok(s) => s.to_string_lossy().to_string(),
            Err(e) => {
                info.error = format!("读返回字符串失败：{e}");
                return info;
            }
        };

        info.elapsed_ms = started.elapsed().as_millis() as u64;
        if let Some(rest) = text.strip_prefix("OK|") {
            info.ok = true;
            let mut it = rest.split('x');
            info.width = it.next().and_then(|v| v.parse().ok()).unwrap_or(0);
            info.height = it.next().and_then(|v| v.parse().ok()).unwrap_or(0);
        } else {
            info.error = text.strip_prefix("ERR:").unwrap_or(&text).to_string();
        }
        info
    }

    /// 在 Java 帧里完成 dlopen → JNI_OnLoad → setupBridgeWindow。
    ///
    /// 由 [`super::Java_com_caiwenjun_ccl_NativeBridge_setupWindowNative`] 调用。
    pub fn bridge_window(
        raw_env: *mut jni::sys::JNIEnv,
        so_path: jni::sys::jstring,
        surface: jni::sys::jobject,
    ) -> String {
        let mut env = match unsafe { jni::JNIEnv::from_raw(raw_env) } {
            Ok(e) => e,
            Err(e) => return format!("ERR:JNIEnv 包装失败 {e}"),
        };

        let so: String = match env.get_string(unsafe { &JString::from(JObject::from_raw(so_path)) }) {
            Ok(s) => s.to_string_lossy().to_string(),
            Err(e) => return format!("ERR:读 so 路径失败 {e}"),
        };
        if surface.is_null() {
            return "ERR:Surface 是 null".into();
        }

        let c_so = match CString::new(so.as_bytes()) {
            Ok(c) => c,
            Err(_) => return "ERR:so 路径含 NUL".into(),
        };
        let handle = unsafe {
            let _ = dlerror();
            dlopen(c_so.as_ptr(), RTLD_NOW | RTLD_GLOBAL)
        };
        if handle.is_null() {
            return format!("ERR:dlopen {} 失败：{}", so, take_dlerror());
        }
        // 记下来：`enable_input` 要重新 dlopen 拿句柄去 dlsym
        let _ = SO_PATH.set(so.clone());

        let onload_sym = unsafe { dlsym(handle, CString::new("JNI_OnLoad").unwrap().as_ptr()) };
        let setup_sym = unsafe {
            dlsym(
                handle,
                CString::new("Java_net_kdt_pojavlaunch_utils_JREUtils_setupBridgeWindow")
                    .unwrap()
                    .as_ptr(),
            )
        };
        if onload_sym.is_null() || setup_sym.is_null() {
            return "ERR:libpojavexec 缺少 JNI_OnLoad / setupBridgeWindow".into();
        }

        let java_vm = match env.get_java_vm() {
            Ok(v) => v,
            Err(e) => return format!("ERR:取 JavaVM 失败 {e}"),
        };

        // 1) ART 先加载一次：占住 dalvikJavaVMPtr 并缓存 CallbackBridge。
        //    这里能成功的前提就是「有 App 类的帧」——否则 FindClass 找不到类。
        let onload: JniOnLoadFn = unsafe { std::mem::transmute(onload_sym) };
        let rc = unsafe { onload(java_vm.get_java_vm_pointer(), null_mut()) };
        // JNI_OnLoad 内部可能留下异常（比如 RegisterNatives 的签名不匹配）
        let _ = env.exception_clear();
        if rc < 0 {
            return format!("ERR:ART 侧 JNI_OnLoad 返回 {rc}");
        }

        // 2) 把 Surface 变成 ANativeWindow 塞进 pojav_environ
        let setup: SetupWindowFn = unsafe { std::mem::transmute(setup_sym) };
        unsafe { setup(env.get_native_interface(), null_mut(), surface) };

        // 3) 回读尺寸（pojavInit 会用它算显示器尺寸）
        let (w, h) = match env.find_class("com/caiwenjun/ccl/GameSurface") {
            Ok(cls) => {
                let w = env
                    .call_static_method(&cls, "surfaceWidth", "()I", &[])
                    .and_then(|v| v.i())
                    .unwrap_or(0);
                let h = env
                    .call_static_method(&cls, "surfaceHeight", "()I", &[])
                    .and_then(|v| v.i())
                    .unwrap_or(0);
                (w, h)
            }
            Err(_) => {
                let _ = env.exception_clear();
                (0, 0)
            }
        };

        format!("OK|{w}x{h}")
    }

    /// 用 Activity 的 ClassLoader 加载 App 自己的类（原生线程上 find_class 不可用）
    pub(super) fn load_app_class<'local>(
        env: &mut jni::JNIEnv<'local>,
        activity: &JObject,
        name: &str,
    ) -> Result<JClass<'local>, String> {
        let loader = env
            .call_method(activity, "getClassLoader", "()Ljava/lang/ClassLoader;", &[])
            .and_then(|v| v.l())
            .map_err(|e| format!("取 Activity.getClassLoader 失败：{e}"))?;
        let js = env
            .new_string(name)
            .map_err(|e| format!("造类名字符串失败：{e}"))?;
        let cls = env
            .call_method(
                &loader,
                "loadClass",
                "(Ljava/lang/String;)Ljava/lang/Class;",
                &[JValue::Object(&js)],
            )
            .and_then(|v| v.l())
            .map_err(|e| format!("loadClass({name}) 失败：{e}"))?;
        if cls.is_null() {
            return Err(format!("{name} 没找到（编译进去了吗？）"));
        }
        Ok(JClass::from(cls))
    }
}
