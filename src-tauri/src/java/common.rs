//! Java 运行时管理 —— 公共部分（桌面 / Android 共用）
//!
//! 平台差异全部下沉到 `desktop.rs` / `android.rs`：
//! - 桌面（Windows）：从 Adoptium 下载官方 x64 JDK，扫描本机已装 JDK
//! - Android：从 EclipseJRE 下载 **Android 交叉编译** 的 JRE（Pojav 格式）
//!
//! 两者提供的 tauri command 名称与签名完全一致，前端无需分支。

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct JavaInfo {
    pub path: String,
    /// 主版本号, 如 8 / 17 / 21
    pub major: u32,
    /// 完整版本字符串, 如 "21.0.5+11"
    pub version: String,
    pub vendor: String,
    /// 是否为启动器下载托管的
    pub managed: bool,
}

/// MC 版本 → 需要的 Java 主版本
pub fn required_java_major(mc_version: &str) -> u32 {
    let (major, minor, patch) = parse_mc_version(mc_version);
    // 26.x 起改用年份式版本号，统一要求 Java 25
    if major >= 26 {
        return 25;
    }
    match (major, minor, patch) {
        (1, minor, patch) if minor >= 21 || (minor == 20 && patch >= 5) => 21,
        (1, minor, _) if minor >= 18 => 17,
        (1, 17, _) => 16,
        _ => 8,
    }
}

/// "1.21.11" → (1, 21, 11)
pub fn parse_mc_version(v: &str) -> (u32, u32, u32) {
    let nums: Vec<u32> = v
        .split('.')
        .map(|s| s.parse::<u32>().unwrap_or(0))
        .collect();
    (
        nums.first().copied().unwrap_or(1),
        nums.get(1).copied().unwrap_or(0),
        nums.get(2).copied().unwrap_or(0),
    )
}

/// 版本号字符串 → 主版本号。"1.8.0_431" → 8，"17.0.21" → 17
pub fn parse_major(ver: &str) -> Option<u32> {
    let ver = ver.trim_start_matches("1.").trim();
    let first = ver.split(['.', '+', '-', '_']).next()?;
    first.parse::<u32>().ok()
}

/// 按指定主版本要求选择本机 Java：满足 ≥ need 的最小者。
/// need 一般来自版本 JSON 的 javaVersion.majorVersion。
pub fn select_java(
    javas: &[JavaInfo],
    need: u32,
    label: &str,
) -> Result<JavaInfo, String> {
    let mut candidates: Vec<&JavaInfo> = javas.iter().filter(|j| j.major >= need).collect();
    candidates.sort_by(|a, b| a.major.cmp(&b.major));
    match candidates.first() {
        Some(j) => Ok((*j).clone()),
        None => Err(format!(
            "未找到 Java {need} 或更高版本（{label} 需要 Java {need}），请到「下载 → Java」安装"
        )),
    }
}
