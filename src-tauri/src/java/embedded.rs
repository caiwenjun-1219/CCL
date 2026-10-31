//! 内嵌 JVM 实验（M1 里程碑）
//!
//! ## 为什么需要这个
//!
//! Android 上 `Command::new(java)` 起的是独立进程。它没有 ART、没有 Binder，
//! 拿不到 SurfaceView 的 `ANativeWindow`，因此 MC 的图形层（EGL）根本无从建立。
//! Pojav / FCL 走的是另一条路：把 `lib/server/libjvm.so` 映射进 App 自己的进程，
//! 调 `JNI_CreateJavaVM` 建 VM，这样渲染、输入、Surface 全在同进程内。
//!
//! ## 这一步只验证什么
//!
//! 只验证「VM 能不能在本进程里建起来」以及「能不能从里面读出系统属性」。
//! 建不起来，后面的 glfw stub / EGL 桥 / GL4ES 全部免谈，所以必须先验。
//!
//! ## 已知约束
//!
//! - `JNI_CreateJavaVM` **一个进程只能成功一次**，第二次返回 `JNI_EEXIST`。
//!   所以结果用 `OnceLock` 缓存，重复调用直接返回上次的结论。
//! - VM 建起来之后**不要销毁**（`DestroyJavaVM` 会阻塞到所有非 daemon 线程退出）。
//!   App 退出即回收，M1 阶段不需要优雅关闭。
//! - 调用线程会变成 JVM 的 main 线程，因此必须放在 `spawn_blocking` 里，
//!   不能占用异步 runtime 线程。

use serde::Serialize;
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

    /// 创建耗时（毫秒）—— 冷启动体验的关键指标
    pub elapsed_ms: u64,
    /// 是否为缓存的旧结果（VM 只能建一次）
    pub cached: bool,
    /// 给人看的结论
    pub verdict: String,
}

#[cfg(target_os = "android")]
mod imp {
    use super::EmbeddedReport;

    use std::ffi::{CStr, CString};
    use std::os::raw::{c_char, c_int, c_void};
    use std::path::{Path, PathBuf};
    use std::ptr::null_mut;
    use std::sync::OnceLock;

    /// 进程内唯一的一次实验结果
    static RESULT: OnceLock<EmbeddedReport> = OnceLock::new();

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

    /// 真正建 VM 的逻辑（阻塞，必须跑在独立线程）
    pub fn create_vm(jre_home: &Path) -> EmbeddedReport {
        let mut r = EmbeddedReport::default();
        r.java_dir = jre_home.to_string_lossy().to_string();

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
        // - `-Djava.class.path` 给个存在的目录即可，M1 不需要业务类
        // - `-Xmx` 限住，避免先把手机内存吃满掩盖真正的问题
        let classpath_dir = jre_home.join("..").join("embedded-cp");
        let _ = std::fs::create_dir_all(&classpath_dir);

        let opt_strings = vec![
            format!("-Djava.home={}", jre_home.display()),
            format!("-Djava.class.path={}", classpath_dir.display()),
            format!("-Djava.io.tmpdir={}", jre_home.join("..").display()),
            "-Xmx256m".to_string(),
            // 只到 C1：模拟器上 C2 会编出 CPU 不支持的指令（Fatal signal 4），
            // M1 只需要调 System.getProperty，用不着 C2
            "-XX:TieredStopAtLevel=1".to_string(),
        ];
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
                "内嵌 JVM 可用（{}，{} ms）。下一步接 glfw stub + EGL 桥 + GL4ES。",
                r.java_version, r.elapsed_ms
            );
        } else {
            r.verdict = format!(
                "VM 建成但属性读不出来：{}。JNIEnv 或类加载可能有问题。",
                r.java_version
            );
        }

        // 保持 vm 存活：不调 DestroyJavaVM（会阻塞到所有非 daemon 线程退出）
        std::mem::forget(vm);
        r
    }

    pub fn get_cached() -> Option<EmbeddedReport> {
        RESULT.get().cloned()
    }

    pub fn put_cache(r: EmbeddedReport) {
        let _ = RESULT.set(r);
    }
}

/// 执行内嵌 JVM 实验。
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
        if let Some(mut prev) = imp::get_cached() {
            prev.cached = true;
            return Ok(prev);
        }

        let javas = crate::java::scan_javas(app);
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

        let report = tokio::task::spawn_blocking(move || imp::create_vm(&jre_home))
            .await
            .map_err(|e| format!("实验线程异常: {e}"))?;

        // 只有真正建成 VM（或已经存在）才缓存；失败的可以换个 JRE 再试
        if report.vm_ok || report.create_rc == -5 {
            imp::put_cache(report.clone());
        }
        Ok(report)
    }
}
