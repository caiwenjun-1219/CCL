package ccl.lwjgl;

/**
 * LWJGL 连通性探针（M2 里程碑）
 *
 * 这个类由内嵌 JVM 通过 JNI 反射调用（{@code ccl.lwjgl.Probe.run()}），
 * 目的只有一个：在一台真实 Android 设备上证明「Pojav 那套 LWJGL 移植
 * 能在我们自己的 JVM 里跑起来」。
 *
 * 三个层次，逐层加深：
 *   1. 加载 native —— libpojavexec / liblwjgl / liblwjgl_opengl
 *   2. 调进 lwjgl 核心 —— Version / MemoryUtil，证明 JNI 符号真的解析到了
 *   3. 触发 GLFW —— fork 版 GLFW 的 static 块会 loadNative("pojavexec")，
 *      这是「能不能建窗口」的前置条件
 *
 * 第 3 步在没有 Surface 的情况下**大概率拿不到 true**，这不是失败：
 * M2 只要求它「不是因为 UnsatisfiedLinkError 而失败」。
 *
 * 全程用反射而不是直接 import org.lwjgl.*，因为本文件要在没有 lwjgl 的
 * 环境下编译（只要 JDK），避免为了一个探针把整套依赖拖进构建。
 */
public final class Probe {

    private static final StringBuilder OUT = new StringBuilder();

    private interface Step {
        String run() throws Throwable;
    }

    private static void step(String key, Step s) {
        String line;
        try {
            String v = s.run();
            line = key + "=" + (v == null ? "<null>" : v);
        } catch (Throwable t) {
            StringBuilder b = new StringBuilder();
            b.append(key).append("=!").append(t.getClass().getName());
            String m = t.getMessage();
            if (m != null) {
                b.append(": ").append(m.replace('\n', ' '));
            }
            // ExceptionInInitializerError / UnsatisfiedLinkError 的真正原因在 cause 里
            Throwable c = t.getCause();
            int depth = 0;
            while (c != null && depth < 3) {
                b.append(" <- ").append(c.getClass().getName());
                if (c.getMessage() != null) {
                    b.append(": ").append(c.getMessage().replace('\n', ' '));
                }
                c = c.getCause();
                depth++;
            }
            line = b.toString();
        }
        OUT.append(line).append('\n');
        // 逐步即时输出：后面某步若把进程带崩，logcat 里至少留下前面几步的结论
        System.out.println("[probe] " + line);
        System.out.flush();
    }

    /** 反射建实例，避免编译期依赖 */
    private static Class<?> cls(String name) throws ClassNotFoundException {
        return Class.forName(name, true, Probe.class.getClassLoader());
    }

    public static String run() {
        OUT.setLength(0);

        step("probe.java", () -> System.getProperty("java.version", "?"));
        step("probe.arch", () -> System.getProperty("os.arch", "?"));
        step("probe.library.path", () -> System.getProperty("java.library.path", "<未设置>"));
        step("probe.class.path", () -> {
            String cp = System.getProperty("java.class.path", "");
            // classpath 可能很长，只报条目数与首尾
            String[] parts = cp.split(":");
            if (parts.length <= 4) {
                return cp;
            }
            return parts.length + " 项, 首=" + parts[0] + ", 尾=" + parts[parts.length - 1];
        });

        // ---- 1. native 加载 ----
        step("native.pojavexec", () -> {
            System.loadLibrary("pojavexec");
            return "ok";
        });
        step("native.lwjgl", () -> {
            System.loadLibrary("lwjgl");
            return "ok";
        });
        step("native.lwjgl_opengl", () -> {
            System.loadLibrary("lwjgl_opengl");
            return "ok";
        });

        // ---- 2. lwjgl 核心 ----
        step("lwjgl.version", () -> (String) cls("org.lwjgl.Version").getMethod("getVersion").invoke(null));
        step("lwjgl.memalloc", () -> {
            // 这个 fork 的 MemoryUtil 只有 nmemAlloc(long)/nmemFree(long)，
            // 没有上游那个 memAlloc(long)（3.3.3 之后被拆掉了）
            Class<?> mu = cls("org.lwjgl.system.MemoryUtil");
            long addr = (Long) mu.getMethod("nmemAlloc", long.class).invoke(null, 64L);
            if (addr == 0L) {
                return "地址为 0";
            }
            mu.getMethod("nmemFree", long.class).invoke(null, addr);
            return "ok addr=0x" + Long.toHexString(addr);
        });

        // ---- 3. GLFW（fork 版）----
        step("glfw.class", () -> {
            cls("org.lwjgl.glfw.GLFW");
            return "ok";
        });
        step("glfw.version", () -> (String) cls("org.lwjgl.glfw.GLFW")
                .getMethod("glfwGetVersionString").invoke(null));
        // glfwInit 会去拿 ANativeWindow 和当前 Activity 的 Surface，
        // 没接上时是**原生层空指针**（不是异常），直接把进程带走。
        // 所以默认不碰，要用 -Dccl.probe.glfwInit=true 显式打开（M3 联调用）。
        if (Boolean.getBoolean("ccl.probe.glfwInit")) {
            step("glfw.init", () -> {
                Class<?> g = cls("org.lwjgl.glfw.GLFW");
                Object ok = g.getMethod("glfwInit").invoke(null);
                return Boolean.TRUE.equals(ok) ? "true" : "false";
            });
        } else {
            OUT.append("glfw.init=skipped（需要 Surface，M3 再接）\n");
            System.out.println("[probe] glfw.init=skipped");
        }

        return OUT.toString();
    }

    private Probe() {
    }
}
