import { useEffect, useState, useCallback } from "react";
import { useSearchParams, useNavigate } from "react-router-dom";
import {
  Save,
  RefreshCw,
  ArrowUpCircle,
  Loader2,
  KeyRound,
  FolderOpen,
  FolderPlus,
  HardDrive,
  Layers,
  Trash2,
} from "lucide-react";
import { open } from "@tauri-apps/plugin-dialog";
import { useApp } from "../store/app";
import { api, type GameFolder } from "../lib/tauri";

/** PCL CE 版本隔离五档（对应后端 LaunchArgumentIndieV2） */
const ISOLATION_OPTIONS: { value: number; label: string; desc: string }[] = [
  { value: 4, label: "隔离所有实例", desc: "推荐。每个实例都有独立的存档、模组、资源包与设置" },
  { value: 3, label: "非正式版与模组实例", desc: "快照版 / 远古版，以及 Fabric、Forge 等可装模组的实例" },
  { value: 2, label: "仅非正式版", desc: "只隔离快照版、远古版与愚人节版" },
  { value: 1, label: "仅可安装模组的实例", desc: "只隔离 Fabric / Forge / NeoForge / Quilt 类实例" },
  { value: 0, label: "全部关闭", desc: "所有实例共用 Minecraft 文件夹，与官方启动器行为一致" },
];

/** PCL 设置项行：左标题+说明，右控件，底部极淡分隔线 */
function Row({
  label,
  desc,
  children,
}: {
  label: string;
  desc?: string;
  children: React.ReactNode;
}) {
  return (
    <div className="pcl-setting-row flex items-center gap-5 py-2.5 border-b border-ccl-gray6 last:border-0">
      <div className="flex-1 min-w-0">
        <div className="text-[13px] text-ccl-gray1">{label}</div>
        {desc && <div className="text-[11.5px] text-ccl-gray3 mt-0.5 leading-relaxed">{desc}</div>}
      </div>
      <div className="shrink-0 flex items-center gap-2">{children}</div>
    </div>
  );
}

/** 分区小标题（PCL 的卡片标题样式） */
function Section({ icon, title }: { icon: React.ReactNode; title: string }) {
  return (
    <div className="flex items-center gap-1.5 pt-4 pb-1 mt-1 first:pt-0">
      <span className="text-ccl-accent">{icon}</span>
      <span className="text-[13px] font-bold text-ccl-gray1">{title}</span>
    </div>
  );
}

function Switch({ on, onChange }: { on: boolean; onChange: (v: boolean) => void }) {
  return <div className="pcl-switch" data-on={on} onClick={() => onChange(!on)} role="switch" aria-checked={on} />;
}

/**
 * Minecraft 文件夹选择（对齐 PCL CE 的「游戏文件夹」下拉）
 *
 * 目录模型：
 * ```text
 * <.minecraft>\versions\<实例名>\   实例文件夹（隔离时的游戏数据）
 * <.minecraft>\versions\<版本 id>\  共享版本文件
 * <.minecraft>\libraries  assets   共享库与资源
 * ```
 */
function GameFolderPanel() {
  const { settings, patchSettings, showToast, loadInstances } = useApp();
  const [folders, setFolders] = useState<GameFolder[]>([]);
  const [loading, setLoading] = useState(true);
  const [migrating, setMigrating] = useState(false);

  const current = settings.game_folder || "";

  const refresh = useCallback(async () => {
    setLoading(true);
    try {
      setFolders(await api.scanGameFolders());
    } catch (e) {
      showToast(String(e), "bad");
    } finally {
      setLoading(false);
    }
  }, [showToast]);

  useEffect(() => {
    refresh();
  }, [refresh]);

  const pick = async () => {
    const selected = await open({ directory: true, multiple: false });
    if (!selected || Array.isArray(selected)) return;
    try {
      await api.switchGameFolder(selected);
      await patchSettings({ game_folder: selected });
      showToast("已切换 Minecraft 文件夹");
      await refresh();
      await loadInstances();
    } catch (e) {
      showToast(String(e), "bad");
    }
  };

  const addCurrent = async () => {
    if (!current) return;
    const name = current.split(/[\\/]/).filter(Boolean).slice(-1)[0] || "自定义";
    try {
      await api.addCustomGameFolder(name, current);
      await refresh();
      showToast("已加入文件夹列表");
    } catch (e) {
      showToast(String(e), "bad");
    }
  };

  const removeFolder = async (path: string) => {
    try {
      await api.removeCustomGameFolder(path);
      await refresh();
    } catch (e) {
      showToast(String(e), "bad");
    }
  };

  const migrate = async () => {
    setMigrating(true);
    try {
      const n = await api.migrateInstancesToMcFolder();
      await loadInstances();
      showToast(n > 0 ? `已迁移 ${n} 个实例到 Minecraft 文件夹` : "没有需要迁移的实例");
    } catch (e) {
      showToast(String(e), "bad");
    } finally {
      setMigrating(false);
    }
  };

  return (
    <div className="mt-2 border border-ccl-gray6 rounded-[4px] p-3 bg-ccl-panel2/40">
      <div className="flex items-center gap-2">
        <HardDrive size={13} className="text-ccl-accent shrink-0" />
        <span className="text-[12.5px] font-bold">Minecraft 文件夹</span>
        {loading && <Loader2 size={12} className="animate-spin text-ccl-gray4" />}
        <button className="btn !h-[26px] !text-[11.5px] ml-auto" onClick={refresh} disabled={loading}>
          <RefreshCw size={11} /> 刷新
        </button>
      </div>

      <div className="flex items-center gap-2 mt-2.5">
        <select
          className="field !flex-1 !min-w-0"
          value={current}
          onChange={async (e) => {
            const v = e.target.value;
            if (!v) return;
            try {
              await api.switchGameFolder(v);
              await patchSettings({ game_folder: v });
              await loadInstances();
              showToast("已切换 Minecraft 文件夹");
            } catch (err) {
              showToast(String(err), "bad");
            }
          }}
        >
          {!current && <option value="">（默认）%APPDATA%\.minecraft</option>}
          {folders.map((f) => (
            <option key={f.location} value={f.location}>
              {f.name}
              {f.kind === "custom" ? "（自定义）" : ""} — {f.location}
            </option>
          ))}
          {current && !folders.some((f) => f.location === current) && (
            <option value={current}>{current}</option>
          )}
        </select>
        <button className="btn !h-[30px] shrink-0" onClick={pick}>
          <FolderPlus size={12} /> 浏览
        </button>
      </div>

      <div className="text-[11px] text-ccl-gray4 mt-1.5 font-mono break-all">
        {current || `%APPDATA%\\.minecraft（默认，尚未写入设置）`}
      </div>

      <div className="flex flex-wrap items-center gap-2 mt-2.5 pt-2.5 border-t border-ccl-gray6">
        <button className="btn !h-[26px] !text-[11.5px]" onClick={addCurrent} disabled={!current}>
          <FolderPlus size={11} /> 收藏当前文件夹
        </button>
        <button
          className="btn !h-[26px] !text-[11.5px]"
          onClick={migrate}
          disabled={migrating}
          title="把早期版本创建的实例（AppData\com.caiwenjun.ccl\instances）搬进 Minecraft 文件夹"
        >
          {migrating ? <Loader2 size={11} className="animate-spin" /> : <Layers size={11} />}
          迁移旧实例
        </button>
        <span className="text-[11px] text-ccl-gray4 ml-auto">
          切换文件夹后需重新加载实例列表
        </span>
      </div>

      {folders.filter((f) => f.kind === "custom").length > 0 && (
        <div className="mt-2 pt-2 border-t border-ccl-gray6">
          {folders
            .filter((f) => f.kind === "custom")
            .map((f) => (
              <div key={f.location} className="flex items-center gap-2 py-1">
                <span className="text-[11.5px] text-ccl-gray2 shrink-0">{f.name}</span>
                <span className="text-[11px] text-ccl-gray4 font-mono truncate flex-1">
                  {f.location}
                </span>
                <button
                  className="icon-danger shrink-0"
                  title="移除"
                  onClick={() => removeFolder(f.location)}
                >
                  <Trash2 size={12} />
                </button>
              </div>
            ))}
        </div>
      )}
    </div>
  );
}

export default function SettingsPage() {
  const [params] = useSearchParams();
  const tab = params.get("tab") || "launch";
  const navigate = useNavigate();
  const { settings, patchSettings, update, checkUpdate, showToast } = useApp();
  const [draft, setDraft] = useState(settings);
  const [checking, setChecking] = useState(false);
  const [installing, setInstalling] = useState(false);
  const [dlPct, setDlPct] = useState(0);

  useEffect(() => setDraft(settings), [settings]);

  const commit = (patch: Partial<typeof settings>) => patchSettings(patch);

  const check = async () => {
    setChecking(true);
    try {
      await checkUpdate();
      const u = useApp.getState().update;
      if (u?.has_update) showToast(`发现新版本 v${u.latest_version}`);
      else if (u) showToast(`当前已是最新版本 v${u.current_version}`);
    } finally {
      setChecking(false);
    }
  };

  const install = async () => {
    if (!update) return;
    setInstalling(true);
    setDlPct(0);
    try {
      const { listen } = await import("@tauri-apps/api/event");
      const un = await listen<{ loaded: number; total: number }>("update-download", (e) => {
        if (e.payload.total > 0) setDlPct(Math.floor((e.payload.loaded / e.payload.total) * 100));
      });
      await api.installCloudUpdate(update.download_url, update.sha256);
      un();
    } catch (e) {
      showToast(String(e), "bad");
      setInstalling(false);
    }
  };

  const pickJava = async () => {
    const selected = await open({
      multiple: false,
      filters: [{ name: "Java 可执行文件", extensions: ["exe"] }],
    });
    if (selected) {
      setDraft({ ...draft, java_path: selected as string });
      commit({ java_path: selected as string });
    }
  };

  const HEAD: Record<string, { title: string; sub: string }> = {
    launch: { title: "启动设置", sub: "内存、JVM 参数与 Java 运行环境" },
    ui: { title: "个性化", sub: "界面与窗口行为" },
    system: { title: "系统", sub: "下载源与第三方服务凭据" },
    other: { title: "其他", sub: "更新与关于信息" },
  };

  return (
    <div className="max-w-[760px]">
      <h1 className="pcl-title">{HEAD[tab]?.title || "设置"}</h1>
      <p className="pcl-subtitle">{HEAD[tab]?.sub}</p>

      {/* -------------------------------------------------- 启动 */}
      {tab === "launch" && (
        <div className="mt-5">
          <Section icon={<HardDrive size={13} />} title="游戏目录" />
          <div className="text-[11.5px] text-ccl-gray3 leading-relaxed mb-2">
            采用与 PCL CE 相同的目录结构：<span className="font-mono">.minecraft\versions\&lt;实例名&gt;</span>{" "}
            存放实例，<span className="font-mono">libraries</span> 与{" "}
            <span className="font-mono">assets</span> 全局共享。
          </div>
          <GameFolderPanel />

          <Row
            label="版本隔离"
            desc="决定实例是否拥有独立的存档、模组与设置。隔离时 --gameDir 指向实例文件夹，否则指向 Minecraft 文件夹根。"
          >
            <select
              className="field !w-[300px]"
              value={draft.version_isolation}
              onChange={(e) => commit({ version_isolation: Number(e.target.value) })}
            >
              {ISOLATION_OPTIONS.map((o) => (
                <option key={o.value} value={o.value}>
                  {o.label}
                </option>
              ))}
            </select>
          </Row>
          <div className="text-[11.5px] text-ccl-gray4 -mt-1 mb-1 pl-1">
            {ISOLATION_OPTIONS.find((o) => o.value === draft.version_isolation)?.desc}
          </div>

          <Section icon={<Layers size={13} />} title="运行环境" />
          <Row label="默认最大内存" desc="新建实例时使用，单位 MB。建议 2048 ~ 8192。">
            <input
              className="field !w-[120px] text-right"
              type="number"
              min={1024}
              step={512}
              value={draft.max_memory}
              onChange={(e) => setDraft({ ...draft, max_memory: Number(e.target.value) })}
              onBlur={() => commit({ max_memory: draft.max_memory })}
            />
            <span className="text-[12px] text-ccl-gray3">MB</span>
          </Row>
          <Row label="额外 JVM 参数" desc="追加到启动命令中，留空则使用默认 GC 策略。">
            <input
              className="field !w-[320px]"
              placeholder="-XX:+UseG1GC"
              value={draft.jvm_args}
              onChange={(e) => setDraft({ ...draft, jvm_args: e.target.value })}
              onBlur={() => commit({ jvm_args: draft.jvm_args })}
            />
          </Row>
          <Row label="Java 路径" desc="留空则按 MC 版本自动匹配本机 JDK。">
            <input
              className="field !w-[300px]"
              placeholder="自动选择"
              value={draft.java_path}
              onChange={(e) => setDraft({ ...draft, java_path: e.target.value })}
              onBlur={() => commit({ java_path: draft.java_path })}
            />
            <button className="btn !h-[30px]" onClick={pickJava}>
              <FolderOpen size={12} /> 浏览
            </button>
          </Row>
          <div className="mt-4 text-[11.5px] text-ccl-gray4 flex items-center gap-1.5">
            <Save size={12} />
            设置修改后即时生效并自动保存。
          </div>
        </div>
      )}

      {/* -------------------------------------------------- 个性化 */}
      {tab === "ui" && (
        <div className="mt-5">
          <Row label="界面主题" desc="当前跟随 PCL 经典蓝（#1370F3）。自定义主题色将在后续版本开放。">
            <div className="flex items-center gap-2">
              <span className="w-[18px] h-[18px] rounded-[3px]" style={{ background: "#1370f3" }} />
              <span className="text-[12.5px] text-ccl-gray2">PCL 蓝</span>
            </div>
          </Row>
          <Row label="关闭窗口时的行为" desc="最小化到托盘可让下载任务在后台继续。">
            <select
              className="field !w-[220px]"
              value={draft.close_action}
              onChange={(e) => commit({ close_action: e.target.value })}
            >
              <option value="minimize">最小化到托盘</option>
              <option value="exit">退出程序</option>
            </select>
          </Row>
          <Row label="启动时自动检查更新" desc="每 30 分钟也会静默检查一次云控配置。">
            <Switch on={draft.auto_check_update} onChange={(v) => commit({ auto_check_update: v })} />
          </Row>
        </div>
      )}

      {/* -------------------------------------------------- 系统 */}
      {tab === "system" && (
        <div className="mt-5">
          <Row label="资源下载源" desc="任何源失败都会自动逐级回退：首选 → 备用镜像 → 官方源。">
            <select
              className="field !w-[300px]"
              value={draft.download_source}
              onChange={(e) => commit({ download_source: e.target.value })}
            >
              <option value="bmclapi">BMCLAPI 国内镜像（推荐）</option>
              <option value="mojang">Mojang 官方源</option>
            </select>
          </Row>
          <Row label="CurseForge API Key" desc="用于 CurseForge 搜索与整合包导入；Modrinth 无需密钥。">
            <div className="flex items-center gap-2">
              <KeyRound size={13} className="text-ccl-gray4" />
              <input
                className="field !w-[300px]"
                placeholder="在 console.curseforge.com 申请"
                value={draft.curseforge_api_key}
                onChange={(e) => setDraft({ ...draft, curseforge_api_key: e.target.value })}
                onBlur={() => commit({ curseforge_api_key: draft.curseforge_api_key })}
              />
            </div>
          </Row>
          <Row
            label="微软应用 client_id"
            desc="留空 = 使用官方启动器（需手动粘贴授权码）。填入自建 Azure 应用的 client_id 可使用设备码登录，体验更顺。"
          >
            <div className="flex items-center gap-2">
              <KeyRound size={13} className="text-ccl-gray4" />
              <input
                className="field !w-[300px]"
                placeholder="留空使用官方 client_id"
                value={draft.microsoft_client_id ?? ""}
                onChange={(e) => setDraft({ ...draft, microsoft_client_id: e.target.value })}
                onBlur={() => commit({ microsoft_client_id: draft.microsoft_client_id ?? "" })}
              />
            </div>
          </Row>
        </div>
      )}

      {/* -------------------------------------------------- 其他 */}
      {tab === "other" && (
        <div className="mt-5">
          <div className="pcl-section">云控与更新</div>
          <Row label="当前版本">
            <span className="text-[13px] font-semibold">v{update?.current_version || "…"}</span>
          </Row>
          <Row label="最新版本">
            <span className={`text-[13px] font-semibold ${update?.has_update ? "text-ccl-accent" : ""}`}>
              v{update?.latest_version || "…"}
            </span>
          </Row>
          <Row label="云控服务" desc="cclconfig.caiwenjun.top">
            <span className={`text-[12.5px] ${update?.cloud_online ? "text-[#2e7d46]" : "text-ccl-gray3"}`}>
              {update?.cloud_online ? "在线" : "离线（不影响使用）"}
            </span>
          </Row>
          <div className="flex gap-2 mt-5">
            <button className="btn" onClick={check} disabled={checking}>
              {checking ? <Loader2 size={12} className="animate-spin" /> : <RefreshCw size={12} />} 检查更新
            </button>
            {update?.has_update && (
              <button className="btn btn-primary" onClick={install} disabled={installing}>
                {installing ? (
                  <>
                    <Loader2 size={12} className="animate-spin" /> {dlPct}%
                  </>
                ) : (
                  <>
                    <ArrowUpCircle size={12} /> 更新到 v{update.latest_version}
                  </>
                )}
              </button>
            )}
            <button className="btn ml-auto" onClick={() => navigate("/more/about")}>
              关于 CCL
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
