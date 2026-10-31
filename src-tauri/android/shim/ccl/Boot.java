package ccl;

import java.io.File;
import java.io.FileOutputStream;
import java.io.PrintStream;

/**
 * 内嵌 JVM 里的启动辅助。
 *
 * <p>存在的唯一理由：**Android 上内嵌 JVM 的 fd 1 / fd 2 是丢的**。
 * HotSpot 的 {@code System.out} 直接 write(1, ...)，而 App 进程的 stdout 既没接
 * logcat 也不是终端，所有 {@code printStackTrace()}、MC 的早期输出全部进了黑洞。
 * 排查 MC 启动时，这等于闭着眼睛修。
 *
 * <p>所以主类开跑之前先把它引到文件里。这个类放在 shim.jar（classpath 最前），
 * 保证任何情况下都能加载到。
 *
 * <p>设计约束：**任何异常都不许往外抛**。日志是排查手段，不能变成启动的前置条件。
 */
public final class Boot {

    private Boot() {
    }

    /**
     * 把 {@code System.out} / {@code System.err} 重定向到指定文件（追加写）。
     *
     * @return 空串表示成功，否则是失败原因（调用方打日志用，不抛异常）
     */
    public static String redirect(String logFile) {
        try {
            File f = new File(logFile);
            File dir = f.getParentFile();
            if (dir != null && !dir.exists()) {
                //noinspection ResultOfMethodCallIgnored
                dir.mkdirs();
            }
            PrintStream ps = new PrintStream(new FileOutputStream(f, true), true, "UTF-8");
            System.setOut(ps);
            System.setErr(ps);
            System.out.println("[CCL] stdout/stderr 已重定向到 " + f.getAbsolutePath());
            return "";
        } catch (Throwable t) {
            return t.toString();
        }
    }

    /**
     * 打印一行到「重定向后」的 stdout。
     *
     * <p>Rust 侧的 {@code println!} 走的是 Rust 的 stdout（那条有 logcat），
     * 这个走的是 JVM 的 stdout（那条到文件）。两条流分开，排查时对照着看。
     */
    public static void log(String msg) {
        System.out.println("[CCL] " + msg);
        System.out.flush();
    }
}
