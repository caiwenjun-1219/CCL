/**
 * 内嵌 JVM 的最小测试类（`auto-embed` 模式跑的就是它）。
 *
 * 用途只有一个：在**不加载 pojavexec、不碰 GL** 的前提下，确认「ART 同进程里的
 * 内嵌 HotSpot 能不能正常建起来并执行字节码」。所以它必须尽量简单、不依赖别的类。
 *
 * ## 堆压力模式（验证 SIGABRT 是不是 GC 引起）
 *
 * 加 `-Dccl.heapTest=1`（走 `jvm-opts.txt`）后，它会不断分配 4MB 数组并周期性
 * `System.gc()`，把 `-Xmx` 撑满。用来回答那个关键问题：
 *
 * > `auto-embed` 90 秒不崩，是因为内嵌 HotSpot 真的稳定，
 * > 还是因为它几乎不分配内存、GC 根本没跑起来？
 *
 * 如果开了堆压力就崩，且栈还是 `VM Thread` + `free`，那 SIGABRT 就是
 * HotSpot 的 GC 在 Android 上踩了 scudo 的指针 tag，跟 pojavexec / GL 无关。
 *
 * 编译：`scripts/android-exp.sh build-tc`
 */
public class T {
    public static void main(String[] args) throws Exception {
        System.out.println("[T] started, java.version=" + System.getProperty("java.version")
                + " maxHeap=" + (Runtime.getRuntime().maxMemory() >> 20) + "MB");
        System.out.println("[T] pristine 段开始");

        // 基线：确认解释器 / JIT / 类加载都正常
        long acc = 0;
        for (int i = 0; i < 200000; i++) {
            acc += i ^ (acc >> 3);
        }
        System.out.println("[T] 算术 ok acc=" + acc);

        if (!"1".equals(System.getProperty("ccl.heapTest"))) {
            System.out.println("[T] 堆压力模式未开启（-Dccl.heapTest=1 可开）");
            System.out.println("[T] main 正常返回");
            return;
        }

        // 堆压力：4MB 一块，留引用防止被优化掉，每 8 块主动 GC 一次
        System.out.println("[T] 堆压力模式：开始分配");
        java.util.ArrayList<byte[]> keep = new java.util.ArrayList<byte[]>();
        int mb = 0;
        for (int i = 0; i < 4096; i++) {
            try {
                keep.add(new byte[4 << 20]);
            } catch (OutOfMemoryError e) {
                System.out.println("[T] OOM @" + mb + "MB，GC 后继续");
                keep.clear();
                System.gc();
                continue;
            }
            mb += 4;
            if (i % 8 == 7) {
                System.gc();
                System.out.println("[T] alloc=" + mb + "MB used="
                        + ((Runtime.getRuntime().totalMemory() - Runtime.getRuntime().freeMemory()) >> 20)
                        + "MB gcCount 已跑");
            }
        }
        System.out.println("[T] 堆压力循环结束，未崩溃。已分配总量 " + mb + "MB");
        System.out.println("[T] main 正常返回");
    }
}
