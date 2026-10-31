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

        // M4-a：真画，而且要**连着画**几秒。
        //
        // 只画一帧的话，画面在 SurfaceView 上闪一下就没了（EGL 的 buffer 会被
        // 回收），截图根本抓不到，也就没法证明「人眼能看见」。这里跑 8 秒动画，
        // 足够截到图。
        step("gl.demo", () -> {
            if (window[0] == 0L) {
                return "跳过（没有窗口）";
            }
            int[] wh = surfaceSize();
            return drawDemo(window[0], wh[0], wh[1]);
        });

        return OUT.toString();
    }

    // =====================================================================
    // M4-a：可见画面演示
    // =====================================================================

    private static final int GL_COLOR_BUFFER_BIT = 0x4000;
    private static final int GL_VERTEX_SHADER = 0x8B31;
    private static final int GL_FRAGMENT_SHADER = 0x8B30;
    private static final int GL_COMPILE_STATUS = 0x8B81;
    private static final int GL_LINK_STATUS = 0x8B82;
    private static final int GL_TRIANGLES = 0x0004;
    private static final int GL_SCISSOR_TEST = 0x0C11;

    /** 演示时长：留足截图时间，又不至于让调试等太久 */
    private static final long DEMO_MS = 8000;

    /**
     * 用 `gl_VertexID` 画三个顶点的三角形 —— 不需要任何 VBO / attribute。
     *
     * 选这条路是为了把「验证 shader 能不能编译链接」和「验证 VBO 通不通」
     * 拆开：M4-a 只关心前者。MC 的真渲染一定会用 attribute + VBO，
     * 但要等 M4-b 真正起游戏时再上，那时有完整的报错上下文。
     */
    private static final String VS_SRC =
            "#version 300 es\n"
                    + "uniform float uTime;\n"
                    + "out vec4 vColor;\n"
                    + "void main() {\n"
                    + "  vec3 c;\n"
                    + "  if (gl_VertexID == 0)      { gl_Position = vec4(-0.85, -0.65, 0.0, 1.0); c = vec3(1.0, 0.25, 0.25); }\n"
                    + "  else if (gl_VertexID == 1) { gl_Position = vec4( 0.85, -0.65, 0.0, 1.0); c = vec3(0.25, 1.0, 0.35); }\n"
                    + "  else                       { gl_Position = vec4( 0.0,   0.85, 0.0, 1.0); c = vec3(0.30, 0.55, 1.0); }\n"
                    + "  vColor = vec4(c * (0.55 + 0.45 * sin(uTime * 2.0)), 1.0);\n"
                    + "}\n";

    private static final String FS_SRC =
            "#version 300 es\n"
                    + "precision mediump float;\n"
                    + "in vec4 vColor;\n"
                    + "out vec4 fragColor;\n"
                    + "void main() { fragColor = vColor; }\n";

    /** 从 `Bridge.surfaceInfo`（形如 `1440x2560（2 ms）`）里取宽高 */
    private static int[] surfaceSize() {
        String s = Bridge.surfaceInfo;
        if (s != null) {
            java.util.regex.Matcher m =
                    java.util.regex.Pattern.compile("(\\d{3,5})x(\\d{3,5})").matcher(s);
            if (m.find()) {
                return new int[]{Integer.parseInt(m.group(1)), Integer.parseInt(m.group(2))};
            }
        }
        // 拿不到就退回 MC 的默认窗口尺寸，至少能看到东西
        return new int[]{854, 480};
    }

    /**
     * 连续渲染 {@link #DEMO_MS} 毫秒。
     *
     * 正常情况下走 shader（三角形脉动）；shader 编译/链接失败就退到
     * scissor 色带 —— 后者只用 GL1.1，几乎不可能失败，能保证「一定有画面」。
     */
    private static String drawDemo(long win, int w, int h) throws Throwable {
        Class<?> gl11 = cls("org.lwjgl.opengl.GL11");
        Class<?> gl20 = cls("org.lwjgl.opengl.GL20");
        Class<?> glfw = cls("org.lwjgl.glfw.GLFW");

        java.lang.reflect.Method glViewport =
                gl11.getMethod("glViewport", int.class, int.class, int.class, int.class);
        java.lang.reflect.Method glClearColor =
                gl11.getMethod("glClearColor", float.class, float.class, float.class, float.class);
        java.lang.reflect.Method glClear = gl11.getMethod("glClear", int.class);
        java.lang.reflect.Method glGetError = gl11.getMethod("glGetError");
        java.lang.reflect.Method glDrawArrays =
                gl11.getMethod("glDrawArrays", int.class, int.class, int.class);
        java.lang.reflect.Method glEnable = gl11.getMethod("glEnable", int.class);
        java.lang.reflect.Method glDisable = gl11.getMethod("glDisable", int.class);
        java.lang.reflect.Method glScissor =
                gl11.getMethod("glScissor", int.class, int.class, int.class, int.class);
        java.lang.reflect.Method swap = glfw.getMethod("glfwSwapBuffers", long.class);
        java.lang.reflect.Method glUseProgram = gl20.getMethod("glUseProgram", int.class);
        java.lang.reflect.Method glUniform1f =
                gl20.getMethod("glUniform1f", int.class, float.class);

        int prog = 0;
        String mode;
        String shaderNote = "";
        try {
            prog = buildTriangleProgram();
            mode = "shader 三角形";
        } catch (Throwable t) {
            mode = "scissor 色带（shader 不可用）";
            shaderNote = " shader=" + brief(t);
        }

        long started = System.nanoTime();
        long deadline = started + DEMO_MS * 1_000_000L;
        int frames = 0;
        int firstErr = 0;

        glViewport.invoke(null, 0, 0, w, h);

        while (System.nanoTime() < deadline) {
            float t = (System.nanoTime() - started) / 1.0e9f;

            // 背景：深蓝，随呼吸微微变亮，方便肉眼确认「这是活的画面」
            float bg = 0.10f + 0.06f * (float) Math.sin(t * 1.5);
            glClearColor.invoke(null, 0.04f, 0.06f, bg, 1.0f);
            glClear.invoke(null, GL_COLOR_BUFFER_BIT);

            if (prog != 0) {
                glUseProgram.invoke(null, prog);
                int loc = uniformLocation(prog, "uTime");
                if (loc >= 0) {
                    glUniform1f.invoke(null, loc, t);
                }
                glDrawArrays.invoke(null, GL_TRIANGLES, 0, 3);
            } else {
                // 24 条横带，色相整体滚动，看得出在动
                final int bands = 24;
                glEnable.invoke(null, GL_SCISSOR_TEST);
                for (int i = 0; i < bands; i++) {
                    int y = h * i / bands;
                    int bh = h / bands;
                    float[] rgb = hsv((t * 0.15f + (float) i / bands) % 1.0f);
                    glClearColor.invoke(null, rgb[0], rgb[1], rgb[2], 1.0f);
                    glScissor.invoke(null, 0, y, w, bh);
                    glClear.invoke(null, GL_COLOR_BUFFER_BIT);
                }
                glDisable.invoke(null, GL_SCISSOR_TEST);
            }

            swap.invoke(null, win);
            frames++;

            int e = ((Number) glGetError.invoke(null)).intValue();
            if (e != 0 && firstErr == 0) {
                firstErr = e;
            }
            Thread.sleep(16);
        }

        StringBuilder b = new StringBuilder();
        b.append("ok 帧数=").append(frames)
                .append(" 尺寸=").append(w).append('x').append(h)
                .append(" 模式=").append(mode);
        if (firstErr != 0) {
            b.append(" GL错误=0x").append(Integer.toHexString(firstErr));
        }
        b.append(shaderNote);

        // 稳定的最后一帧 + 读回像素。
        //
        // MuMu 这类模拟器把应用放在独立的虚拟 display 上，`screencap` 换 display
        // 只能拿到**冻结帧**（实测拿到的还是上一轮的画面），没法用它证明实时内容。
        // 直接从 GPU 读回像素就没这个问题：读到什么就是这一帧真画出来的什么。
        float tEnd = DEMO_MS / 1000.0f;
        glClearColor.invoke(null, 0.04f, 0.06f, 0.10f, 1.0f);
        glClear.invoke(null, GL_COLOR_BUFFER_BIT);
        if (prog != 0) {
            glUseProgram.invoke(null, prog);
            int loc = uniformLocation(prog, "uTime");
            if (loc >= 0) {
                glUniform1f.invoke(null, loc, tEnd);
            }
            glDrawArrays.invoke(null, GL_TRIANGLES, 0, 3);
        }
        String px = samplePixels(gl11, w, h);
        swap.invoke(null, win);

        b.append(" 像素[").append(px).append(']');
        return b.toString();
    }

    /**
     * 从当前 back buffer 读三个点的 RGB。
     *
     * 必须是 **swapBuffers 之前**：双缓冲交换后 back buffer 内容未定义。
     * 采样点按三角形的位置挑：中心与上中部在三角形内，左上角在背景里。
     */
    private static String samplePixels(Class<?> gl11, int w, int h) throws Throwable {
        java.lang.reflect.Method rp = gl11.getMethod("glReadPixels",
                int.class, int.class, int.class, int.class, int.class, int.class,
                java.nio.ByteBuffer.class);

        int[][] pts = {
                {w / 2, h / 2, '中'},
                {w / 2, (int) (h * 0.30), '上'},
                {5, 5, '角'},
        };
        StringBuilder b = new StringBuilder();
        for (int[] p : pts) {
            java.nio.ByteBuffer buf = java.nio.ByteBuffer.allocateDirect(4);
            rp.invoke(null, p[0], p[1], 1, 1, 0x1908 /*RGBA*/, 0x1401 /*UNSIGNED_BYTE*/, buf);
            if (b.length() > 0) {
                b.append(' ');
            }
            b.append((char) p[2]).append('(')
                    .append(buf.get(0) & 0xFF).append(',')
                    .append(buf.get(1) & 0xFF).append(',')
                    .append(buf.get(2) & 0xFF).append(')');
        }
        return b.toString();
    }

    /** 编译 + 链接，返回 program；任何一步失败都抛异常（消息里带 infoLog） */
    private static int buildTriangleProgram() throws Throwable {
        Class<?> gl20 = cls("org.lwjgl.opengl.GL20");
        int vs = compileShader(gl20, GL_VERTEX_SHADER, VS_SRC);
        int fs = compileShader(gl20, GL_FRAGMENT_SHADER, FS_SRC);

        int p = (Integer) gl20.getMethod("glCreateProgram").invoke(null);
        if (p == 0) {
            throw new IllegalStateException("glCreateProgram 返回 0");
        }
        gl20.getMethod("glAttachShader", int.class, int.class).invoke(null, p, vs);
        gl20.getMethod("glAttachShader", int.class, int.class).invoke(null, p, fs);
        gl20.getMethod("glLinkProgram", int.class).invoke(null, p);

        int ok = (Integer) gl20.getMethod("glGetProgrami", int.class, int.class)
                .invoke(null, p, GL_LINK_STATUS);
        if (ok == 0) {
            String log = (String) gl20.getMethod("glGetProgramInfoLog", int.class).invoke(null, p);
            throw new IllegalStateException("链接失败: " + log);
        }
        return p;
    }

    private static int compileShader(Class<?> gl20, int type, String src) throws Throwable {
        int s = (Integer) gl20.getMethod("glCreateShader", int.class).invoke(null, type);
        if (s == 0) {
            throw new IllegalStateException("glCreateShader(" + type + ") 返回 0（GL 没加载？）");
        }
        gl20.getMethod("glShaderSource", int.class, CharSequence.class).invoke(null, s, src);
        gl20.getMethod("glCompileShader", int.class).invoke(null, s);

        int ok = (Integer) gl20.getMethod("glGetShaderi", int.class, int.class)
                .invoke(null, s, GL_COMPILE_STATUS);
        if (ok == 0) {
            String log = (String) gl20.getMethod("glGetShaderInfoLog", int.class).invoke(null, s);
            throw new IllegalStateException(type + " 编译失败: " + log);
        }
        return s;
    }

    /**
     * `glGetUniformLocation(int, ByteBuffer)`。
     *
     * fork 版只有 `ByteBuffer` 重载（`CharSequence` 那个上游版本没编进来），
     * 所以自己拼一个 NUL 结尾的 direct buffer —— 不需要 MemoryUtil。
     */
    private static int uniformLocation(int prog, String name) throws Throwable {
        byte[] raw = name.getBytes("UTF-8");
        java.nio.ByteBuffer b = java.nio.ByteBuffer.allocateDirect(raw.length + 1);
        b.put(raw);
        b.put((byte) 0);
        b.flip();
        Class<?> gl20 = cls("org.lwjgl.opengl.GL20");
        return (Integer) gl20.getMethod("glGetUniformLocation", int.class, java.nio.ByteBuffer.class)
                .invoke(null, prog, b);
    }

    /** HSV(0..1) → RGB(0..1)，只用于色带配色 */
    private static float[] hsv(float hue) {
        float h = (hue - (float) Math.floor(hue)) * 6.0f;
        int i = (int) h;
        float f = h - i;
        float q = 1.0f - f;
        switch (i % 6) {
            case 0:  return new float[]{0.95f, f * 0.8f + 0.1f, 0.15f};
            case 1:  return new float[]{q * 0.8f + 0.1f, 0.95f, 0.15f};
            case 2:  return new float[]{0.15f, 0.95f, f * 0.8f + 0.1f};
            case 3:  return new float[]{0.15f, q * 0.8f + 0.1f, 0.95f};
            case 4:  return new float[]{f * 0.8f + 0.1f, 0.15f, 0.95f};
            default: return new float[]{0.95f, 0.15f, q * 0.8f + 0.1f};
        }
    }

    private static String brief(Throwable t) {
        Throwable c = t;
        StringBuilder b = new StringBuilder();
        int depth = 0;
        while (c != null && depth < 4) {
            if (b.length() > 0) {
                b.append(" <- ");
            }
            b.append(c.getClass().getSimpleName());
            if (c.getMessage() != null) {
                b.append(": ").append(c.getMessage().replace('\n', ' '));
            }
            c = c.getCause();
            depth++;
        }
        return b.toString();
    }

    /** `glGetString(int)`：返回值是 String，拿不到就报错字符串 */
    private static String glString(String clsName, int name) throws Throwable {
        Object v = cls(clsName).getMethod("glGetString", int.class).invoke(null, name);
        return v == null ? "<null>" : v.toString();
    }

    private Probe() {
    }
}
