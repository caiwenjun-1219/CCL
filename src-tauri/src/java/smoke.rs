//! JVM 冒烟自检
//!
//! 目的：在动图形层（LWJGL → GL4ES）之前，先用最小代价确认「这台设备上到底
//! 能不能把打包进来的 Android JRE 跑起来」。两条路线要分开验证：
//!
//! - **路线 A：exec**（`bin/java -version`）—— 桌面一直这么做。但 Android 10
//!   (API 29) 起 SELinux 收走了 `app_data_file` 的 execute 权限，targetSdk >= 29
//!   的应用 exec 私有目录里的二进制会被拒（EACCES）。若这里失败，
//!   `launcher::launch_instance` 里那句 `Command::new(java)` 在 Android 上
//!   永远起不来，必须换成路线 B。
//! - **路线 B：dlopen**（把 `lib/server/libjvm.so` 映射进本进程）+ 后续
//!   `JNI_CreateJavaVM` —— Pojav / FCL 实际走的路。dlopen 不受 execve 限制，
//!   先验证能否映射成功。
//!
//! 自检结果直接回给前端展示，避免每次都靠 adb logcat 猜。

use std::io::Read;
use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::AppHandle;

/// 一次自检的完整结论
#[derive(Serialize)]
pub struct SmokeReport {
    /// 设备 ABI，如 arm64-v8a / x86_64
    pub device_arch: String,
    /// JRE 根目录
    pub java_dir: String,
    /// bin/java 绝对路径
    pub java_bin: String,
    pub bin_exists: bool,
    /// 可执行位是否已置（Android 上解包后要显式 chmod）
    pub bin_exec_bit: bool,
    /// 路线 A：exec 是否成功
    pub exec_ok: bool,
    /// exec 的退出码（成功时）
    pub exec_code: Option<i32>,
    /// exec 的原始 errno（失败时）
    pub exec_errno: Option<i32>,
    pub exec_error: Option<String>,
    /// `-version` 输出（stdout + stderr 合并）
    pub exec_output: String,
    /// libjvm.so 绝对路径
    pub libjvm_path: String,
    pub libjvm_exists: bool,
    /// 路线 B：dlopen 是否成功
    pub dlopen_ok: bool,
    /// dlopen 失败时的 dlerror 原文
    pub dlopen_error: String,
    /// 给人看的结论
    pub verdict: String,
}

fn device_arch() -> &'static str {
    match std::env::consts::ARCH {
        "aarch64" => "arm64",
        "arm" => "arm",
        "x86_64" => "x86_64",
        "x86" => "x86",
        _ => "arm64",
    }
}

/// 跑一次子进程并收集输出（带超时，避免 JVM 卡住把界面拖死）
fn run_proc(bin: &Path, args: &[&str], timeout_ms: u64) -> Result<(i32, String), (Option<i32>, String)> {
    use std::process::{Command, Stdio};

    let mut cmd = Command::new(bin);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    // JRE 的 `bin/java` 是动态依赖 `lib/libjli.so` 的 ELF，Android 的 linker 不会像
    // Linux 那样按 rpath 搜索 JRE 自己的 lib 目录，必须显式给 LD_LIBRARY_PATH。
    // 不给的话 exec 会“成功”但进程立刻退出，报
    // `CANNOT LINK EXECUTABLE ... library "libjli.so" not found`。
    #[cfg(target_os = "android")]
    if let Some(root) = bin.parent().and_then(|p| p.parent()) {
        let lib = root.join("lib");
        let server = lib.join("server");
        let ld = format!("{}:{}", lib.display(), server.display());
        cmd.env("LD_LIBRARY_PATH", ld);
    }

    let mut child = cmd
        .spawn()
        .map_err(|e| (e.raw_os_error(), format!("{e}")))?;

    let mut o = child.stdout.take().expect("stdout piped");
    let mut e = child.stderr.take().expect("stderr piped");
    let ho = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = o.read_to_string(&mut s);
        s
    });
    let he = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = e.read_to_string(&mut s);
        s
    });

    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
    let code = loop {
        match child.try_wait() {
            Ok(Some(st)) => break st.code().unwrap_or(-1),
            Ok(None) => {}
            Err(err) => return Err((err.raw_os_error(), format!("{err}"))),
        }
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err((None, format!("{timeout_ms}ms 内没退出，已强制结束")));
        }
        std::thread::sleep(std::time::Duration::from_millis(60));
    };

    // `java -version` 往 stderr 打，放前面更符合直觉
    let mut text = he.join().unwrap_or_default();
    let out = ho.join().unwrap_or_default();
    if !out.is_empty() {
        if !text.is_empty() {
            text.push('\n');
        }
        text.push_str(&out);
    }
    Ok((code, text))
}

/// 路线 B：直接 dlopen 目标 so，失败返回 dlerror 原文
#[cfg(target_os = "android")]
fn try_dlopen(path: &Path) -> Result<(), String> {
    use std::ffi::{CStr, CString};
    use std::os::raw::{c_char, c_int, c_void};

    extern "C" {
        fn dlopen(filename: *const c_char, flag: c_int) -> *mut c_void;
        fn dlerror() -> *mut c_char;
    }
    const RTLD_NOW: c_int = 2;

    let c = CString::new(path.to_string_lossy().as_bytes()).map_err(|_| "路径含 NUL 字节".to_string())?;
    unsafe {
        // 清掉历史错误，否则 dlerror 会返回上一次的陈旧消息
        let _ = dlerror();
        let h = dlopen(c.as_ptr(), RTLD_NOW);
        if h.is_null() {
            let e = dlerror();
            if e.is_null() {
                return Err("dlopen 返回 NULL 但没有 dlerror 信息".into());
            }
            return Err(CStr::from_ptr(e).to_string_lossy().to_string());
        }
    }
    Ok(())
}

#[cfg(not(target_os = "android"))]
fn try_dlopen(_path: &Path) -> Result<(), String> {
    Err("仅 Android 支持".into())
}

/// 执行 JVM 自检
pub async fn run(app: AppHandle) -> Result<SmokeReport, String> {
    let info = super::platform::scan_javas(app.clone())
        .into_iter()
        .max_by_key(|j| j.major)
        .ok_or_else(|| "没有已安装的 Java，先去 Java 页装一个（内置 17 可直接装）".to_string())?;

    let bin = PathBuf::from(&info.path);
    let dir = bin
        .parent()
        .and_then(|p| p.parent())
        .map(|p| p.to_path_buf())
        .ok_or_else(|| format!("无法从 {} 推出 JRE 根目录", info.path))?;

    let bin_exists = bin.exists();
    #[cfg(unix)]
    let bin_exec_bit = std::fs::metadata(&bin)
        .map(|m| {
            use std::os::unix::fs::PermissionsExt;
            m.permissions().mode() & 0o111 != 0
        })
        .unwrap_or(false);
    #[cfg(not(unix))]
    let bin_exec_bit = true;

    let libjvm = dir.join("lib").join("server").join("libjvm.so");
    let libjvm_exists = libjvm.exists();

    // 路线 A：exec java -version
    let bin_for_proc = bin.clone();
    let (exec_ok, exec_code, exec_errno, exec_error, exec_output) =
        tokio::task::spawn_blocking(move || match run_proc(&bin_for_proc, &["-version"], 15_000) {
            Ok((code, text)) => (true, Some(code), None, None, text),
            Err((errno, msg)) => (false, None, errno, Some(msg), String::new()),
        })
        .await
        .map_err(|e| format!("自检线程异常: {e}"))?;

    // 路线 B：dlopen libjvm.so（A 失败时才是关键，但无论如何都跑，结论更完整）
    let libjvm_for_dl = libjvm.clone();
    let (dlopen_ok, dlopen_error) = tokio::task::spawn_blocking(move || {
        if !libjvm_for_dl.exists() {
            return (false, "libjvm.so 不存在".to_string());
        }
        match try_dlopen(&libjvm_for_dl) {
            Ok(()) => (true, String::new()),
            Err(e) => (false, e),
        }
    })
    .await
    .map_err(|e| format!("自检线程异常: {e}"))?;

    let verdict = if exec_ok && exec_output.contains("version") {
        "exec 可用：可以沿用现有 Command::new(java) 的启动方式，接下来只需接图形层。".to_string()
    } else if exec_ok {
        "exec 进程起来了但输出异常，把下面的输出贴出来看看。".to_string()
    } else if dlopen_ok {
        format!(
            "exec 被系统拒绝（errno {:?}：{}），但 libjvm.so 可以 dlopen —— 符合预期，必须改走 JNI_CreateJavaVM 内嵌 JVM 路线（Pojav/FCL 的做法）。",
            exec_errno,
            exec_error.clone().unwrap_or_default()
        )
    } else {
        format!(
            "两条路都不通。exec: {:?} / dlopen: {}。先确认内置 JRE 的架构与设备 ABI（{}）是否匹配。",
            exec_error.clone().unwrap_or_else(|| "未知".into()),
            dlopen_error,
            device_arch()
        )
    };

    Ok(SmokeReport {
        device_arch: device_arch().to_string(),
        java_dir: dir.to_string_lossy().to_string(),
        java_bin: bin.to_string_lossy().to_string(),
        bin_exists,
        bin_exec_bit,
        exec_ok,
        exec_code,
        exec_errno,
        exec_error,
        exec_output,
        libjvm_path: libjvm.to_string_lossy().to_string(),
        libjvm_exists,
        dlopen_ok,
        dlopen_error,
        verdict,
    })
}
