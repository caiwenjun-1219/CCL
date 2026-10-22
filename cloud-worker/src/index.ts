/**
 * CCL 云控后端 - Cloudflare Worker
 * 域名: cclconfig.caiwenjun.top
 *
 * 路由:
 *   GET  /config                 启动器获取云控配置(更新信息/公告)
 *   GET  /download/<key>         从 R2 下载安装包(带下载计数)
 *   GET  /dashboard              管理面板(未登录显示登录页)
 *   POST /dashboard/login        面板登录
 *   POST /dashboard/logout       面板退出
 *   POST /dashboard/config       保存云控配置
 *   POST /dashboard/upload       上传安装包到 R2
 *   GET  /dashboard/files        安装包列表
 *   POST /dashboard/files/delete 删除安装包
 *   POST /dashboard/password     修改面板密码
 */

export interface Env {
  CCL_KV: KVNamespace;
  CCL_BUCKET: R2Bucket;
}

interface CloudConfig {
  latest_version: string;   // 最新版本号, 如 "1.0.1"
  min_version: string;      // 最低可运行版本, 低于它则强制更新
  download_url: string;     // 安装包下载地址(建议指向本 Worker /download/<key>)
  sha256: string;           // 安装包 SHA256(可选, 启动器会校验)
  release_notes: string;    // 更新说明
  force_update: boolean;    // 是否强制更新
  announcement: string;     // 首页公告
  updated_at: string;       // 配置更新时间(自动填充)
}

const DEFAULT_CONFIG: CloudConfig = {
  latest_version: "1.0.0",
  min_version: "1.0.0",
  download_url: "",
  sha256: "",
  release_notes: "",
  force_update: false,
  announcement: "",
  updated_at: "",
};

const DEFAULT_PASSWORD = "123456";
const COOKIE_NAME = "ccl_session";

const enc = new TextEncoder();

function json(data: unknown, status = 200, extra: Record<string, string> = {}): Response {
  return new Response(JSON.stringify(data), {
    status,
    headers: {
      "Content-Type": "application/json; charset=utf-8",
      "Access-Control-Allow-Origin": "*",
      "Cache-Control": "no-store",
      ...extra,
    },
  });
}

async function sha256Hex(data: ArrayBuffer | string): Promise<string> {
  const buf = typeof data === "string" ? enc.encode(data) : new Uint8Array(data);
  const hash = await crypto.subtle.digest("SHA-256", buf);
  return [...new Uint8Array(hash)].map((b) => b.toString(16).padStart(2, "0")).join("");
}

async function hmacHex(key: string, msg: string): Promise<string> {
  const k = await crypto.subtle.importKey(
    "raw",
    enc.encode(key),
    { name: "HMAC", hash: "SHA-256" },
    false,
    ["sign"]
  );
  const buf = await crypto.subtle.sign("HMAC", k, enc.encode(msg));
  return [...new Uint8Array(buf)].map((b) => b.toString(16).padStart(2, "0")).join("");
}

async function passwordHash(env: Env): Promise<string> {
  return (await env.CCL_KV.get("password_hash")) || (await sha256Hex(DEFAULT_PASSWORD));
}

function dayStr(offsetMs = 0): string {
  return new Date(Date.now() + offsetMs).toISOString().slice(0, 10);
}

/** 会话令牌 = HMAC(密码哈希, 日期)。密码一改, 旧 Cookie 全部失效 */
async function sessionTokenFor(env: Env, day: string): Promise<string> {
  return hmacHex(await passwordHash(env), "ccl-session:" + day);
}

function getCookie(req: Request, name: string): string | null {
  const cookie = req.headers.get("Cookie") || "";
  for (const part of cookie.split(";")) {
    const idx = part.indexOf("=");
    if (idx < 0) continue;
    if (part.slice(0, idx).trim() === name) return part.slice(idx + 1).trim();
  }
  return null;
}

async function isAuthed(env: Env, req: Request): Promise<boolean> {
  const cookie = getCookie(req, COOKIE_NAME);
  if (!cookie) return false;
  return (
    cookie === (await sessionTokenFor(env, dayStr())) ||
    cookie === (await sessionTokenFor(env, dayStr(-86400000)))
  );
}

async function readConfig(env: Env): Promise<CloudConfig> {
  const raw = await env.CCL_KV.get("config");
  const cfg = { ...DEFAULT_CONFIG };
  if (raw) {
    try {
      Object.assign(cfg, JSON.parse(raw));
    } catch {
      /* 配置损坏时回退默认值 */
    }
  }
  return cfg;
}

// ---------------------------------------------------------------- 处理器

async function handleConfig(env: Env): Promise<Response> {
  return json(await readConfig(env));
}

async function handleDownload(env: Env, path: string, ctx: ExecutionContext): Promise<Response> {
  const key = decodeURIComponent(path.replace(/^\/download\//, ""));
  if (!key || key.includes("..")) return json({ error: "invalid key" }, 400);
  const obj = await env.CCL_BUCKET.get(key);
  if (!obj) return json({ error: "file not found: " + key }, 404);

  // 下载计数(尽力而为, 不阻塞响应)
  const ck = "dl:" + key;
  ctx.waitUntil(
    env.CCL_KV.get(ck)
      .then((v) => env.CCL_KV.put(ck, String((parseInt(v || "0", 10) || 0) + 1)))
      .catch(() => {})
  );

  const headers = new Headers();
  obj.writeHttpMetadata(headers);
  headers.set(
    "Content-Disposition",
    'attachment; filename="' + encodeURIComponent(key.split("/").pop() || key) + '"'
  );
  headers.set("Cache-Control", "public, max-age=60");
  return new Response(obj.body, { headers });
}

async function handleLogin(env: Env, req: Request): Promise<Response> {
  const form = await req.formData();
  const password = String(form.get("password") || "");
  if ((await sha256Hex(password)) !== (await passwordHash(env))) {
    return dashboardPage("密码错误, 请重试");
  }
  const token = await sessionTokenFor(env, dayStr());
  return new Response(null, {
    status: 302,
    headers: {
      Location: "/dashboard",
      "Set-Cookie": COOKIE_NAME + "=" + token + "; Path=/; HttpOnly; SameSite=Lax; Max-Age=86400",
    },
  });
}

function handleLogout(): Response {
  return new Response(null, {
    status: 302,
    headers: {
      Location: "/dashboard",
      "Set-Cookie": COOKIE_NAME + "=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0",
    },
  });
}

async function handleSaveConfig(env: Env, req: Request): Promise<Response> {
  const body = (await req.json()) as Partial<CloudConfig>;
  const cur = await readConfig(env);
  const next: CloudConfig = {
    latest_version: String(body.latest_version ?? cur.latest_version),
    min_version: String(body.min_version ?? cur.min_version),
    download_url: String(body.download_url ?? cur.download_url),
    sha256: String(body.sha256 ?? cur.sha256),
    release_notes: String(body.release_notes ?? cur.release_notes),
    force_update: Boolean(body.force_update ?? cur.force_update),
    announcement: String(body.announcement ?? cur.announcement),
    updated_at: new Date().toISOString(),
  };
  await env.CCL_KV.put("config", JSON.stringify(next));
  return json({ ok: true, config: next });
}

async function handleUpload(env: Env, req: Request): Promise<Response> {
  const form = await req.formData();
  const file = form.get("file");
  if (!file || typeof file === "string") return json({ error: "no file field" }, 400);
  const f = file as File;
  if (f.size > 95 * 1024 * 1024) return json({ error: "file too large (max 95MB on free plan)" }, 413);

  const customKey = String(form.get("key") || "").trim();
  const key = (customKey || f.name).replace(/[^\w.\-\/]/g, "_");
  const buf = await f.arrayBuffer();

  await env.CCL_BUCKET.put(key, buf, {
    httpMetadata: { contentType: f.type || "application/octet-stream" },
  });
  return json({ ok: true, key, size: buf.byteLength, sha256: await sha256Hex(buf) });
}

async function handleFiles(env: Env): Promise<Response> {
  const listed = await env.CCL_BUCKET.list({ limit: 500 });
  const files = await Promise.all(
    listed.objects.map(async (o) => ({
      key: o.key,
      size: o.size,
      uploaded: o.uploaded.toISOString(),
      downloads: parseInt((await env.CCL_KV.get("dl:" + o.key)) || "0", 10) || 0,
    }))
  );
  files.sort((a, b) => (a.uploaded < b.uploaded ? 1 : -1));
  return json({ files });
}

async function handleDeleteFile(env: Env, req: Request): Promise<Response> {
  const body = (await req.json()) as { key?: string };
  const key = body.key || "";
  if (!key || key.includes("..")) return json({ error: "invalid key" }, 400);
  await env.CCL_BUCKET.delete(key);
  await env.CCL_KV.delete("dl:" + key).catch(() => {});
  return json({ ok: true });
}

async function handleChangePassword(env: Env, req: Request): Promise<Response> {
  const form = await req.formData();
  const oldPw = String(form.get("old_password") || "");
  const newPw = String(form.get("new_password") || "");
  const confirm = String(form.get("confirm_password") || "");
  if (newPw.length < 4) return json({ error: "新密码至少 4 位" }, 400);
  if (newPw !== confirm) return json({ error: "两次输入的新密码不一致" }, 400);
  if ((await sha256Hex(oldPw)) !== (await passwordHash(env))) {
    return json({ error: "原密码错误" }, 400);
  }
  await env.CCL_KV.put("password_hash", await sha256Hex(newPw));
  return json({ ok: true, message: "密码已修改, 请重新登录" });
}

// ---------------------------------------------------------------- 面板页面

function esc(s: unknown): string {
  return String(s ?? "")
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

function dashboardPage(loginError = ""): Response {
  const errHtml = loginError
    ? '<div class="err">' + esc(loginError) + "</div>"
    : "";
  const html = `<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>CCL 云控面板</title>
<style>
  * { margin: 0; padding: 0; box-sizing: border-box; }
  body { font-family: "Segoe UI", "Microsoft YaHei", sans-serif; background: #0f1420; color: #e6e9f0; min-height: 100vh; }
  .wrap { max-width: 980px; margin: 0 auto; padding: 32px 20px 60px; }
  .brand { display: flex; align-items: center; gap: 12px; margin-bottom: 24px; }
  .brand .cube { width: 40px; height: 40px; border-radius: 8px; background: linear-gradient(135deg,#5db95f,#3c8c3f); position: relative; box-shadow: 0 4px 14px rgba(93,185,95,.35); }
  .brand .cube::after { content: "CCL"; position: absolute; inset: 0; display: flex; align-items: center; justify-content: center; font-weight: 800; font-size: 13px; color: #0f1420; }
  .brand h1 { font-size: 20px; font-weight: 700; }
  .brand p { font-size: 12px; color: #8b93a7; }
  .card { background: #171e2e; border: 1px solid #232c42; border-radius: 14px; padding: 22px; margin-bottom: 18px; }
  .card h2 { font-size: 15px; margin-bottom: 16px; color: #aeb6c8; font-weight: 600; }
  .tabs { display: flex; gap: 8px; margin-bottom: 18px; flex-wrap: wrap; }
  .tabs button { background: #171e2e; border: 1px solid #232c42; color: #aeb6c8; padding: 8px 16px; border-radius: 9px; cursor: pointer; font-size: 13px; }
  .tabs button.active { background: #2f6b3a; border-color: #3f8a4d; color: #fff; }
  label { display: block; font-size: 12px; color: #8b93a7; margin: 12px 0 6px; }
  input[type=text], input[type=password], textarea { width: 100%; background: #0f1420; border: 1px solid #2a3450; border-radius: 8px; color: #e6e9f0; padding: 9px 12px; font-size: 13px; outline: none; }
  input:focus, textarea:focus { border-color: #4c9a56; }
  textarea { min-height: 90px; resize: vertical; font-family: inherit; }
  .row { display: flex; gap: 14px; flex-wrap: wrap; }
  .row > div { flex: 1; min-width: 200px; }
  .check { display: flex; align-items: center; gap: 8px; margin-top: 14px; font-size: 13px; color: #cdd3e0; }
  .btn { background: #3f8a4d; color: #fff; border: none; padding: 10px 22px; border-radius: 9px; font-size: 13px; cursor: pointer; font-weight: 600; }
  .btn:hover { background: #4c9a56; }
  .btn.ghost { background: transparent; border: 1px solid #2a3450; color: #aeb6c8; }
  .btn.ghost:hover { border-color: #4c9a56; color: #fff; }
  .btn.danger { background: #8c3c3c; }
  .btn.sm { padding: 5px 12px; font-size: 12px; border-radius: 7px; }
  table { width: 100%; border-collapse: collapse; font-size: 13px; }
  th { text-align: left; color: #8b93a7; font-weight: 500; padding: 8px 10px; border-bottom: 1px solid #232c42; }
  td { padding: 10px; border-bottom: 1px solid #1c2438; word-break: break-all; }
  .mono { font-family: Consolas, monospace; font-size: 12px; color: #9fb4ff; }
  .msg { margin-top: 12px; font-size: 13px; padding: 10px 14px; border-radius: 8px; display: none; }
  .msg.ok { background: rgba(63,138,77,.15); color: #7fd08f; display: block; }
  .msg.bad { background: rgba(140,60,60,.18); color: #e08c8c; display: block; }
  .err { background: rgba(140,60,60,.18); color: #e08c8c; padding: 10px 14px; border-radius: 8px; font-size: 13px; margin-bottom: 14px; }
  .login-card { max-width: 380px; margin: 80px auto; }
  .hint { font-size: 12px; color: #6b7488; margin-top: 8px; line-height: 1.7; }
  .bar { display:flex; justify-content: space-between; align-items:center; margin-bottom: 16px; }
  .bar a { color: #8b93a7; font-size: 12px; text-decoration: none; }
  .bar a:hover { color: #fff; }
  .empty { color: #6b7488; font-size: 13px; padding: 18px 0; text-align: center; }
</style>
</head>
<body>
<div class="wrap">
  <div class="brand">
    <div class="cube"></div>
    <div>
      <h1>Cwj Craft Launcher 云控面板</h1>
      <p>cclconfig.caiwenjun.top</p>
    </div>
  </div>
  ${errHtml}
  <div class="card login-card">
    <h2>面板登录</h2>
    <form method="POST" action="/dashboard/login">
      <label>管理密码</label>
      <input type="password" name="password" placeholder="默认密码 123456" autofocus required>
      <div style="margin-top:18px"><button class="btn" style="width:100%">登 录</button></div>
    </form>
    <p class="hint">默认密码 123456, 登录后在「修改密码」页签更换。忘记密码可在 Cloudflare KV 中删除 password_hash 键恢复默认。</p>
  </div>
</div>
</body>
</html>`;
  return new Response(html, {
    headers: { "Content-Type": "text/html; charset=utf-8", "Cache-Control": "no-store" },
  });
}

function dashboardApp(): Response {
  const html = `<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>CCL 云控面板</title>
<style>
  * { margin: 0; padding: 0; box-sizing: border-box; }
  body { font-family: "Segoe UI", "Microsoft YaHei", sans-serif; background: #0f1420; color: #e6e9f0; min-height: 100vh; }
  .wrap { max-width: 980px; margin: 0 auto; padding: 32px 20px 60px; }
  .brand { display: flex; align-items: center; gap: 12px; margin-bottom: 24px; }
  .brand .cube { width: 40px; height: 40px; border-radius: 8px; background: linear-gradient(135deg,#5db95f,#3c8c3f); position: relative; box-shadow: 0 4px 14px rgba(93,185,95,.35); }
  .brand .cube::after { content: "CCL"; position: absolute; inset: 0; display: flex; align-items: center; justify-content: center; font-weight: 800; font-size: 13px; color: #0f1420; }
  .brand h1 { font-size: 20px; font-weight: 700; }
  .brand p { font-size: 12px; color: #8b93a7; }
  .card { background: #171e2e; border: 1px solid #232c42; border-radius: 14px; padding: 22px; margin-bottom: 18px; }
  .card h2 { font-size: 15px; margin-bottom: 16px; color: #aeb6c8; font-weight: 600; }
  .tabs { display: flex; gap: 8px; margin-bottom: 18px; flex-wrap: wrap; }
  .tabs button { background: #171e2e; border: 1px solid #232c42; color: #aeb6c8; padding: 8px 16px; border-radius: 9px; cursor: pointer; font-size: 13px; }
  .tabs button.active { background: #2f6b3a; border-color: #3f8a4d; color: #fff; }
  label { display: block; font-size: 12px; color: #8b93a7; margin: 12px 0 6px; }
  input[type=text], input[type=password], textarea { width: 100%; background: #0f1420; border: 1px solid #2a3450; border-radius: 8px; color: #e6e9f0; padding: 9px 12px; font-size: 13px; outline: none; }
  input:focus, textarea:focus { border-color: #4c9a56; }
  textarea { min-height: 90px; resize: vertical; font-family: inherit; }
  .row { display: flex; gap: 14px; flex-wrap: wrap; }
  .row > div { flex: 1; min-width: 200px; }
  .check { display: flex; align-items: center; gap: 8px; margin-top: 14px; font-size: 13px; color: #cdd3e0; }
  .btn { background: #3f8a4d; color: #fff; border: none; padding: 10px 22px; border-radius: 9px; font-size: 13px; cursor: pointer; font-weight: 600; }
  .btn:hover { background: #4c9a56; }
  .btn.ghost { background: transparent; border: 1px solid #2a3450; color: #aeb6c8; }
  .btn.ghost:hover { border-color: #4c9a56; color: #fff; }
  .btn.danger { background: #8c3c3c; }
  .btn.sm { padding: 5px 12px; font-size: 12px; border-radius: 7px; }
  table { width: 100%; border-collapse: collapse; font-size: 13px; }
  th { text-align: left; color: #8b93a7; font-weight: 500; padding: 8px 10px; border-bottom: 1px solid #232c42; }
  td { padding: 10px; border-bottom: 1px solid #1c2438; word-break: break-all; }
  .mono { font-family: Consolas, monospace; font-size: 12px; color: #9fb4ff; }
  .msg { margin-top: 12px; font-size: 13px; padding: 10px 14px; border-radius: 8px; display: none; }
  .msg.ok { background: rgba(63,138,77,.15); color: #7fd08f; display: block; }
  .msg.bad { background: rgba(140,60,60,.18); color: #e08c8c; display: block; }
  .bar { display:flex; justify-content: space-between; align-items:center; margin-bottom: 16px; }
  .bar form { margin: 0; }
  .hint { font-size: 12px; color: #6b7488; margin-top: 8px; line-height: 1.7; }
  .empty { color: #6b7488; font-size: 13px; padding: 18px 0; text-align: center; }
  pre { background: #0f1420; border: 1px solid #232c42; border-radius: 8px; padding: 14px; font-size: 12px; color: #9fb4ff; overflow: auto; }
</style>
</head>
<body>
<div class="wrap">
  <div class="brand">
    <div class="cube"></div>
    <div>
      <h1>Cwj Craft Launcher 云控面板</h1>
      <p>cclconfig.caiwenjun.top</p>
    </div>
    <div style="margin-left:auto">
      <form method="POST" action="/dashboard/logout"><button class="btn ghost sm">退出登录</button></form>
    </div>
  </div>

  <div class="tabs">
    <button id="tab-config" class="active" onclick="showTab('config')">更新配置</button>
    <button id="tab-files" onclick="showTab('files')">安装包管理</button>
    <button id="tab-password" onclick="showTab('password')">修改密码</button>
  </div>

  <!-- 更新配置 -->
  <div id="page-config" class="card">
    <h2>云控配置（启动器启动时读取 /config）</h2>
    <div class="row">
      <div>
        <label>最新版本号 latest_version</label>
        <input type="text" id="latest_version" placeholder="1.0.1">
      </div>
      <div>
        <label>最低可运行版本 min_version（低于则强制更新）</label>
        <input type="text" id="min_version" placeholder="1.0.0">
      </div>
    </div>
    <label>安装包下载地址 download_url</label>
    <input type="text" id="download_url" placeholder="https://cclconfig.caiwenjun.top/download/CCL_1.0.1_x64-setup.exe">
    <div class="row">
      <div>
        <label>SHA256 校验（可选，启动器下载后校验）</label>
        <input type="text" id="sha256" placeholder="上传安装包后自动生成，可一键复制">
      </div>
    </div>
    <label>更新说明 release_notes</label>
    <textarea id="release_notes" placeholder="1. 修复xxx&#10;2. 新增xxx"></textarea>
    <label>首页公告 announcement</label>
    <textarea id="announcement" placeholder="显示在启动器首页的公告，可留空"></textarea>
    <div class="check">
      <input type="checkbox" id="force_update">
      <label for="force_update" style="margin:0">强制更新（低于 min_version 的版本必须更新才能用）</label>
    </div>
    <div style="margin-top:18px">
      <button class="btn" onclick="saveConfig()">保存配置</button>
      <button class="btn ghost" onclick="loadConfig()">重新载入</button>
    </div>
    <div id="config-msg" class="msg"></div>
    <h2 style="margin-top:22px">/config 实时预览</h2>
    <pre id="config-preview">加载中...</pre>
  </div>

  <!-- 安装包管理 -->
  <div id="page-files" class="card" style="display:none">
    <h2>安装包管理（存储在 Cloudflare R2，通过 /download/&lt;key&gt; 分发）</h2>
    <div class="row">
      <div style="flex:2">
        <label>选择安装包（.exe / .zip，免费计划单文件 ≤ 95MB）</label>
        <input type="file" id="upload-file">
      </div>
      <div style="flex:1">
        <label>存储键名 key（留空则用文件名）</label>
        <input type="text" id="upload-key" placeholder="CCL_1.0.1_x64-setup.exe">
      </div>
    </div>
    <div style="margin-top:16px">
      <button class="btn" onclick="uploadFile()">上传</button>
      <button class="btn ghost" onclick="loadFiles()">刷新列表</button>
    </div>
    <div id="upload-msg" class="msg"></div>
    <div id="file-table" style="margin-top:16px"></div>
    <p class="hint">上传后把返回的 SHA256 填到「更新配置」页即可开启校验；下载地址填 https://cclconfig.caiwenjun.top/download/&lt;key&gt;</p>
  </div>

  <!-- 修改密码 -->
  <div id="page-password" class="card" style="display:none">
    <h2>修改面板密码</h2>
    <label>原密码</label>
    <input type="password" id="old_password">
    <label>新密码（至少 4 位）</label>
    <input type="password" id="new_password">
    <label>确认新密码</label>
    <input type="password" id="confirm_password">
    <div style="margin-top:18px"><button class="btn" onclick="changePassword()">修改密码</button></div>
    <div id="pw-msg" class="msg"></div>
    <p class="hint">修改成功后所有已登录会话立即失效，需要用新密码重新登录。</p>
  </div>
</div>

<script>
function showTab(name) {
  var pages = { config: 'page-config', files: 'page-files', password: 'page-password' };
  var tabs = { config: 'tab-config', files: 'tab-files', password: 'tab-password' };
  for (var k in pages) {
    document.getElementById(pages[k]).style.display = k === name ? '' : 'none';
    document.getElementById(tabs[k]).classList.toggle('active', k === name);
  }
  if (name === 'files') loadFiles();
}
function showMsg(id, ok, text) {
  var el = document.getElementById(id);
  el.className = 'msg ' + (ok ? 'ok' : 'bad');
  el.textContent = text;
}
function fmtSize(n) {
  if (n < 1024) return n + ' B';
  if (n < 1048576) return (n / 1024).toFixed(1) + ' KB';
  return (n / 1048576).toFixed(2) + ' MB';
}
async function loadConfig() {
  var r = await fetch('/config');
  var c = await r.json();
  latest_version.value = c.latest_version || '';
  min_version.value = c.min_version || '';
  download_url.value = c.download_url || '';
  sha256.value = c.sha256 || '';
  release_notes.value = c.release_notes || '';
  announcement.value = c.announcement || '';
  force_update.checked = !!c.force_update;
  document.getElementById('config-preview').textContent = JSON.stringify(c, null, 2);
}
async function saveConfig() {
  var body = {
    latest_version: latest_version.value.trim(),
    min_version: min_version.value.trim(),
    download_url: download_url.value.trim(),
    sha256: sha256.value.trim(),
    release_notes: release_notes.value,
    force_update: force_update.checked,
    announcement: announcement.value
  };
  var r = await fetch('/dashboard/config', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body)
  });
  var j = await r.json();
  if (j.ok) { showMsg('config-msg', true, '保存成功，启动器下次检查更新即可生效'); loadConfig(); }
  else showMsg('config-msg', false, j.error || '保存失败');
}
async function loadFiles() {
  var r = await fetch('/dashboard/files');
  var j = await r.json();
  var box = document.getElementById('file-table');
  if (!j.files || !j.files.length) { box.innerHTML = '<div class="empty">暂无安装包，请先上传</div>'; return; }
  var h = '<table><tr><th>键名 key</th><th>大小</th><th>下载次数</th><th>上传时间</th><th>操作</th></tr>';
  j.files.forEach(function (f) {
    h += '<tr><td class="mono">' + f.key + '</td><td>' + fmtSize(f.size) + '</td><td>' + f.downloads +
      '</td><td>' + f.uploaded.replace('T', ' ').slice(0, 19) + '</td><td>' +
      '<button class="btn ghost sm" onclick="copyUrl(\\'' + f.key + '\\')">复制链接</button> ' +
      '<button class="btn danger sm" onclick="delFile(\\'' + f.key + '\\')">删除</button></td></tr>';
  });
  box.innerHTML = h + '</table>';
}
async function uploadFile() {
  var input = document.getElementById('upload-file');
  if (!input.files.length) { showMsg('upload-msg', false, '请先选择文件'); return; }
  var fd = new FormData();
  fd.append('file', input.files[0]);
  fd.append('key', document.getElementById('upload-key').value.trim());
  showMsg('upload-msg', true, '上传中...');
  var r = await fetch('/dashboard/upload', { method: 'POST', body: fd });
  var j = await r.json();
  if (j.ok) {
    showMsg('upload-msg', true, '上传成功: ' + j.key + ' (' + fmtSize(j.size) + ')  SHA256: ' + j.sha256);
    loadFiles();
  } else showMsg('upload-msg', false, j.error || '上传失败');
}
function copyUrl(key) {
  var url = location.origin + '/download/' + encodeURIComponent(key);
  navigator.clipboard.writeText(url).then(function () { alert('已复制: ' + url); });
}
async function delFile(key) {
  if (!confirm('确定删除 ' + key + ' ?')) return;
  var r = await fetch('/dashboard/files/delete', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ key: key })
  });
  var j = await r.json();
  if (j.ok) loadFiles(); else alert(j.error || '删除失败');
}
async function changePassword() {
  var fd = new FormData();
  fd.append('old_password', document.getElementById('old_password').value);
  fd.append('new_password', document.getElementById('new_password').value);
  fd.append('confirm_password', document.getElementById('confirm_password').value);
  var r = await fetch('/dashboard/password', { method: 'POST', body: fd });
  var j = await r.json();
  if (j.ok) {
    showMsg('pw-msg', true, j.message || '已修改');
    setTimeout(function () { location.href = '/dashboard'; }, 1200);
  } else showMsg('pw-msg', false, j.error || '修改失败');
}
loadConfig();
</script>
</body>
</html>`;
  return new Response(html, {
    headers: { "Content-Type": "text/html; charset=utf-8", "Cache-Control": "no-store" },
  });
}

// ---------------------------------------------------------------- 路由入口

export default {
  async fetch(req: Request, env: Env, ctx: ExecutionContext): Promise<Response> {
    const url = new URL(req.url);
    const path = url.pathname;
    try {
      if (req.method === "OPTIONS") {
        return new Response(null, {
          headers: {
            "Access-Control-Allow-Origin": "*",
            "Access-Control-Allow-Methods": "GET,POST,OPTIONS",
            "Access-Control-Allow-Headers": "Content-Type",
          },
        });
      }
      // 云控配置(启动器读取, 无需鉴权)
      if (path === "/config" && req.method === "GET") return await handleConfig(env);

      // 安装包下载(启动器读取, 无需鉴权)
      if (path.startsWith("/download/")) return await handleDownload(env, path, ctx);

      // 面板
      if (path === "/dashboard" && req.method === "GET") {
        return (await isAuthed(env, req)) ? dashboardApp() : dashboardPage();
      }
      if (path === "/dashboard/login" && req.method === "POST") return await handleLogin(env, req);
      if (path === "/dashboard/logout" && req.method === "POST") return handleLogout();

      // 以下面板接口需要登录
      if (path.startsWith("/dashboard/")) {
        if (!(await isAuthed(env, req))) return json({ error: "unauthorized" }, 401);
        if (path === "/dashboard/config" && req.method === "POST") return await handleSaveConfig(env, req);
        if (path === "/dashboard/upload" && req.method === "POST") return await handleUpload(env, req);
        if (path === "/dashboard/files" && req.method === "GET") return await handleFiles(env);
        if (path === "/dashboard/files/delete" && req.method === "POST") return await handleDeleteFile(env, req);
        if (path === "/dashboard/password" && req.method === "POST") return await handleChangePassword(env, req);
      }

      if (path === "/" || path === "") {
        return new Response("CCL cloud control online. config: /config | dashboard: /dashboard | download: /download/<key>", {
          headers: { "Content-Type": "text/plain; charset=utf-8" },
        });
      }
      return json({ error: "not found: " + path }, 404);
    } catch (e) {
      return json({ error: String(e) }, 500);
    }
  },
};
