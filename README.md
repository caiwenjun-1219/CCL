# CCL · Cwj Craft Launcher

Minecraft 启动器，桌面端基于 **Electron + Vue（XMCL 内核）**，Windows / macOS / Linux 三端同源。

> 早期用 Tauri 2 + React 写的 1.0.x 版本（PCL2 / HMCL / 原版三套界面）已于 2026-11-27 从 `main` 移除，源码可在该日期之前的提交里找回；安装包仍保留在 Releases。

## 下载

| 平台 | 文件 |
|---|---|
| Windows x64 | `CCL-<版本>-win-x64-setup.exe` |
| macOS Intel | `ccl-<版本>-x64.dmg` |
| macOS Apple Silicon | `ccl-<版本>-arm64.dmg` |
| Linux x64 / arm64 | `ccl-<版本>-x64.AppImage`、`ccl-<版本>-arm64.AppImage`（或对应 `tar.xz`） |

- Releases：<https://github.com/caiwenjun-1219/CCL/releases>
- 官网：<https://ccl.caiwenjun.top>
- 论坛：<http://caiwenjuncraftlauncher.free.mbbs.cc>

下载链接统一走加速反代 `https://zh.ccl.caiwenjun.top/dl/gh/<owner>/<repo>/releases/download/<tag>/<file>`，国内直连可用且支持 `Range` 断点续传。

## 功能

| 模块 | 说明 |
|---|---|
| 账户 | 微软正版（设备码）、**第三方皮肤站（LittleSkin / 自定义 Yggdrasil）**、离线模式 |
| 游戏 | 原版 / Fabric / Quilt / Forge / NeoForge，Java 自动匹配，多实例隔离 |
| 模组 | Modrinth + CurseForge 双源、一键安装、整合包导入导出 |
| 联机 | Campfire 房间码 |
| 更新 | 内置检查 + 公告，三端 CI 统一出包 |

## 仓库结构

```
CCL/
├── desktop/                    # Electron 桌面端（pnpm workspace）
│   ├── xmcl-keystone-ui/       # Vue 前端
│   ├── xmcl-electron-app/      # 主进程 / 打包脚本（build.ts + electron-builder）
│   ├── xmcl-runtime/           # 启动器运行时（账户、实例、启动）
│   └── packages/               # @xmcl/* 底层库（user / install / modrinth…）
├── website/                    # 官网静态站（Python 生成 + wrangler 部署）
├── cloud-worker/               # 早期 Tauri 版的云控后端（Cloudflare Worker，现网仍在跑）
└── .github/workflows/          # desktop-linux / desktop-mac / desktop-win 三端构建
```

## 构建

### 本地

前置：Node.js 22+、pnpm 11、各平台原生构建工具（Windows 需 VS BuildTools，mac 需 Xcode CLT）。

```bash
cd desktop
pnpm install
pnpm run --prefix xmcl-keystone-ui build          # 渲染进程

# 打当前平台安装包（Linux 可用 CCL_LINUX_TARGETS 裁剪目标）
RELEASE=true CCL_PLATFORM=win CCL_ARCH=x64 pnpm run --prefix xmcl-electron-app build
```

产物落在 `desktop/xmcl-electron-app/build/output/`。`node_datachannel` 是原生模块，**不能跨平台交叉打包**，Windows / macOS / Linux 必须在各自系统上构建。

### CI 发布

推 `v*` tag 会同时触发三条流水线，产物自动挂到同名 Release：

```bash
git tag v2.1.2 && git push origin v2.1.2
```

也可以手动 `workflow_dispatch`，填 `ref` 与 `release_tag` 只跑单端。顺序上先建好 Release（草稿即可）再推 tag，否则 `gh release upload` 找不到目标。

## 官网

```bash
cd website
python build.py
cp index.html dist/index.html
npx wrangler deploy
```

## 数据目录

- Windows：`%APPDATA%\CCL`
- macOS：`~/Library/Application Support/CCL`
- Linux：`~/.config/CCL`

## 许可

`desktop/` 源自 [XMCL](https://github.com/Voxelum/x-minecraft-launcher)（MIT），CCL 的改动同样以 MIT 发布；随包分发的 Campfire 为 GPL-3.0。
