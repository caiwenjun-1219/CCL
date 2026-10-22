# Cwj Craft Launcher (CCL)

基于 **Tauri 2 + React + TypeScript + Rust** 的 Minecraft 启动器，Windows 优先。

## 功能特性

| 模块 | 功能 |
|---|---|
| 游戏启动 | 原版 / Fabric / Quilt / Forge / NeoForge，classpath 自动组装、natives 解压、参数规则解析、进程管理与实时日志流 |
| 版本下载 | BMCLAPI 国内镜像优先，自动一级一级回退到 Mojang 官方源；断点续传 + SHA1 校验 + 并发下载 |
| Java 管理 | 自动扫描本机 JDK、Adoptium 在线下载托管 JDK、按 MC 版本自动选择 Java 主版本 |
| 账户 | 微软设备码登录（Xbox Live → XSTS → Minecraft 令牌链，自动刷新）+ 离线模式 |
| 模组市场 | Modrinth API v2 + CurseForge API 双源搜索、版本匹配、一键安装、启用/禁用/删除 |
| 整合包 | 导入 Modrinth `.mrpack` 与 CurseForge `manifest.json` 整合包（自动装 MC 版本+加载器+全部模组+配置）；导出实例为 `.mrpack` |
| 云控更新 | Cloudflare Worker 后端：启动时自动检查更新、首页公告、强制更新、SHA256 校验、静默安装 |
| 下载引擎 | 多源回退、分片并发（默认 24 路）、Range 断点续传、失败自动换源、全局下载任务坞 |

## 技术栈

- **前端**：Tauri 2 · React 19 · TypeScript · Vite 6 · Tailwind CSS 4 · Zustand · React Router · lucide-react
- **后端**：Rust · Tauri 2 · reqwest（native-tls）· tokio · serde · sha1/sha2/md5 · zip
- **云控**：Cloudflare Workers · KV（配置/密码哈希）· R2（安装包存储）
- **资源源**：BMCLAPI2（版本/库/资源/Fabric/Quilt 元数据）→ Mojang 官方；NeoForge 走 maven.neoforged.net；Java 走 Adoptium API

## 目录结构

```
CCL/
├── src/                        # React 前端
│   ├── components/Layout.tsx   # 标题栏/侧边栏/下载坞/更新横幅/强制更新弹窗/公告
│   ├── pages/                  # 首页/实例/版本/模组/整合包/Java/账户/设置
│   ├── lib/tauri.ts            # 命令封装与事件订阅（前后端契约）
│   └── store/app.ts            # Zustand 全局状态
├── src-tauri/                  # Rust 后端
│   ├── src/downloader.rs       # 多源回退下载引擎（核心）
│   ├── src/minecraft.rs        # 版本清单/安装/加载器（Fabric/Quilt/Forge/NeoForge）
│   ├── src/java.rs             # Java 扫描/下载/自动选择
│   ├── src/auth.rs             # 微软设备码登录 + 离线模式
│   ├── src/launcher.rs         # classpath/参数规则/进程/日志
│   ├── src/modrinth.rs         # Modrinth API v2
│   ├── src/curseforge.rs       # CurseForge API
│   ├── src/modpack.rs          # .mrpack / CurseForge 整合包导入导出
│   ├── src/updater.rs          # 云控更新（cclconfig.caiwenjun.top）
│   ├── src/protocol.rs         # ccl:// 自定义协议（本地文件服务）
│   └── src/instances.rs        # 实例 CRUD / 模组管理
├── cloud-worker/               # 云控后端（Cloudflare Worker）
└── docs/                       # 技术方案文档
```

## ccl:// 自定义协议

参考 VersePc2 的 `wewp://` 设计（`register_uri_scheme_protocol`），CCL 注册了 `ccl` 协议为前端提供本地文件服务。Windows 上经 wry workaround，**前端统一用 `http://ccl.localhost/<路径>` 访问**（Rust 侧收到的是 `ccl://localhost/<路径>`）：

| URL | 说明 |
|---|---|
| `http://ccl.localhost/instances/<实例ID>/<相对路径>` | 实例游戏目录（如 `screenshots/xxx.png`） |
| `http://ccl.localhost/data/<相对路径>` | CCL 数据目录（如 `accounts.json`） |

特性：MIME 推断、HTTP Range（206，大媒体不会整文件读入内存）、CORS、逐段 percent 解码 + `..` 拒绝 + canonicalize 前缀校验（防路径穿越）。前端辅助函数见 `src/lib/tauri.ts` 的 `cclUrl()`，首个用途是首页实例卡片展示最新截图。

## 本地开发

前置：Node.js 18+、Rust stable、MSVC BuildTools（C++ 桌面开发负载）。

```bash
# 前端依赖（npmmirror 镜像）
npm install

# 开发模式（热更新）
npm run tauri dev

# 类型检查
npx tsc --noEmit

# 生产构建（产出 NSIS 安装包）
npm run tauri build
```

Rust 工具链在本机位于 `F:\rust`（RUSTUP_HOME/CARGO_HOME），crates.io 走 rsproxy.cn 镜像。

## 云控后端部署

详见 [cloud-worker/README.md](cloud-worker/README.md)。要点：

```bash
cd cloud-worker
npm install
npx wrangler login
npx wrangler kv:namespace create CCL_KV   # id 填入 wrangler.toml
npx wrangler r2 bucket create ccl-updates
npm run deploy
# 然后在 Cloudflare 后台把 cclconfig.caiwenjun.top 绑定为该 Worker 自定义域名
```

| 路径 | 用途 |
|---|---|
| `/config` | 启动器读取：latest_version / min_version / download_url / sha256 / release_notes / force_update / announcement |
| `/download/<key>` | R2 安装包分发（带下载计数） |
| `/dashboard` | 管理面板，默认密码 `123456`（登录后可改） |

## 上传 GitHub

```bash
git init
git add .
git commit -m "feat: CCL v1.0.0 - Tauri Minecraft launcher with cloud control"
git branch -M main
git remote add origin https://github.com/<你的用户名>/<仓库名>.git
git push -u origin main
```

## 数据存储位置

- 设置/账户/实例：`%APPDATA%\com.caiwenjun.ccl\`
  - `settings.json`、`accounts.json`、`instances/<id>/instance.json`
  - 每个实例的游戏目录：`instances/<id>/game/`（versions/libraries/assets/mods/config…）
  - 托管的 JDK：`java/<版本>/`
