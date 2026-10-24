// 与 Rust 后端对应的类型与命令封装

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export interface AppSettings {
  download_source: string;
  max_memory: number;
  jvm_args: string;
  auto_check_update: boolean;
  close_action: string;
  curseforge_api_key: string;
  java_path: string;
  window_width: number | null;
  window_height: number | null;
}

export interface Instance {
  id: string;
  name: string;
  minecraft_version: string;
  loader: string;
  loader_version: string | null;
  installed_version_id: string | null;
  created_at: string;
  max_memory: number | null;
  game_dir: string;
}

export interface ModInfo {
  file_name: string;
  enabled: boolean;
  size: number;
  project_name: string;
}

export interface JavaInfo {
  path: string;
  major: number;
  version: string;
  vendor: string;
  managed: boolean;
}

export interface Account {
  uuid: string;
  name: string;
  access_token: string;
  refresh_token: string;
  account_type: string;
  expires_at: number;
}

export interface UpdateInfo {
  current_version: string;
  latest_version: string;
  has_update: boolean;
  must_update: boolean;
  download_url: string;
  sha256: string;
  release_notes: string;
  announcement: string;
  updated_at: string;
  cloud_online: boolean;
}

export interface DlProgress {
  id: string;
  name: string;
  loaded: number;
  total: number;
  speed: number;
  finished: boolean;
  error: string | null;
}

export interface GameLogEvent {
  instance_id: string;
  line: string;
}

export interface GameExitEvent {
  instance_id: string;
  code: number | null;
}

export interface ModHit {
  project_id: string;
  slug: string;
  title: string;
  description: string;
  icon_url: string | null;
  categories: string[];
  versions: string[];
  downloads: number;
  author: string;
}

export interface CfModHit {
  id: number;
  name: string;
  summary: string;
  icon_url: string | null;
  download_count: number;
  date_modified: string;
}

export interface ManifestEntry {
  id: string;
  vtype: string;
  url: string;
  /** 版本 JSON 最后修改时间（ISO 8601） */
  time: string;
  /** 正式发布时间（ISO 8601） */
  release_time: string;
}

export interface TestStep {
  name: string;
  ok: boolean;
  detail: string;
}

export interface VersionTestReport {
  version_id: string;
  ok: boolean;
  steps: TestStep[];
  /** 测试过程累计下载体积（字节） */
  total_size: number;
  /** 耗时（毫秒） */
  elapsed_ms: number;
  /** 测试用的隔离目录（失败时保留以便排查） */
  test_dir: string;
}

export const api = {
  // 设置
  getSettings: () => invoke<AppSettings>("get_settings"),
  saveSettings: (settings: AppSettings) =>
    invoke("save_settings", { settings }),

  // Java
  scanJavas: () => invoke<JavaInfo[]>("scan_javas"),
  downloadJava: (major: number) => invoke<JavaInfo>("download_java", { major }),
  resolveJava: (mcVersion: string) =>
    invoke<JavaInfo>("resolve_java", { mcVersion }),

  // Minecraft 版本
  versionManifest: () => invoke<ManifestEntry[]>("mc_version_manifest"),
  testVersion: (versionId: string) =>
    invoke<VersionTestReport>("mc_test_version", { versionId }),
  localVersions: (gameDir: string) =>
    invoke<string[]>("mc_local_versions", { gameDir }),
  installVersion: (gameDir: string, versionId: string) =>
    invoke<string>("mc_install_version", { gameDir, versionId }),
  /// 下载本体并自动创建同名实例（失败自动回滚实例目录）
  downloadAsInstance: (versionId: string, name?: string | null, maxMemory?: number | null) =>
    invoke<Instance>("mc_download_as_instance", {
      versionId,
      name: name ?? null,
      maxMemory: maxMemory ?? null,
    }),
  deleteVersion: (gameDir: string, versionId: string) =>
    invoke("mc_delete_version", { gameDir, versionId }),
  loaderVersions: (kind: string, mcVersion: string) =>
    invoke<string[]>("loader_versions", { kind, mcVersion }),
  installLoader: (gameDir: string, mcVersion: string, kind: string, loaderVersion: string) =>
    invoke<string>("mc_install_loader", { gameDir, mcVersion, kind, loaderVersion }),
  forgeVersions: (kind: string, mcVersion: string) =>
    invoke<string[]>("forge_versions", { kind, mcVersion }),
  installForge: (gameDir: string, mcVersion: string, kind: string, loaderVersion: string) =>
    invoke<string>("mc_install_forge", { gameDir, mcVersion, kind, loaderVersion }),

  // 实例
  listInstances: () => invoke<Instance[]>("list_instances"),
  createInstance: (
    name: string,
    minecraftVersion: string,
    loader: string,
    loaderVersion: string | null,
    maxMemory: number | null
  ) =>
    invoke<Instance>("create_instance", {
      name,
      minecraftVersion,
      loader,
      loaderVersion,
      maxMemory,
    }),
  updateInstance: (
    id: string,
    patch: {
      name?: string;
      max_memory?: number;
      installed_version_id?: string;
      loader_version?: string;
      loader?: string;
    }
  ) => invoke<Instance>("update_instance", { id, patch }),
  deleteInstance: (id: string) => invoke("delete_instance", { id }),
  openInstanceDir: (id: string) => invoke("open_instance_dir", { id }),
  listScreenshots: (instanceId: string) =>
    invoke<string[]>("list_screenshots", { instanceId }),

  // 模组
  listMods: (instanceId: string) => invoke<ModInfo[]>("list_mods", { instanceId }),
  toggleMod: (instanceId: string, fileName: string, enabled: boolean) =>
    invoke("toggle_mod", { instanceId, fileName, enabled }),
  deleteMod: (instanceId: string, fileName: string) =>
    invoke("delete_mod", { instanceId, fileName }),
  installModFile: (instanceId: string, srcPath: string) =>
    invoke<string>("install_mod_file", { instanceId, srcPath }),
  openModsDir: (instanceId: string) => invoke("open_mods_dir", { instanceId }),

  // 启动
  launchInstance: (instanceId: string, accountUuid: string | null) =>
    invoke<{ instance_id: string; version_id: string; java_major: number }>(
      "launch_instance",
      { instanceId, accountUuid }
    ),
  stopInstance: (instanceId: string) => invoke("stop_instance", { instanceId }),
  isInstanceRunning: (instanceId: string) =>
    invoke<boolean>("is_instance_running", { instanceId }),

  // 账户
  accountsList: () => invoke<Account[]>("accounts_list"),
  removeAccount: (uuid: string) => invoke("remove_account", { uuid }),
  msLoginStart: () =>
    invoke<{
      login_url: string;
      redirect_uri: string;
      expires_in: number;
    }>("ms_login_start"),
  msLoginPoll: () =>
    invoke<{ Pending?: never } | { Done: Account } | { Failed: string }>("ms_login_poll"),
  msLoginExchange: (code: string, redirectUri: string) =>
    invoke<{ Pending?: never } | { Done: Account } | { Failed: string }>("ms_login_exchange", {
      code,
      redirectUri,
    }),
  offlineLogin: (name: string) => invoke<Account>("offline_login", { name }),
  refreshAccount: (uuid: string) => invoke<Account>("refresh_account", { uuid }),

  // 模组市场
  modrinthSearch: (query: string, gameVersion: string | null, loader: string | null, offset = 0) =>
    invoke<ModHit[]>("modrinth_search", { query, gameVersion, loader, offset }),
  modrinthVersions: (slug: string, gameVersion: string | null, loader: string | null) =>
    invoke<Record<string, unknown>[]>("modrinth_versions", { slug, gameVersion, loader }),
  modrinthInstall: (
    instanceId: string,
    downloadUrl: string,
    filename: string,
    sha1: string | null
  ) => invoke<string>("modrinth_install", { instanceId, downloadUrl, filename, sha1 }),
  cfSearch: (query: string, gameVersion: string | null, offset = 0) =>
    invoke<CfModHit[]>("cf_search", { query, gameVersion, offset }),
  cfFiles: (modId: number, gameVersion: string | null) =>
    invoke<Record<string, unknown>[]>("cf_files", { modId, gameVersion }),
  cfInstall: (instanceId: string, downloadUrl: string, filename: string) =>
    invoke<string>("cf_install", { instanceId, downloadUrl, filename }),

  // 整合包
  importModpack: (path: string, nameOverride: string | null) =>
    invoke<Instance>("import_modpack", { path, nameOverride }),
  exportModpack: (instanceId: string, destPath: string) =>
    invoke<string>("export_modpack", { instanceId, destPath }),

  // 云控更新
  appVersion: () => invoke<string>("app_version"),
  checkCloudUpdate: () => invoke<UpdateInfo>("check_cloud_update"),
  installCloudUpdate: (downloadUrl: string, sha256: string) =>
    invoke("install_cloud_update", { downloadUrl, sha256 }),
};

// ---------------------------------------------------------------- 事件订阅

export function onDownloadProgress(cb: (p: DlProgress) => void) {
  return listen<DlProgress>("download-progress", (e) => cb(e.payload));
}

export function onGameLog(cb: (e: GameLogEvent) => void) {
  return listen<GameLogEvent>("game-log", (e) => cb(e.payload));
}

export function onGameExit(cb: (e: GameExitEvent) => void) {
  return listen<GameExitEvent>("game-exit", (e) => cb(e.payload));
}

export function onInstallProgress(cb: (p: { phase: string; done: number; total: number }) => void) {
  return listen<{ phase: string; done: number; total: number }>(
    "mc-install-progress",
    (e) => cb(e.payload)
  );
}

export function onTestProgress(
  cb: (p: { version_id: string; phase: string; done: number; total: number }) => void
) {
  return listen<{ version_id: string; phase: string; done: number; total: number }>(
    "mc-test-progress",
    (e) => cb(e.payload)
  );
}

export function onModpackProgress(cb: (p: { phase: string; done: number; total: number }) => void) {
  return listen<{ phase: string; done: number; total: number }>(
    "modpack-progress",
    (e) => cb(e.payload)
  );
}

export function onUpdateDownload(cb: (p: { loaded: number; total: number }) => void) {
  return listen<{ loaded: number; total: number }>("update-download", (e) => cb(e.payload));
}

/**
 * 构造 CCL 自定义协议 URL（本地文件服务，见 src-tauri/src/protocol.rs）。
 * Windows 上自定义协议经 wry workaround 映射为 http://<协议名>.localhost/...，
 * 因此前端统一用 http://ccl.localhost/<路径> 访问（Rust 侧收到的是 ccl://localhost/...）。
 * 支持两类路径：
 *   cclUrl(`instances/${instanceId}/screenshots/x.png`)  → 实例游戏目录
 *   cclUrl(`data/settings.json`)                        → CCL 数据目录
 * 传入的 fileName 等片段建议先 encodeURIComponent。
 */
export function cclUrl(path: string): string {
  return `http://ccl.localhost/${path.replace(/^\/+/, "")}`;
}

export function fmtSize(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1048576) return `${(n / 1024).toFixed(1)} KB`;
  if (n < 1073741824) return `${(n / 1048576).toFixed(2)} MB`;
  return `${(n / 1073741824).toFixed(2)} GB`;
}

export function fmtSpeed(n: number): string {
  if (n < 1024) return `${n} B/s`;
  if (n < 1048576) return `${(n / 1024).toFixed(0)} KB/s`;
  return `${(n / 1048576).toFixed(1)} MB/s`;
}
