package ccl.lwjgl;

/**
 * Rust 侧写、Java 侧读的一小块共享状态（M3 起）。
 *
 * 为什么不走 JVM 启动参数：`JNI_CreateJavaVM` 一个进程只能建一次，
 * 建完以后 `-D` 就改不了了。而「Surface 有没有桥接成功」是在建 VM
 * 之前多线程探测出来的，用静态字段传递比在参数里埋开关可靠 ——
 * 也不会出现「VM 是上一次建的、参数还是旧的」这种诡异情况。
 *
 * 这些字段由 {@code src-tauri/src/java/lwjgl.rs} 通过 `SetStaticField` 写入。
 */
public final class Bridge {

    /** Surface 是否已经交给 libpojavexec（决定探针敢不敢碰 glfwInit） */
    public static volatile boolean surfaceReady;

    /** Surface 桥接的一句话说明，出错时写进探针报告 */
    public static volatile String surfaceInfo = "";

    /**
     * Rust 侧统一从这个入口写状态。
     *
     * 用 setter 而不是直接 `SetStaticField`：`jni` crate 的静态字段 API 要先
     * 拿 `JStaticFieldID` 再写，比一次 `CallStaticVoidMethod` 啰嗦得多；
     * 而「按名字+签名调方法」这条路径在探针里已经跑通了。
     */
    public static void setSurface(boolean ready, String info) {
        surfaceReady = ready;
        surfaceInfo = info == null ? "" : info;
    }

    private Bridge() {
    }
}
