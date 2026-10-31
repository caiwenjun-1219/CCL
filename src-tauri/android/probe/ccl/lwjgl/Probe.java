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

        step("probe.env", () -> {
            String[] keys = {"POJAV_RENDERER", "POJAVEXEC_EGL", "LIBGL_ES", "FORCE_VSYNC"};
            StringBuilder b = new StringBuilder();
            for (String k : keys) {
                String v = System.getenv(k);
                if (b.length() > 0) {
                    b.append(' ');
                }
                b.append(k).append('=').append(v == null ? "<未设置>" : v);
            }
            return b.toString();
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

        // ---- 4. 真正建窗口（M3）----
        //
        // glfwInit 会进 pojavInit → ANativeWindow_acquire(全局 window)。window 为
        // NULL 时是**原生层空指针**，不是异常，直接把进程带走。所以只有 Rust
        // 侧确认 Surface 已经交给 pojavexec（Bridge.surfaceReady）才敢往下走。
        if (!Bridge.surfaceReady) {
            String why = Bridge.surfaceInfo == null || Bridge.surfaceInfo.isEmpty()
                    ? "Surface 未桥接"
                    : Bridge.surfaceInfo;
            OUT.append("render=skipped（").append(why).append("）\n");
            System.out.println("[probe] render=skipped: " + why);
            return OUT.toString();
        }

        // 这个属性决定 LWJGL 去加载哪个 GL 实现。Android 上必须是 LTW：
        // 它把 GL 3.x 调用翻译成 GLES 3.x，MC 1.17+ 才跑得起来。
        // （官方 libGL 在 bionic 上根本不存在）
        System.setProperty("org.lwjgl.opengl.libname", "libltw.so");

        step("glfw.init", () -> {
            Object ok = cls("org.lwjgl.glfw.GLFW").getMethod("glfwInit").invoke(null);
            return Boolean.TRUE.equals(ok) ? "true" : "false";
        });

        final long[] window = new long[1];
        step("glfw.window", () -> {
            Class<?> g = cls("org.lwjgl.glfw.GLFW");
            // 854x480 是 MC 的默认窗口尺寸；monitor / share 都给 0
            Object h = g.getMethod("glfwCreateWindow", int.class, int.class,
                    CharSequence.class, long.class, long.class)
                    .invoke(null, 854, 480, "CCL", 0L, 0L);
            window[0] = ((Number) h).longValue();
            if (window[0] == 0L) {
                return "句柄为 0";
            }
            g.getMethod("glfwShowWindow", long.class).invoke(null, window[0]);
            return "ok handle=0x" + Long.toHexString(window[0]);
        });

        step("glfw.current", () -> {
            if (window[0] == 0L) {
                return "跳过（没有窗口）";
            }
            cls("org.lwjgl.glfw.GLFW")
                    .getMethod("glfwMakeContextCurrent", long.class)
                    .invoke(null, window[0]);
            Object cur = cls("org.lwjgl.glfw.GLFW")
                    .getMethod("glfwGetCurrentContext").invoke(null);
            long c = ((Number) cur).longValue();
            return c == window[0]
                    ? "ok current=0x" + Long.toHexString(c)
                    : "不一致 current=0x" + Long.toHexString(c)
                            + " 期望=0x" + Long.toHexString(window[0]);
        });

        step("gl.fp", () -> {
            Class<?> gl = cls("org.lwjgl.opengl.GL");
            Object fp = gl.getMethod("getFunctionProvider").invoke(null);
            if (fp == null) {
                return "null（GL 库没加载）";
            }
            Class<?> fpi = cls("org.lwjgl.system.FunctionProvider");
            java.lang.reflect.Method getAddr =
                    fpi.getMethod("getFunctionAddress", CharSequence.class);
            StringBuilder b = new StringBuilder();
            b.append(fp.getClass().getName());
            for (String fn : new String[]{"glGetError", "glGetString", "glGetIntegerv"}) {
                Object a = getAddr.invoke(fp, fn);
                b.append(' ').append(fn).append("=0x")
                        .append(Long.toHexString(((Number) a).longValue()));
            }
            return b.toString();
        });

        step("gl.caps", () -> {
            Class<?> gl = cls("org.lwjgl.opengl.GL");
            Object caps = gl.getMethod("createCapabilities").invoke(null);
            if (caps == null) {
                return "createCapabilities 返回 null";
            }
            boolean gl32 = caps.getClass().getField("OpenGL32").getBoolean(caps);
            boolean gl33 = caps.getClass().getField("OpenGL33").getBoolean(caps);
            return "ok OpenGL32=" + gl32 + " OpenGL33=" + gl33;
        });

        // GL_VERSION / GL_RENDERER / GL_VENDOR / GL_SHADING_LANGUAGE_VERSION
        step("gl.version", () -> glString("org.lwjgl.opengl.GL11", 0x1F02));
        step("gl.renderer", () -> glString("org.lwjgl.opengl.GL11", 0x1F01)
                + " / " + glString("org.lwjgl.opengl.GL11", 0x1F00));
        step("gl.glsl", () -> glString("org.lwjgl.opengl.GL11", 0x8B8C));

        // 真画一帧：清屏 + 交换缓冲。能走完说明整条 EGL surface 链是通的，
        // 而不只是「上下文建起来了」。
        //
        // 必须查 glGetError：上下文没 current 时这些调用是「静默无操作」，
        // 上一版就是这样报了个假的 ok。
        step("gl.frame", () -> {
            if (window[0] == 0L) {
                return "跳过（没有窗口）";
            }
            Class<?> gl11 = cls("org.lwjgl.opengl.GL11");
            java.lang.reflect.Method getErr = gl11.getMethod("glGetError");

            gl11.getMethod("glViewport", int.class, int.class, int.class, int.class)
                    .invoke(null, 0, 0, 854, 480);
            int e1 = ((Number) getErr.invoke(null)).intValue();

            gl11.getMethod("glClearColor", float.class, float.class, float.class, float.class)
                    .invoke(null, 0.07f, 0.09f, 0.13f, 1.0f);
            int e2 = ((Number) getErr.invoke(null)).intValue();

            gl11.getMethod("glClear", int.class).invoke(null, 0x4000); // GL_COLOR_BUFFER_BIT
            int e3 = ((Number) getErr.invoke(null)).intValue();

            cls("org.lwjgl.glfw.GLFW")
                    .getMethod("glfwSwapBuffers", long.class)
                    .invoke(null, window[0]);

            StringBuilder b = new StringBuilder();
            b.append("viewport=0x").append(Integer.toHexString(e1))
                    .append(" clearColor=0x").append(Integer.toHexString(e2))
                    .append(" clear=0x").append(Integer.toHexString(e3));
            if (e1 == 0 && e2 == 0 && e3 == 0) {
                b.append(" 已清屏并交换缓冲");
            } else {
                b.append(" GL 报错，交换缓冲不能算成功");
            }
            return b.toString();
        });

        return OUT.toString();
    }

    /** `glGetString(int)`：返回值是 String，拿不到就报错字符串 */
    private static String glString(String clsName, int name) throws Throwable {
        Object v = cls(clsName).getMethod("glGetString", int.class).invoke(null, name);
        return v == null ? "<null>" : v.toString();
    }

    private Probe() {
    }
}
