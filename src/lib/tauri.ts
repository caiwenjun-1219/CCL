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
  /** 自建 Azure 应用 client_id（留空 = 用官方启动器 client_id） */
  microsoft_client_id: string;
  /** Minecraft 文件夹（.minecraft 根），空 = %APPDATA%\.minecraft */
  game_folder: string;
  /** 版本隔离策略 0 关闭 / 1 仅模组实例 / 2 仅非正式版 / 3 两者 / 4 全部 */
  version_isolation: number;
  /** 自定义文件夹列表，格式「名称>路径」 */
  custom_folders: string[];
  /** 网络代理："" = 直连；"auto" = 自动读系统代理；否则 "http://host:port" / "socks5://host:port" */
  proxy: string;
  /** GitHub 加速前缀（Android JRE 等 Release 资产用）；空 = 自动多源回退 */
  github_mirror: string;
}

/** 可选的 Minecraft 文件夹 */
export interface GameFolder {
  name: string;
  location: string;
  kind: "original" | "custom";
}

/** 实例的路径信息（PCL CE 风格） */
export interface InstancePaths {
  mc_root: string;
  game_dir: string;
  instance_folder: string | null;
  isolation_label: string;
  isolated: boolean;
  legacy: boolean;
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
  /** 版本设置（对齐 PCL：按实例独立） */
  min_memory?: number | null;
  java_path?: string | null;
  jvm_args?: string | null;
  game_args?: string | null;
  window_width?: number | null;
  window_height?: number | null;
  fullscreen?: boolean;
  auto_join_server?: string | null;
  last_played?: string | null;
  /** 新布局：在 <.minecraft>\versions\ 下的文件夹名；null = 旧布局 */
  folder_name?: string | null;
  /** 版本隔离覆盖：null/-1 跟随全局，1 强制隔离，2 强制不隔离 */
  version_isolation?: number | null;
}

/** 存档 */
export interface SaveInfo {
  dir_name: string;
  level_name: string;
  size: number;
  mtime: number;
  /** data:image/png;base64,... */
  icon: string | null;
}

/** 资源包 / 光影包 */
export interface PackInfo {
  file_name: string;
  display_name: string;
  description: string;
  size: number;
  mtime: number;
  enabled: boolean;
  is_dir: boolean;
  incompatible: boolean;
}

export interface LogFile {
  name: string;
  size: number;
  mtime: number;
}

export interface ModInfo {
  file_name: string;
  enabled: boolean;
  size: number;
  project_name: string;
  /** jar 内声明的模组名（fabric/quilt/forge/neoforge 元数据） */
  mod_name?: string | null;
  mod_version?: string | null;
  mod_id?: string | null;
}

export interface JavaInfo {
  path: string;
  major: number;
  version: string;
  vendor: string;
  managed: boolean;
}

/** Java 运行环境信息（桌面 / Android 由后端区分） */
export interface JavaEnv {
  platform: "desktop" | "android";
  supported_majors: number[];
  source: string;
  is_android_jre: boolean;
}

export interface Account {
  uuid: string;
  name: string;
  access_token: string;
  refresh_token: string;
  account_type: string;
  expires_at: number;
}

/**
 * Rust 端 `LoginPoll` 枚举的 JSON 形状。
 * 注意：Pending 是单元变体，serde 会序列化成**字符串** "Pending"（不是对象），
 * 所以判断时必须先确认是对象，不能直接用 `"Done" in res`（对字符串用 in 会抛 TypeError）。
 */
export type LoginPollResult = "Pending" | { Done: Account } | { Failed: string };

/** 登录轮询结果 → 成功账户（非成功时返回 null） */
export function pollAccount(r: LoginPollResult): Account | null {
  return typeof r === "object" && r !== null && "Done" in r ? r.Done : null;
}

/** 登录轮询结果 → 失败原因（未失败时返回 null） */
export function pollError(r: LoginPollResult): string | null {
  return typeof r === "object" && r !== null && "Failed" in r ? r.Failed : null;
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

/** 推荐位项目卡片：比 ModHit 多带关注数 / 类型 / 更新时间 / 主题色 */
export interface ProjectHit {
  project_id: string;
  project_type: string;
  slug: string;
  title: string;
  description: string;
  icon_url: string | null;
  categories: string[];
  versions: string[];
  downloads: number;
  follows: number;
  author: string;
  date_modified: string;
  color: number | null;
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
  javaEnv: () => invoke<JavaEnv>("java_env"),

  // Android 存储权限（MANAGE_EXTERNAL_STORAGE）：检测与申请
  checkStoragePermission: () =>
    invoke<{ granted: boolean }>("plugin:ccl-android|check_storage_permission"),
  requestStoragePermission: () =>
    invoke<{ granted: boolean }>("plugin:ccl-android|request_storage_permission"),

  // Minecraft 版本
  versionManifest: () => invoke<ManifestEntry[]>("mc_version_manifest"),
  testVersion: (versionId: string) =>
    invoke<VersionTestReport>("mc_test_version", { versionId }),
  localVersions: (gameDir: string) =>
    invoke<string[]>("mc_local_versions", { gameDir }),
  installVersion: (gameDir: string, versionId: string) =>
    invoke<string>("mc_install_version", { gameDir, versionId }),
  /// 下载本体并自动创建同名实例（失败自动回滚实例目录）。
  /// 可选 loader / loaderVersion：建实例的同时一并安装模组加载器（对齐 PCL）。
  downloadAsInstance: (
    versionId: string,
    name?: string | null,
    maxMemory?: number | null,
    loader?: string | null,
    loaderVersion?: string | null
  ) =>
    invoke<Instance>("mc_download_as_instance", {
      versionId,
      name: name ?? null,
      maxMemory: maxMemory ?? null,
      loader: loader ?? null,
      loaderVersion: loaderVersion ?? null,
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
      loader_version?: string | null;
      loader?: string;
      min_memory?: number;
      java_path?: string;
      jvm_args?: string;
      game_args?: string;
      window_width?: number;
      window_height?: number;
      fullscreen?: boolean;
      auto_join_server?: string;
      /** 0 跟随全局 / 1 强制隔离 / 2 强制不隔离 */
      version_isolation?: number;
    }
  ) => invoke<Instance>("update_instance", { id, patch }),
  deleteInstance: (id: string, removeFiles = true) =>
    invoke("delete_instance", { id, removeFiles }),
  openInstanceDir: (id: string) => invoke("open_instance_dir", { id }),

  // ---- PCL CE 风格：游戏目录与版本隔离
  scanGameFolders: () => invoke<GameFolder[]>("scan_game_folders"),
  /** 当前 Minecraft 文件夹绝对路径（版本/库/资源都在这里） */
  getMcFolder: () => invoke<string>("get_mc_folder"),
  switchGameFolder: (path: string) => invoke<string>("switch_game_folder", { path }),
  addCustomGameFolder: (name: string, path: string) =>
    invoke<string[]>("add_custom_game_folder", { name, path }),
  removeCustomGameFolder: (path: string) =>
    invoke<string[]>("remove_custom_game_folder", { path }),
  migrateInstancesToMcFolder: () => invoke<number>("migrate_instances_to_mc_folder"),
  instancePaths: (id: string) => invoke<InstancePaths>("instance_paths", { id }),
  listScreenshots: (instanceId: string) =>
    invoke<string[]>("list_screenshots", { instanceId }),

  // 实例资源：存档 / 资源包 / 光影 / 日志
  instanceSize: (instanceId: string) => invoke<number>("instance_size", { instanceId }),
  openGameSubdir: (instanceId: string, sub: string) =>
    invoke("open_game_subdir", { instanceId, sub }),
  listSaves: (instanceId: string) => invoke<SaveInfo[]>("list_saves", { instanceId }),
  deleteSave: (instanceId: string, dirName: string) =>
    invoke("delete_save", { instanceId, dirName }),
  backupSave: (instanceId: string, dirName: string) =>
    invoke<string>("backup_save", { instanceId, dirName }),
  listResourcepacks: (instanceId: string) =>
    invoke<PackInfo[]>("list_resourcepacks", { instanceId }),
  toggleResourcepack: (instanceId: string, fileName: string, enabled: boolean) =>
    invoke("toggle_resourcepack", { instanceId, fileName, enabled }),
  deleteResourcepack: (instanceId: string, fileName: string) =>
    invoke("delete_resourcepack", { instanceId, fileName }),
  listShaderpacks: (instanceId: string) =>
    invoke<PackInfo[]>("list_shaderpacks", { instanceId }),
  setShaderpack: (instanceId: string, name: string) =>
    invoke("set_shaderpack", { instanceId, name }),
  deleteShaderpack: (instanceId: string, fileName: string) =>
    invoke("delete_shaderpack", { instanceId, fileName }),
  importPackFile: (instanceId: string, kind: "resourcepack" | "shaderpack", srcPath: string) =>
    invoke<string>("import_pack_file", { instanceId, kind, srcPath }),
  listGameLogs: (instanceId: string) => invoke<LogFile[]>("list_game_logs", { instanceId }),
  readGameLog: (instanceId: string, name: string, tail = 400) =>
    invoke<string>("read_game_log", { instanceId, name, tail }),

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
      mode: string;
      user_code: string;
      device_code: string;
      verification_uri: string;
      interval: number;
      login_url: string;
      expires_in: number;
    }>("ms_login_start"),
  msLoginPoll: (deviceCode: string) =>
    invoke<LoginPollResult>("ms_login_poll", { deviceCode }),
  msLoginExchange: (code: string) => invoke<LoginPollResult>("ms_login_exchange", { code }),
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
  /** 推荐位：projectType 为 mod / modpack / resourcepack / shader，category 传 Modrinth 分类 slug */
  modrinthRecommend: (
    projectType: "mod" | "modpack" | "resourcepack" | "shader" | "datapack",
    category: string | null,
    gameVersion: string | null,
    loader: string | null,
    index: "downloads" | "follows" | "newest" | "updated" = "downloads",
    offset = 0,
    limit = 12
  ) =>
    invoke<ProjectHit[]>("modrinth_recommend", {
      projectType,
      category,
      gameVersion,
      loader,
      index,
      offset,
      limit,
    }),
  /** 在线安装整合包：下载 .mrpack 并走完整导入流程 */
  modrinthInstallModpack: (slug: string, versionId: string | null = null) =>
    invoke<Instance>("modrinth_install_modpack", { slug, versionId }),
  /** 在线安装资源包 / 光影包到实例对应目录 */
  modrinthInstallResource: (
    instanceId: string,
    kind: "resourcepack" | "shader",
    downloadUrl: string,
    filename: string,
    sha1: string | null
  ) =>
    invoke<string>("modrinth_install_resource", {
      instanceId,
      kind,
      downloadUrl,
      filename,
      sha1,
    }),
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

  // 网络代理
  /** 探测到的系统代理地址（无则 null） */
  proxyDetect: () => invoke<string | null>("proxy_detect"),
  /** 走当前代理配置做一次连通性测试，返回耗时与状态描述 */
  proxyTest: (url?: string | null) => invoke<string>("proxy_test", { url: url ?? null }),
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
