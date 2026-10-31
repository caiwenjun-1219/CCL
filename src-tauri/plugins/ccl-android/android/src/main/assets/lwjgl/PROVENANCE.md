# lwjgl 资源来源

这些文件**不是**本项目编译的，全部来自 PojavLauncher 的 Android 版 LWJGL 移植。
换版本时要么整包替换，要么按下面的清单重新生成。

## 来源

```
PojavLauncher.apk   net.kdt.pojavlaunch.debug
                    versionName = gladiolus-20250128-32ff194-v3_openjdk
```

对应上游仓库 `PojavLauncherTeam/PojavLauncher`（现已迁到 `AngelAuraMC/Amethyst-Android`），
分支 `v3_openjdk`。许可证 LGPL-3.0 / BSD-3，见 `docs/android-launch-research.md` 第六节。

## 文件

| 文件 | 内容 | 生成方式 |
|---|---|---|
| `lwjgl-bin-arm64-v8a.tar.xz` | 12 个 `.so`（arm64） | 见下 |
| `lwjgl-bin-x86_64.tar.xz` | 同上（x86_64，模拟器调试用） | 见下 |
| `lwjgl-classes.jar` | Pojav 的 fat jar，原路径 `assets/components/lwjgl3/lwjgl-glfw-classes.jar` | 直接拷贝 |
| `probe.jar` | M2 探针 | `bash src-tauri/android/probe/build.sh` |
| `shim.jar` | `CallbackBridge` 兼容层 | 同上 |

## 打包进 tar.xz 的 so（顺序即 tar 内顺序）

```
libpojavexec.so  libbytehook.so  libexithook.so  liblinkerhook.so
liblwjgl.so      liblwjgl_opengl.so  liblwjgl_stb.so
liblwjgl_tinyfd.so  liblwjgl_nanovg.so
libltw.so  libopenal.so  libfreetype.so
```

**刻意排除**：

- `libOSMesa.so`（12 MB，用不上）
- `libvulkan_freedreno.so`（7 MB，特定 GPU 才用）
- `libgl4es_114.so` —— 只支持 GL 2.1，MC 1.17+ 要 GL 3.2，所以选 `libltw.so`
- `libjvm`-相关的 JRE 库（那是 JRE 自己的事）
- `libpojavexec_awt.so`（Caciocavallo 用，暂不接 AWT）

tar 里条目是扁平结构（不带目录），`mode=0755`，`uid/gid=0`。
xz 用 `preset=6`，压缩后约 1.5 MB / ABI。

## 换版本时必须复核

1. `libpojavexec.so` 里是不是 `input_bridge_v3`：
   ```bash
   grep -c input_bridge_v3 libpojavexec.so
   ```
2. `lwjgl-classes.jar` 里 `CallbackBridge` 的 native 方法是否与 so 的
   `RegisterNatives` 表一致。不一致就得改
   `src-tauri/android/shim/org/lwjgl/glfw/CallbackBridge.java`。
   （解析 so 的 `.rela.dyn` 可以还原那张表，方法见 `docs/android-launch-research.md`。）
3. 改完把 `src-tauri/src/java/lwjgl.rs` 里的 `BUNDLE_REV` 加一，
   否则设备上会复用旧解包结果。
