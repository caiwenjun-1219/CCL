import { useEffect, useState } from "react";
import { NavLink, Outlet, useLocation } from "react-router-dom";
import {
  House,
  Boxes,
  Download,
  Puzzle,
  Package,
  Coffee,
  User,
  Settings as SettingsIcon,
  X,
  Minus,
  Square,
  Copy,
  ArrowUpCircle,
  Bell,
} from "lucide-react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useApp } from "../store/app";
import { api, fmtSize, fmtSpeed, type UpdateInfo } from "../lib/tauri";

/** Tauri 上下文之外（浏览器里预览）getCurrentWindow() 会抛异常，这里兜住 */
const appWindow: (() => ReturnType<typeof getCurrentWindow>) | null = (() => {
  try {
    const w = getCurrentWindow();
    return () => w;
  } catch {
    return null;
  }
})();
const getWin = () => (appWindow ? appWindow() : null);

/** HMCL 的导航抽屉是单层扁平结构，按「实例 / 下载 / 其他」分组 */
const NAV_GROUPS: { cat: string; items: { to: string; label: string; icon: typeof House }[] }[] = [
  {
    cat: "实例",
    items: [
      { to: "/", label: "主页", icon: House },
      { to: "/instances", label: "版本管理", icon: Boxes },
      { to: "/mods", label: "模组管理", icon: Puzzle },
      { to: "/modpacks", label: "整合包", icon: Package },
    ],
  },
  {
    cat: "下载",
    items: [
      { to: "/versions", label: "游戏版本", icon: Download },
      { to: "/java", label: "Java 运行环境", icon: Coffee },
    ],
  },
  {
    cat: "其他",
    items: [
      { to: "/accounts", label: "游戏账户", icon: User },
      { to: "/settings", label: "启动器设置", icon: SettingsIcon },
    ],
  },
];

function Toolbar() {
  const [maximized, setMaximized] = useState(false);
  useEffect(() => {
    getWin()?.isMaximized().then(setMaximized).catch(() => {});
  }, []);
  return (
    <div className="hm-toolbar" data-tauri-drag-region>
      <div className="flex items-center gap-2 pl-3 pr-2" data-tauri-drag-region>
        <CclMark />
        <span className="hm-toolbar-title" data-tauri-drag-region>
          Cwj Craft Launcher
        </span>
      </div>
      <div className="flex-1 h-full" data-tauri-drag-region />
      <div className="flex items-center h-full">
        <button className="hm-winbtn" onClick={() => getWin()?.minimize()} title="最小化">
          <Minus size={14} />
        </button>
        <button
          className="hm-winbtn"
          onClick={() => getWin()?.toggleMaximize().then(() => setMaximized((v) => !v))}
          title={maximized ? "还原" : "最大化"}
        >
          {maximized ? <Copy size={11} /> : <Square size={10} />}
        </button>
        <button
          className="hm-winbtn hm-winbtn-close"
          onClick={() => getWin()?.close()}
          title="关闭"
        >
          <X size={15} />
        </button>
      </div>
    </div>
  );
}

/** 标题栏小图标：靛蓝圆角方块 + 白色立方体，仿 HMCL 的 icon-title */
function CclMark() {
  return (
    <div
      className="w-[18px] h-[18px] rounded-[4px] flex items-center justify-center shrink-0"
      style={{ background: "var(--hm-primary-container)" }}
    >
      <svg viewBox="0 0 24 24" width={12} height={12} fill="none" stroke="#F8F6FF" strokeWidth={1.9}>
        <path d="M12 3 20 7.5v9L12 21 4 16.5v-9z" strokeLinejoin="round" />
        <path d="M4 7.5 12 12l8-4.5M12 12v9" strokeLinejoin="round" />
      </svg>
    </div>
  );
}

function Drawer() {
  const { instances, running, accounts, update } = useApp();
  const [version, setVersion] = useState("");
  const runningCount = Object.values(running).filter(Boolean).length;
  const hasUpdate = !!update?.has_update;

  useEffect(() => {
    api.appVersion().then(setVersion).catch(() => {});
  }, []);

  return (
    <nav className="hm-drawer">
      <div className="hm-drawer-scroll">
        {NAV_GROUPS.map((g) => (
          <div key={g.cat}>
            <div className="hm-nav-cat">{g.cat}</div>
            {g.items.map(({ to, label, icon: Icon }) => (
              <NavLink
                key={to}
                to={to}
                end={to === "/"}
                className={({ isActive }) => `hm-nav-item${isActive ? " active" : ""}`}
              >
                <Icon size={17} strokeWidth={1.8} />
                <span className="truncate">{label}</span>
                {to === "/instances" && instances.length > 0 && (
                  <span className="hm-nav-count">{instances.length}</span>
                )}
                {to === "/" && runningCount > 0 && <span className="hm-nav-dot" />}
                {to === "/settings" && hasUpdate && (
                  <span className="hm-nav-count" style={{ background: "#ffdeac", color: "#5f4100" }}>
                    新
                  </span>
                )}
              </NavLink>
            ))}
          </div>
        ))}
      </div>

      <div className="hm-drawer-foot">
        <NavLink to="/accounts" className="hm-account-chip">
          <span className="hm-avatar">
            {accounts[0] ? accounts[0].name.slice(0, 1).toUpperCase() : "?"}
          </span>
          <span className="min-w-0 flex-1">
            <span className="block text-[12.5px] font-semibold truncate">
              {accounts[0] ? accounts[0].name : "未登录"}
            </span>
            <span className="block text-[10px] text-ccl-muted truncate">
              {accounts[0]
                ? accounts[0].account_type === "microsoft"
                  ? "微软账户"
                  : "离线账户"
                : "点击添加账户"}
            </span>
          </span>
        </NavLink>
        <div className="px-2.5 pt-1 pb-0.5 text-[10px] text-ccl-muted/70">
          {version ? `Cwj Craft Launcher v${version}` : ""}
        </div>
      </div>
    </nav>
  );
}

function DownloadPanel() {
  const downloads = useApp((s) => s.downloads);
  const list = Object.values(downloads).filter((d) => !d.finished);
  if (list.length === 0) return null;
  return (
    <div className="hm-dl-panel">
      <div className="flex items-center gap-1.5 text-[11px] font-semibold text-ccl-muted mb-1.5">
        <Download size={12} />
        正在下载 {list.length} 项
      </div>
      <div className="flex flex-col gap-1.5 max-h-28 overflow-auto">
        {list.map((d) => {
          const pct = d.total > 0 ? Math.floor((d.loaded / d.total) * 100) : 0;
          return (
            <div key={d.id} className="text-[11px]">
              <div className="flex justify-between mb-0.5 gap-2">
                <span className="truncate text-ccl-text">{d.name}</span>
                <span className="text-ccl-muted shrink-0">
                  {d.total > 0 ? `${fmtSize(d.loaded)} / ${fmtSize(d.total)}` : fmtSpeed(d.speed)}
                </span>
              </div>
              <div className="h-[3px] rounded-full overflow-hidden" style={{ background: "#e3e1e9" }}>
                <div
                  className="h-full transition-all"
                  style={{ width: `${pct}%`, background: "var(--hm-primary-container)" }}
                />
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}

function UpdateBanner({ update }: { update: UpdateInfo }) {
  const { showToast } = useApp();
  const [installing, setInstalling] = useState(false);
  const [dismissed, setDismissed] = useState(false);

  if (!update.has_update || dismissed || update.must_update) return null;

  const install = async () => {
    setInstalling(true);
    try {
      await api.installCloudUpdate(update.download_url, update.sha256);
    } catch (e) {
      showToast(String(e), "bad");
      setInstalling(false);
    }
  };

  return (
    <div
      className="flex items-center gap-3 px-4 py-2 text-[12px]"
      style={{ background: "#dee0ff", color: "#2f3f92" }}
    >
      <ArrowUpCircle size={15} className="shrink-0" />
      <span>
        发现新版本 <b>v{update.latest_version}</b>（当前 v{update.current_version}）
        {update.release_notes && (
          <span className="text-ccl-muted ml-2">{update.release_notes.split("\n")[0]}</span>
        )}
      </span>
      <div className="flex-1" />
      <button className="btn btn-primary !py-1 !px-3" onClick={install} disabled={installing}>
        {installing ? "正在下载…" : "立即更新"}
      </button>
      <button className="btn btn-ghost !py-1 !px-3" onClick={() => setDismissed(true)}>
        稍后
      </button>
    </div>
  );
}

function ForceUpdateModal({ update }: { update: UpdateInfo }) {
  const { showToast } = useApp();
  const [installing, setInstalling] = useState(false);
  const [progress, setProgress] = useState(0);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    import("@tauri-apps/api/event").then(({ listen }) =>
      listen<{ loaded: number; total: number }>("update-download", (e) => {
        if (e.payload.total > 0) {
          setProgress(Math.floor((e.payload.loaded / e.payload.total) * 100));
        }
      })
    ).then((u) => (unlisten = u));
    return () => unlisten?.();
  }, []);

  const install = async () => {
    setInstalling(true);
    try {
      await api.installCloudUpdate(update.download_url, update.sha256);
    } catch (e) {
      showToast(String(e), "bad");
      setInstalling(false);
    }
  };

  return (
    <div className="hm-dialog-mask">
      <div className="hm-dialog w-[420px] text-center">
        <ArrowUpCircle size={38} className="mx-auto mb-3" style={{ color: "var(--hm-primary)" }} />
        <h2 className="text-[17px] font-bold mb-2">必须更新才能继续使用</h2>
        <p className="text-[12.5px] text-ccl-muted mb-4 leading-relaxed">
          当前版本 v{update.current_version} 已低于最低支持版本 v{update.latest_version}
          <br />
          请更新到最新版本后重新打开启动器
        </p>
        {update.release_notes && (
          <pre className="text-left text-[11.5px] bg-white border border-ccl-border rounded-[4px] p-3 mb-4 whitespace-pre-wrap max-h-32 overflow-auto font-mono">
            {update.release_notes}
          </pre>
        )}
        {installing && (
          <div className="h-1 rounded-full overflow-hidden mb-3" style={{ background: "#e3e1e9" }}>
            <div
              className="h-full"
              style={{ width: `${progress}%`, background: "var(--hm-primary-container)" }}
            />
          </div>
        )}
        <button className="btn btn-primary w-full" onClick={install} disabled={installing}>
          {installing ? `下载中 ${progress}%` : "下载并安装更新"}
        </button>
      </div>
    </div>
  );
}

function AnnouncementCard({ update }: { update: UpdateInfo }) {
  const [dismissed, setDismissed] = useState(() => {
    const seen = localStorage.getItem("ccl-announcement-seen");
    return seen === `${update.announcement}|${update.updated_at}`;
  });
  if (!update.announcement || dismissed) return null;
  return (
    <div className="card flex items-start gap-2.5 mb-4 !p-3">
      <Bell size={14} className="mt-0.5 shrink-0" style={{ color: "var(--hm-primary)" }} />
      <div className="flex-1 whitespace-pre-wrap text-[12px] leading-relaxed">{update.announcement}</div>
      <button
        className="text-ccl-muted hover:text-ccl-text shrink-0"
        onClick={() => {
          localStorage.setItem("ccl-announcement-seen", `${update.announcement}|${update.updated_at}`);
          setDismissed(true);
        }}
      >
        <X size={14} />
      </button>
    </div>
  );
}

function Snackbar() {
  const toast = useApp((s) => s.toast);
  if (!toast) return null;
  return <div className={`hm-snackbar${toast.kind === "bad" ? " bad" : ""}`}>{toast.text}</div>;
}

export default function Layout() {
  const { settings, update, loadSettings, loadInstances, loadAccounts, checkUpdate, setDownload, setRunning, appendLog } =
    useApp();
  const location = useLocation();

  useEffect(() => {
    loadSettings();
    loadInstances();
    loadAccounts();
    checkUpdate();

    let unsubs: (() => void)[] = [];
    import("@tauri-apps/api/event").then(({ listen }) => {
      Promise.all([
        listen<{
          id: string;
          name: string;
          loaded: number;
          total: number;
          speed: number;
          finished: boolean;
          error: string | null;
        }>("download-progress", (e) => setDownload(e.payload)),
        listen<{ instance_id: string; line: string }>("game-log", (e) => appendLog(e.payload.instance_id, e.payload.line)),
        listen<{ instance_id: string; code: number | null }>("game-exit", (e) => setRunning(e.payload.instance_id, false)),
      ]).then((us) => (unsubs = us));
    });
    return () => unsubs.forEach((u) => u());
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    if (!settings.auto_check_update) return;
    const timer = setInterval(checkUpdate, 30 * 60 * 1000);
    return () => clearInterval(timer);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [settings.auto_check_update]);

  useEffect(() => {
    document.title = "Cwj Craft Launcher";
  }, []);

  return (
    <div className="hm-root">
      <Toolbar />
      {update?.must_update ? (
        <ForceUpdateModal update={update} />
      ) : (
        update && <UpdateBanner update={update} />
      )}
      <div className="flex-1 flex min-h-0">
        <Drawer />
        <main className="hm-content">
          <div className="hm-content-scroll" key={location.pathname}>
            {location.pathname === "/" && update?.cloud_online && <AnnouncementCard update={update} />}
            <Outlet />
          </div>
          <DownloadPanel />
        </main>
      </div>
      <Snackbar />
    </div>
  );
}
