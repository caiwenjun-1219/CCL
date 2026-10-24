/**
 * 平台检测。
 * Tauri Android WebView 的 UA 含 "Android"；桌面 WebView / 浏览器无此字段。
 * 同步判断，避免渲染闪烁。
 */
const UA = typeof navigator !== "undefined" ? navigator.userAgent : "";

/** 是否运行在 Android（手机版）上 */
export const isMobile = (): boolean => /Android/i.test(UA);

/** 移动端不支持启动游戏（PC Java 版 Minecraft 无官方 Android 客户端） */
export const MOBILE_LAUNCH_HINT = "移动端暂不支持启动游戏，请使用桌面版 CCL";
