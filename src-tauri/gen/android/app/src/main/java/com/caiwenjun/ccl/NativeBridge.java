package com.caiwenjun.ccl;

import android.view.Surface;

/**
 * ART 侧到 `libpojavexec` 的桥（M3）
 *
 * ## 为什么不能直接从 Rust 调
 *
 * `libpojavexec` 的 `JNI_OnLoad` 会用 `FindClass("org/lwjgl/glfw/CallbackBridge")`
 * 把回调类缓存成全局引用。按 JNI 规范，**原生线程**里 `FindClass` 走的是
 * 系统类加载器（在 App 里基本等于什么都找不到），只有从**声明该 native 方法的
 * Java 类**的帧进来，才会用那个类的类加载器 —— 也就是 App 自己的。
 *
 * 所以链路必须是：
 *
 * ```
 * Rust 线程 → 调 Java 的 setupWindow()
 *           → Java 调 setupWindowNative()（此时栈上有 App 类的帧）
 *           → Rust 在这个 JNI 函数里 dlopen + JNI_OnLoad + setupBridgeWindow
 * ```
 *
 * 顺带的好处是 `Surface` 直接用参数传，不用再想办法跨 VM 递对象。
 */
public final class NativeBridge {

    /**
     * 由 `libccl_lib.so` 实现（`Java_com_caiwenjun_ccl_NativeBridge_setupWindowNative`）。
     *
     * @param soPath  libpojavexec.so 的绝对路径
     * @param surface 游戏画面用的 Surface
     * @return `OK|<宽>x<高>` 或 `ERR:<原因>`
     */
    private static native String setupWindowNative(String soPath, Surface surface);

    /**
     * 等 Surface 就绪后交给 native 层，返回结果字符串。
     *
     * @param timeoutMs 等 Surface 的超时（SurfaceView 是异步创建的）
     */
    public static String setupWindow(String soPath, long timeoutMs) {
        Surface surface = GameSurface.awaitSurface(timeoutMs);
        if (surface == null) {
            return "ERR:等了 " + timeoutMs + " ms 也没等到 SurfaceView 的 Surface";
        }
        if (!surface.isValid()) {
            return "ERR:Surface 已失效（Activity 被回收？）";
        }
        return setupWindowNative(soPath, surface);
    }

    private NativeBridge() {
    }
}
