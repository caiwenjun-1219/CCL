package org.lwjgl.glfw;

/**
 * Pojav lwjgl3 fork 的 CallbackBridge —— 兼容层实现。
 *
 * ## 为什么要自己写这个类
 *
 * 我们手上这套 natives（`libpojavexec.so`）是 **input_bridge_v3** 一代。
 * APK 自带的 `lwjgl-glfw-classes.jar` 里，`GLFW` 已经是 v3
 * （有 `internalWindowSizeChanged`、`keyDownBuffer`），但 `CallbackBridge`
 * 还停留在 v2 —— 只有 `nativeSendData(boolean,int,String)` 那套旧接口，
 * 而 v3 的 so 要的是 `nativeSendChar` / `nativeSendKey` / `nativeSendScroll` …
 *
 * 结果就是 `libpojavexec` 的 `JNI_OnLoad` 注册 native 方法时找不到对应声明，
 * 抛 `NoSuchMethodError`，整个 `System.loadLibrary("pojavexec")` 失败，
 * 后续 `GLFW.glfwInit()` 全盘皆输。
 *
 * ## 方法清单是怎么来的
 *
 * 不是猜的：解析 `libpojavexec.so` 的 `.rela.dyn` 重定位表，
 * 还原出 `RegisterNatives` 用的 `JNINativeMethod` 数组，正好 8 项，
 * 名字与签名见下面每个方法。GLFW 实际只调 `nativeSetInputReady` /
 * `nativeSetGrabbing` / `nativeClipboard`，其余是输入事件通路，
 * 建窗口阶段走不到，但必须先声明成 native 才能注册成功。
 *
 * ## classpath 顺序
 *
 * 本类打成 `shim.jar` 排在最前面，靠「第一个命中的赢」覆盖 fat jar 里那份 v2 实现。
 *
 * ---
 * 常量值与 v2 版保持一致（javap 反编译得出）。
 */
public class CallbackBridge {

    public static final int CLIPBOARD_COPY = 2000;
    public static final int CLIPBOARD_PASTE = 2001;

    public static final int EVENT_TYPE_CHAR = 1000;
    public static final int EVENT_TYPE_CHAR_MODS = 1001;
    public static final int EVENT_TYPE_CURSOR_ENTER = 1002;
    public static final int EVENT_TYPE_CURSOR_POS = 1003;
    public static final int EVENT_TYPE_FRAMEBUFFER_SIZE = 1004;
    public static final int EVENT_TYPE_KEY = 1005;
    public static final int EVENT_TYPE_MOUSE_BUTTON = 1006;
    public static final int EVENT_TYPE_SCROLL = 1007;
    public static final int EVENT_TYPE_WINDOW_SIZE = 1008;

    public static final int ANDROID_TYPE_GRAB_STATE = 0;

    public static final boolean INPUT_DEBUG_ENABLED =
            Boolean.parseBoolean(System.getProperty("glfwstub.debugInput", "false"));

    // ---- v3 的 RegisterNatives 表（8 项，签名与 so 内完全一致）----
    public static native boolean nativeSendChar(char codePoint);

    public static native boolean nativeSendCharMods(char codePoint, int mods);

    public static native void nativeSendCursorPos(float x, float y);

    public static native void nativeSendKey(int key, int scancode, int action, int mods);

    public static native void nativeSendMouseButton(int button, int action, int mods);

    public static native void nativeSendScreenSize(int width, int height);

    public static native void nativeSendScroll(double xoffset, double yoffset);

    public static native void nativeSetUseInputStackQueue(boolean use);

    // ---- 按名字规则解析的老接口（so 里有 Java_..._ 导出）----
    public static native boolean nativeSetInputReady(boolean ready);

    public static native String nativeClipboard(int action, byte[] data);

    public static native void nativeSetGrabbing(boolean grabbing);

    /**
     * v2 时代的统一事件入口。v3 的 so 不再导出 `nativeSendData`，
     * 保留声明只为兼容可能还在调用它的类；正常流程不会走到。
     */
    public static native void nativeSendData(boolean debug, int eventType, String data);

    // ---- v3 要求存在的 Java 侧回调（so 用 GetStaticMethodID 缓存）----

    /**
     * 原生侧读写剪贴板时回调进来。
     *
     * @param action  {@link #CLIPBOARD_COPY} / {@link #CLIPBOARD_PASTE}
     * @param current 当前剪贴板内容（COPY 时为新值）
     * @return 应视为「剪贴板现在的内容」
     */
    public static String accessAndroidClipboard(int action, String current) {
        if (INPUT_DEBUG_ENABLED) {
            System.out.println("[shim] accessAndroidClipboard action=" + action + " cur=" + current);
        }
        // 没有 Activity 句柄可转，原样返回：COPY 当作已写入，PASTE 当作没内容
        return current;
    }

    /** 鼠标锁定（grab）状态变化通知，用于显隐 UI 层。 */
    public static void onGrabStateChanged(boolean grabbed) {
        if (INPUT_DEBUG_ENABLED) {
            System.out.println("[shim] onGrabStateChanged " + grabbed);
        }
    }

    /**
     * 原生侧要求开启 / 关闭「直接输入」（手柄直通）。
     *
     * so 的 `JNI_OnLoad` 会用 `GetStaticMethodID(..., "onDirectInputEnable", "()V")`
     * 缓存这个方法，**方法不存在就会让 JNI_OnLoad 失败**（进而整个库加载失败）。
     * 我们现在没有手柄直通实现，留空即可。
     */
    public static void onDirectInputEnable() {
        if (INPUT_DEBUG_ENABLED) {
            System.out.println("[shim] onDirectInputEnable");
        }
    }

    public static void sendData(int eventType, String data) {
        nativeSendData(INPUT_DEBUG_ENABLED, eventType, data);
    }
}
