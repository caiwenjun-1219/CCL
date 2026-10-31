# CCL 安卓端启动 Minecraft 技术调研

调研日期：2026-10-30
调研对象：PojavLauncher（Gladiolus 版，APK 实测）、PojavLauncherTeam/lwjgl3、PojavLauncherTeam/LTW
结论用途：判断"能否复用 Pojav 那套二进制，让 CCL 在安卓上真的启动游戏"

---

## 一、结论摘要

| 问题 | 结论 |
|---|---|
| 二进制能否复用 | **能**。版本、ABI 全部对得上，实测 APK 里拿到了完整清单 |
| 现有启动架构能否复用 | **不能**。`Command::new(java)` 子进程拿不到 Android Surface，必须改成内嵌 JVM |
| 工作量属于哪一档 | **不是"接一个库"，是"移植一套运行时"**。核心链路 1~2 周，加上输入/音频/持久化 3~4 周 |
| 许可证是否卡住 | 不卡。LGPL-3.0（Pojav/LTW）+ BSD-3（LWJGL）+ MIT（GL4ES），CCL 可保持自有许可 |
| 最大风险 | 无 arm64 真机可测；模拟器有 SIGILL，只能验证到"进程起来"，验证不了画面 |

**一句话**：路是通的，而且 `1.21.1 = LWJGL 3.3.3` 正好和 Pojav 的 fork 是同版本，这是最有利的地方；代价是 CCL 必须在安卓侧重建一套"内嵌 JVM + EGL 桥 + 输入桥"，不能沿用现在的子进程模型。

---

## 二、为什么必须改架构

### 2.1 子进程拿不到 Surface

Minecraft 桌面版通过 `LWJGL → GLFW → EGL/WGL` 打开窗口。Android 上没有 GLFW，需要把 GLFW 的"创建窗口"换成"拿到一个 `ANativeWindow`（来自 SurfaceView）再用 EGL 建 surface"。

`ANativeWindow` 是一个进程内的 native 指针，只能由 `ANativeWindow_fromSurface(JNIEnv*, jobject surface)` 得到，而 `Surface` 是 Binder Parcelable —— 要跨进程传递，接收方必须有 ART/Binder 环境去解析 Parcel。

CCL 现在 `Command::new(java)` 起的是普通 Linux 进程（exec 出来的 `bin/java`），它没有 ART、没有 Binder，**拿不到 Surface，也就没有画面**。

### 2.2 三条绕不过去的死路

| 想法 | 为什么不行 |
|---|---|
| 子进程 + 离屏 pbuffer，像素回传 | 1080p 每帧 ~8MB，走 socket 传，帧率撑不住，且延迟巨大 |
| 子进程 + 硬下 `natives-linux` 的 lwjgl | 那些 so 依赖 glibc 的 `libGL.so`/`libX11.so`，Android bionic 上加载即失败 |
| 子进程 + 自己用 libbinder 收 Surface | 技术上能写，但要自建 service、权限校验、Parcel 解析，比内嵌 JVM 还复杂 |

### 2.3 唯一现实路径

在 app 进程内 `dlopen libjvm.so` → `JNI_CreateJavaVM` → 反射调用 `net.minecraft.client.main.Main.main()`。这**正是 Pojav 做的**：它调的是 `com.oracle.dalvik.VMLauncher.launchJVM(argv)`，native 实现在 `jni/jre_launcher.c`。

好消息：上一轮已经在模拟器上验证过 `dlopen libjvm.so` 成功，所以内嵌 JVM 的前提条件已具备。

---

## 三、Pojav 到底由哪几块组成（实测拆解）

把 Gladiolus APK 解包后，`lib/arm64-v8a/` 的真实内容：

| 文件 | 大小 | 作用 | CCL 是否需要 |
|---|---|---|---|
| `libpojavexec.so` | 71 KB | **核心**：EGL 桥、输入桥、JVM hook、`jre_launcher` | 必需 |
| `libexithook.so` | 7.4 KB | 拦 `System.exit`，否则游戏一退 App 也死 | 必需 |
| `liblinkerhook.so` | 5.2 KB | 绕过 linker namespace 限制 | 部分机型必需 |
| `libbytehook.so` | 63 KB | hook 框架，`exithook` 依赖它 | 必需 |
| `liblwjgl.so` | 495 KB | LWJGL 核心 | 必需 |
| `liblwjgl_opengl.so` | 360 KB | OpenGL 绑定（被 hook 重定向到 GL4ES/LTW） | 必需 |
| `liblwjgl_stb.so` | 347 KB | 贴图解码 | 必需 |
| `liblwjgl_tinyfd.so` | 82 KB | 文件对话框 | 可选 |
| `libopenal.so` | 1.45 MB | 音频 | 必需 |
| `libfreetype.so` | 706 KB | 字体渲染 | 必需 |
| `libgl4es_114.so` | 2.6 MB | **GL → GLES 转译**（渲染器 A） | 二选一 |
| `libltw.so` | 1.6 MB | LTW 渲染器（渲染器 B，性能更好） | 二选一 |
| `libjnidispatch.so` | 157 KB | JNA | MC 依赖 |
| `libOSMesa.so` | 12.8 MB | 软件渲染兜底 | 不需要 |
| `libvulkan_freedreno.so` | 7.7 MB | Zink（仅高通 Adreno） | 不需要 |

**核心增量 ≈ 9~10 MB**（不含 OSMesa 和 freedreno）。

注意：**没有 `liblwjgl_glfw.so`**。GLFW 被换成了纯 Java 的 stub，见下。

### 3.1 lwjgl 只替换了一个 jar

`assets/components/lwjgl3/` 只有一个文件：

```
lwjgl-glfw-classes.jar   2.9 MB
version                  (git commit hash)
```

也就是说：**其余 lwjgl jar 全部沿用官方 3.3.3**，只有 `lwjgl-glfw` 被这个定制 jar 顶掉（纯 Java 实现，内部走 `pojavexec` 的 native，读 `-Dglfwstub.*` 属性）。

这比"整套 lwjgl 都要重编"轻得多。

### 3.2 native 骨架（`jni/Android.mk` 原文）

```
pojavexec:
  egl_bridge.c  ctxbridges/{gl_bridge,osm_bridge,egl_loader,osmesa_loader,swap_interval_no_egl}.c
  jvm_hooks/{java_exec_hooks, lwjgl_dlopen_hook, emui_iterator_fix_hook}.c
  input_bridge_v3.c  jre_launcher.c  stdio_is.c  driver_helper/nsbypass.c

exithook:      native_hooks/{exit_hook, chmod_hook}.c
linkerhook:    driver_helper/hook.c
pojavexec_awt: awt_bridge.c
```

其中 `lwjgl_dlopen_hook.c` 是渲染能跑的关键 —— 它把 lwjgl 对 `libGL.so` 的 `dlopen` 重定向到 GL4ES/LTW。

### 3.3 启动参数（`JREUtils.java` 原文摘录）

Pojav 给 MC 的 JVM 参数里，和图形直接相关的这些：

```java
-Dos.name=Linux
-Dos.version=Android-<版本>
-Djava.library.path=<natives>:<nativeLibraryDir>
-Djna.boot.library.path=<natives>
-Dorg.lwjgl.opengl.libname=<渲染库全路径>     // 关键：指定 GL 实现
-Dorg.lwjgl.freetype.libname=<...>/libfreetype.so
-Dorg.lwjgl.vulkan.libname=libvulkan.so
-Dglfwstub.windowWidth=<宽>
-Dglfwstub.windowHeight=<高>
-Dglfwstub.initEgl=false
-XX:ActiveProcessorCount=<核数>
```

另外它会 `purgeArg("-Xint")`（主动删掉解释模式参数），并把 `LD_LIBRARY_PATH` 设为
`<jre>/lib/server : <jre>/lib : /system/lib64 : /vendor/lib64 : <nativeLibraryDir>`。

### 3.4 JRE

APK 里打了三套：`jre`(8) / `jre-new`(17) / `jre-21`，每套 = `universal.tar.xz` + `bin-<arch>.tar.xz`。

**这和 CCL 现在用的 EclipseJRE 分包结构完全一致**（`jre21-universal.tar.xz` + `jre21-bin-arm64.tar.xz`），说明 JRE 这一层我们已经在正确的路上，不需要改。

---

## 四、版本兼容性核对

| 项目 | CCL 现状 | Pojav | 是否匹配 |
|---|---|---|---|
| MC 1.21.1 的 LWJGL | 3.3.3（实测设备上 `1.21.1.json`） | fork 分支 `wip/rebase_3.3.3` | ✅ 完全一致 |
| ABI | arm64-v8a | arm64-v8a | ✅ |
| JRE 21 分包 | universal + bin-arm64 | 同结构 | ✅ |

老版本（≤1.12.2）Pojav 用 `lwjglx` 兼容层解决，fork 里已内置，暂不影响 1.21.1。

---

## 五、三条路线对比

### 路线 A：移植 Pojav 运行时（推荐）

把 `pojavexec` 那套 so + `lwjgl-glfw-classes.jar` 搬进 CCL，Kotlin 侧重建启动器。

**要做的事**
1. `jniLibs/arm64-v8a/` 放 10 个 so（约 9 MB）
2. `assets/` 放 `lwjgl-glfw-classes.jar`，首次启动解压到 `<game>/lwjgl3/`
3. `launcher.rs` 的 classpath 末尾追加 `lwjgl3/*.jar`；natives 指向 so 目录
4. Android 分支的 `launch_instance` 改为转发给 Kotlin 插件
5. Kotlin 新建 `MinecraftLauncher`：`SurfaceView` + `VMLauncher` 内嵌 JVM + 环境变量
6. 触屏/键鼠输入映射（`pojavexec` 的 `input_bridge_v3` 提供 native 侧，Java 侧要接 `CallbackBridge`）
7. 渲染器选择（默认 GL4ES，可选 LTW）

**风险**：第 6 步是真正的工作量黑洞 —— Pojav 的触摸手势、虚拟按键、鼠标映射是一大坨 Java 代码。

### 路线 B：自研 EGL 桥

自己写 `libGL.so` 的替代实现 + 自己编 lwjgl native。
**结论：不推荐**。等于把 lwjgl3 fork + GL4ES 重做一遍，投入产出比极差。

### 路线 C：只做管理，不做启动

保留下载/实例/Java 管理，启动时提示"安卓启动请用 FCL/Pojav"。
**成本**：半天。**缺点**：产品缺口一直在。

### 建议

走 A，但**分里程碑验收**，每个里程碑都是可独立回退的：

| 里程碑 | 内容 | 验收标准 | 预估 |
|---|---|---|---|
| M1 | 内嵌 JVM 自检 | 在 app 进程内 `JNI_CreateJavaVM` 成功，打印 `java.version` | 0.5 天 |
| M2 | 加载 MC 主类 | `Main.main()` 被调用，日志走到 LWJGL 初始化 | 1~2 天 |
| M3 | 出画面 | SurfaceView 上出现 MC 主菜单 | 2~3 天 |
| M4 | 可操作 | 触屏能点、能进世界 | 3~5 天 |
| M5 | 可用 | 音频、设置持久化、崩溃兜底 | 2~3 天 |

M1 失败的话整条路要重估，所以必须先做 M1。

---

## 六、许可证

| 组件 | 许可 | 对 CCL 的影响 |
|---|---|---|
| PojavLauncher（含 `pojavexec`、glfw stub） | LGPL-3.0 | 动态链接，CCL 可保持自有许可；需附源码与许可声明，且允许替换该库 |
| lwjgl3 fork | BSD-3-Clause | 几乎无约束，保留版权声明即可 |
| LTW 渲染器 | LGPL-3.0 | 同 Pojav |
| GL4ES | MIT | 无约束 |
| OpenJDK (JRE) | GPLv2 + Classpath Exception | 作为独立运行时打包，合规 |

**注意**：QuestCraft 里的 "LightThinWrapper" 用的是 PolyForm Shield（禁止竞争性使用），那是**另一个项目**，不要和 PojavLauncherTeam/LTW 搞混 —— 后者的 LICENSE 文件明确是 LGPL-3.0。

实践上要做的事：APK 里放 `open_source_licenses.txt`，列出上述组件 + 源码地址。LGPL 要求"可替换"，Android 上通常通过在 APK 内保留 `.so` 独立文件（而非静态合并）满足。

---

## 七、剩余不确定项

| 项 | 状态 | 怎么消除 |
|---|---|---|
| 真机 SELinux 是否拦 exec | 未知（模拟器是 permissive） | 内嵌 JVM 后不再依赖 exec，此风险自动消失 |
| 内嵌 JVM 在 CCL 进程内是否稳定 | 未验证 | M1 实验 |
| 模拟器 SIGILL | 已知（C2 JIT 编出不支持的指令） | 只在模拟器出现，arm64 真机不会有；调试时加 `-Xint` |
| Tauri 的 Activity 叠 SurfaceView | 可行性高（`MainActivity` 是自己的类） | M3 时验证 |
| 性能（GL4ES on 中端机） | 未知 | 真机实测；不行换 LTW |

---

## 八、马上可以做的 M1 实验

最小验证：在 CCL 的 Android 插件里加一条命令，`dlopen` 内置 JRE 的 `libjvm.so` 并 `JNI_CreateJavaVM`，成功后反射调 `System.getProperty("java.version")`，把结果回传前端。

已有基础：
- `java/smoke.rs` 已验证 `dlopen libjvm.so` 成功
- `StoragePermissionPlugin.kt` 已有现成的插件通道可以加命令

如果 M1 通过，再决定是否把 `pojavexec` 那套搬进来。

---

# 实施记录

## M1 —— 内嵌 JVM（2026-10-31 完成）

`src-tauri/src/java/embedded.rs`：`dlopen(libjvm.so, RTLD_NOW|RTLD_GLOBAL)` + `JNI_CreateJavaVM`，
跑在 `spawn_blocking` 里（该线程成为 JVM main thread）。结果 `OnceLock` 缓存，
不调 `DestroyJavaVM`（会阻塞）。

实测（MuMu / Android 15 / x86_64）：

```
vm_ok=true  rc=0(JNI_OK)  java.version=21.0.12-internal  os.arch=amd64  191 ms
```

踩过的坑：

| 现象 | 原因 | 解法 |
|---|---|---|
| `libnio.so: dlopen failed: library "libnet.so" not found` | Android linker 解析 `DT_NEEDED` 只在**自己 namespace 的搜索路径**里找，JRE 的 `lib/` 不在其中；`java.library.path` 只影响 `System.loadLibrary` 拼绝对路径，管不了 so 内部的依赖 | 启动前把 `<jre>/lib/*.so` 逐个 `dlopen(RTLD_GLOBAL)` 塞进 namespace，多轮扫描处理依赖顺序 |
| JVM 信号处理链崩在 `__rt_sigprocmask` | 预加载把 `libjsig.so` 也拉进来了，它和 ART 的 `libsigchain` 抢同一批信号 | 预加载时排除 `libjsig.so` |
| 模拟器 `Fatal signal 4 (SIGILL)` | 模拟器 CPU 能力上报与实际不符，JIT 编出跑不了的指令 | 调试时 `<data>/jvm-opts.txt` 写 `-Xint`；真机不受影响 |

## M2 —— LWJGL 移植层（2026-10-31 完成）

`src-tauri/src/java/lwjgl.rs`：把 Pojav 的 natives 打成 `tar.xz` 放进 APK assets，
首装解到 `<data>/lwjgl/`，通过 `-Djava.library.path` + `-Dorg.lwjgl.librarypath` 交给 JVM。

装进去的东西（每 ABI 12 个 so，xz 后约 1.5 MB）：

```
libpojavexec.so     EGL 桥 + 输入桥（GLFW 的 native 实现）
liblwjgl.so         lwjgl 核心 JNI
liblwjgl_opengl.so  OpenGL
liblwjgl_stb.so / liblwjgl_tinyfd.so / liblwjgl_nanovg.so
libltw.so           GLES→GL 翻译（MC 1.17+ 需要 GL 3.2，GL4ES 只到 2.1，所以选 LTW）
libopenal.so        声音
libfreetype.so      字形
libbytehook.so / libexithook.so / liblinkerhook.so
```

外加 `lwjgl-classes.jar`（Pojav 的 fat jar，含 fork 版 `org.lwjgl.glfw.GLFW` 纯 Java 实现）。

实测（同一台 MuMu）：

```
native.pojavexec=ok   native.lwjgl=ok   native.lwjgl_opengl=ok
lwjgl.version=3.3.3-snapshot    lwjgl.memalloc=ok
glfw.class=ok         glfw.version=3.4.0
```

### 关键坑：jar 与 so 不是同一代

APK 自带的 `lwjgl-glfw-classes.jar` 里 `GLFW` 已经是 v3（有 `internalWindowSizeChanged`、
`keyDownBuffer`），但 `CallbackBridge` 还停在 v2 —— 只有 `nativeSendData(boolean,int,String)`。
而 `libpojavexec.so` 是 **input_bridge_v3**，`JNI_OnLoad` 注册的是一批全新 native：

```
nativeSendChar(C)Z          nativeSendKey(IIII)V
nativeSendCharMods(CI)Z     nativeSendMouseButton(III)V
nativeSendCursorPos(FF)V    nativeSendScreenSize(II)V
nativeSendScroll(DD)V       nativeSetUseInputStackQueue(Z)V
```

少一个就 `NoSuchMethodError`，整个 `loadLibrary("pojavexec")` 失败。
（这个清单是解析 so 的 `.rela.dyn` 重定位表还原出来的，不是猜的。）

**解法**：`src-tauri/android/shim/org/lwjgl/glfw/CallbackBridge.java` 写一份兼容实现，
打成 `shim.jar` 排在 classpath 最前面，覆盖 fat jar 里那份 v2。

### glfwInit 为什么还不能调

`glfwInit` 会取 `ANativeWindow` 和当前 Activity 的 Surface，没接上时是**原生层空指针**
（不是可捕获的异常），直接 SIGSEGV 带走进程。探针默认跳过（`glfw.init=skipped`），
要用 `-Dccl.probe.glfwInit=true` 才试。这是 M3 的活。

## 调试开关

| 开关 | 位置 | 用途 |
|---|---|---|
| `<data>/jvm-opts.txt` | `<data>` = `/data/user/0/com.caiwenjun.ccl/CCL` | 一行一个 JVM 选项追加到默认值后（模拟器加 `-Xint`） |
| `<data>/auto-probe` | 同上（空文件即可） | 启动 6 秒后自动跑 M2 探针并打 logcat |

用 adb 操作：

```bash
adb shell run-as com.caiwenjun.ccl touch CCL/auto-probe
adb shell run-as com.caiwenjun.ccl cp /sdcard/jvm-opts.txt CCL/java/jvm-opts.txt
adb logcat -s RustStdoutStderr | grep '\[M2\]'
```

## 构建（Windows 下绕开符号链接限制）

`bash build-android.sh [arm64|x86_64] [release|debug]`

tauri CLI 会把 `libccl_lib.so` 符号链接进 jniLibs，普通账号没有该权限必失败。
脚本改成 cargo 编完手工 `cp` 进 jniLibs，再用 `-x rustBuild*` 跳过 gradle 侧的 rust 任务。
