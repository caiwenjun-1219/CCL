/**
 * 平台检测。
 * Tauri Android WebView 的 UA 含 "Android"；桌面 WebView / 浏览器无此字段。
 * 同步判断，避免渲染闪烁。
 */
const UA = typeof navigator !== "undefined" ? navigator.userAgent : "";

/** 是否运行在 Android（手机版）上 */
export const isMobile = (): boolean => /Android/i.test(UA);

/** 移动端启动游戏：Java 运行时已能在 Android 上跑通，尚缺 LWJGL 图形层 */
export const MOBILE_LAUNCH_HINT = "安卓端启动功能正在接入（Java 运行时已就绪），暂请使用桌面版";
