# CCL 云控后端（Cloudflare Worker）

Cwj Craft Launcher 的云控更新后端，部署在 Cloudflare Workers 上。

| 路径 | 说明 |
|---|---|
| `https://cclconfig.caiwenjun.top/config` | 启动器读取的云控配置（最新版本、最低版本、下载地址、SHA256、更新说明、公告、强制更新） |
| `https://cclconfig.caiwenjun.top/download/<key>` | 安装包下载（R2 存储，带下载计数） |
| `https://cclconfig.caiwenjun.top/dashboard` | 管理面板（默认密码 `123456`，登录后可改） |

## 一、部署前准备

1. 域名 `caiwenjun.top` 已托管在 Cloudflare（NS 指向 Cloudflare）。
2. 安装 Node.js 18+。

## 二、部署步骤

```bash
# 1. 安装依赖（已配置 npmmirror 镜像）
cd F:\Projects\CCL\cloud-worker
npm install

# 2. 登录 Cloudflare（会打开浏览器授权）
npx wrangler login

# 3. 创建 KV 命名空间，把输出的 id 填到 wrangler.toml 的 [[kv_namespaces]] id 处
npx wrangler kv:namespace create CCL_KV

# 4. 创建 R2 存储桶（免费计划含 10GB/月）
npx wrangler r2 bucket create ccl-updates

# 5. 部署
npm run deploy
```

## 三、绑定自定义域名

部署完成后，把 `cclconfig.caiwenjun.top` 绑到 Worker：

- Cloudflare Dashboard → Workers & Pages → `ccl-config` → Settings → Domains & Routes → Add → Custom domain → 填 `cclconfig.caiwenjun.top`
- 域名在 Cloudflare 激活后，DNS 记录会自动创建，等几分钟生效。

## 四、使用面板

1. 浏览器打开 `https://cclconfig.caiwenjun.top/dashboard`
2. 默认密码 `123456` 登录
3. 「安装包管理」页上传构建好的安装包（如 `CCL_1.0.1_x64-setup.exe`），复制返回的下载链接和 SHA256
4. 「更新配置」页填写最新版本号、最低版本号、下载地址、SHA256、更新说明，保存
5. 「修改密码」页把默认密码改掉

## 五、本地调试

```bash
npm run dev        # 本地起 http://localhost:8787
npm run tail       # 线上实时日志
```

## 六、配置字段说明（/config 返回）

```json
{
  "latest_version": "1.0.1",
  "min_version": "1.0.0",
  "download_url": "https://cclconfig.caiwenjun.top/download/CCL_1.0.1_x64-setup.exe",
  "sha256": "...",
  "release_notes": "1. 修复下载断点续传\n2. 新增 NeoForge 支持",
  "force_update": false,
  "announcement": "欢迎使用 CCL！",
  "updated_at": "2026-10-22T12:00:00.000Z"
}
```

- `latest_version`：启动器检测到更新后提示用户升级
- `min_version`：当前版本低于它且 `force_update=true` 时，启动器强制更新不可跳过
- `announcement`：启动器首页公告，可随时下发
- `download_url`：也可以指向 GitHub Releases 等外部地址，不强制用 `/download/`

## 七、常见问题

- **R2 需要付费吗？** 免费计划含 10GB 存储/月，启动器安装包几 MB，足够。R2 可能要求绑定支付方式，按提示操作即可；不想用 R2 的话，安装包传 GitHub Releases，`download_url` 直接填外部链接即可。
- **KV 是最终一致的吗？** 改配置后全球生效通常几秒，启动器下次检查更新即可看到。
- **上传超过 95MB 失败？** Workers 免费计划请求体上限 100MB。更大安装包请放外部存储。
