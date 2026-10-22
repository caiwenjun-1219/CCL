import { useEffect, useState } from "react";
import { NavLink, Outlet, useLocation } from "react-router-dom";
import {
  Home,
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
  Check,
  ArrowUpCircle,
  Bell,
} from "lucide-react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useApp } from "../store/app";
import { api, fmtSize, fmtSpeed, type UpdateInfo } from "../lib/tauri";

const appWindow = getCurrentWindow();

const NAV = [
  { to: "/", label: "首页", icon: Home },
  { to: "/instances", label: "实例", icon: Boxes },
  { to: "/versions", label: "版本", icon: Download },
  { to: "/mods", label: "模组", icon: Puzzle },
  { to: "/modpacks", label: "整合包", icon: Package },
  { to: "/java", label: "Java", icon: Coffee },
  { to: "/accounts", label: "账户", icon: User },
  { to: "/settings", label: "设置", icon: SettingsIcon },
];

function TitleBar() {
  const [maximized, setMaximized] = useState(false);
  useEffect(() => {
    appWindow.isMaximized().then(setMaximized).catch(() => {});
  }, []);
  return (
    <div
      data-tauri-drag-region
      className="flex items-center h-11 shrink-0 bg-ccl-panel border-b border-ccl-border select-none"
    >
      <div className="flex items-center gap-2 px-4" data-tauri-drag-region>
        <div className="w-5 h-5 rounded bg-gradient-to-br from-[#5db95f] to-[#3c8c3f] flex items-center justify-center text-[8px] font-extrabold text-[#0f1420]">
          CCL
        </div>
        <span className="text-xs text-ccl-muted font-semibold" data-tauri-drag-region>
          Cwj Craft Launcher
        </span>
      </div>
      <div className="flex-1" data-tauri-drag-region />
      <div className="flex items-center h-full">
        <button
          className="h-full px-3.5 hover:bg-white/5 text-ccl-muted"
          onClick={() => appWindow.minimize()}
        >
          <Minus size={14} />
        </button>
        <button
          className="h-full px-3.5 hover:bg-white/5 text-ccl-muted"
          onClick={() => appWindow.toggleMaximize().then(() => setMaximized(!maximized))}
        >
          {maximized ? <Copy size={12} /> : <Square size={11} />}
        </button>
        <button
          className="h-full px-3.5 hover:bg-red-600/80 text-ccl-muted hover:text-white"
          onClick={() => appWindow.close()}
        >
          <X size={14} />
        </button>
      </div>
    </div>
  );
}

function Sidebar() {
  const { instances, running } = useApp();
  const [version, setVersion] = useState("");
  const runningCount = Object.values(running).filter(Boolean).length;

  useEffect(() => {
    api.appVersion().then(setVersion).catch(() => {});
  }, []);

  return (
    <nav className="w-44 shrink-0 bg-ccl-panel border-r border-ccl-border flex flex-col py-3">
      {NAV.map(({ to, label, icon: Icon }) => (
        <NavLink
          key={to}
          to={to}
          end={to === "/"}
          className={({ isActive }) =>
            `flex items-center gap-2.5 mx-2 px-3 py-2 rounded-lg text-[13px] font-medium transition-colors ${
              isActive
                ? "bg-[#2f6b3a] text-white"
                : "text-ccl-muted hover:bg-white/5 hover:text-ccl-text"
            }`
          }
        >
          <Icon size={15} />
          {label}
          {to === "/instances" && instances.length > 0 && (
            <span className="ml-auto text-[10px] text-ccl-muted">{instances.length}</span>
          )}
          {to === "/" && runningCount > 0 && (
            <span className="ml-auto w-2 h-2 rounded-full bg-ccl-accent animate-pulse" />
          )}
        </NavLink>
      ))}
      <div className="flex-1" />
      <div className="mx-3 mb-1 text-[10px] text-ccl-muted/60 leading-relaxed">
        {version ? `v${version}` : ""}
      </div>
    </nav>
  );
}

function DownloadDock() {
  const downloads = useApp((s) => s.downloads);
  const list = Object.values(downloads).filter((d) => !d.finished);
  if (list.length === 0) return null;
  return (
    <div className="shrink-0 border-t border-ccl-border bg-ccl-panel px-4 py-2">
      <div className="flex items-center gap-2 text-[11px] text-ccl-muted mb-1.5">
        <Download size={12} />
        下载任务 {list.length}
      </div>
      <div className="grid grid-cols-2 gap-x-6 gap-y-1.5 max-h-28 overflow-auto">
        {list.map((d) => {
          const pct = d.total > 0 ? Math.floor((d.loaded / d.total) * 100) : 0;
          return (
            <div key={d.id} className="text-[11px]">
              <div className="flex justify-between mb-0.5">
                <span className="truncate max-w-[220px] text-ccl-text">{d.name}</span>
                <span className="text-ccl-muted shrink-0">
                  {d.total > 0 ? `${fmtSize(d.loaded)} / ${fmtSize(d.total)}` : fmtSpeed(d.speed)}
                </span>
              </div>
              <div className="h-1 bg-[#232c42] rounded overflow-hidden">
                <div
                  className="h-full bg-ccl-accent transition-all"
                  style={{ width: `${pct}%` }}
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
    <div className="flex items-center gap-3 bg-[#2f6b3a]/25 border-b border-[#3f8a4d]/40 px-4 py-2 text-[12px]">
      <ArrowUpCircle size={15} className="text-ccl-accent2 shrink-0" />
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
    <div className="fixed inset-0 bg-black/70 flex items-center justify-center z-50">
      <div className="card w-[420px] text-center">
        <ArrowUpCircle size={40} className="mx-auto text-ccl-accent2 mb-3" />
        <h2 className="text-lg font-bold mb-2">必须更新才能继续使用</h2>
        <p className="text-[13px] text-ccl-muted mb-4">
          当前版本 v{update.current_version} 已低于最低支持版本 v{update.latest_version}
          <br />
          请更新到最新版本后重新打开启动器
        </p>
        {update.release_notes && (
          <pre className="text-left text-[12px] bg-ccl-bg border border-ccl-border rounded-lg p-3 mb-4 whitespace-pre-wrap max-h-32 overflow-auto">
            {update.release_notes}
          </pre>
        )}
        {installing && (
          <div className="h-1.5 bg-[#232c42] rounded overflow-hidden mb-3">
            <div className="h-full bg-ccl-accent" style={{ width: `${progress}%` }} />
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
    <div className="flex items-start gap-2.5 bg-[#223052]/40 border border-[#2f4470] rounded-xl px-4 py-3 text-[12px] mb-4">
      <Bell size={14} className="text-[#9fb4ff] mt-0.5 shrink-0" />
      <div className="flex-1 whitespace-pre-wrap">{update.announcement}</div>
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

function Toast() {
  const toast = useApp((s) => s.toast);
  if (!toast) return null;
  return (
    <div
      className={`fixed bottom-16 left-1/2 -translate-x-1/2 px-4 py-2 rounded-lg text-[13px] z-[60] shadow-lg border ${
        toast.kind === "ok"
          ? "bg-[#2f6b3a] border-[#3f8a4d] text-white"
          : "bg-[#6e3535] border-[#8c3c3c] text-white"
      }`}
    >
      {toast.text}
    </div>
  );
}

export default function Layout() {
  const {
    settings,
    update,
    loadSettings,
    loadInstances,
    loadAccounts,
    checkUpdate,
    setDownload,
    setRunning,
    appendLog,
  } = useApp();
  const location = useLocation();

  // 启动加载 + 事件订阅
  useEffect(() => {
    loadSettings();
    loadInstances();
    loadAccounts();
    checkUpdate();

    let unsubs: (() => void)[] = [];
    import("@tauri-apps/api/event").then(({ listen }) => {
      Promise.all([
        listen<{ id: string; name: string; loaded: number; total: number; speed: number; finished: boolean; error: string | null }>(
          "download-progress",
          (e) => setDownload(e.payload)
        ),
        listen<{ instance_id: string; line: string }>("game-log", (e) =>
          appendLog(e.payload.instance_id, e.payload.line)
        ),
        listen<{ instance_id: string; code: number | null }>("game-exit", (e) =>
          setRunning(e.payload.instance_id, false)
        ),
      ]).then((us) => (unsubs = us));
    });
    return () => unsubs.forEach((u) => u());
  }, []);

  // 定时检查更新（30 分钟）
  useEffect(() => {
    if (!settings.auto_check_update) return;
    const timer = setInterval(checkUpdate, 30 * 60 * 1000);
    return () => clearInterval(timer);
  }, [settings.auto_check_update]);

  // 全局 Enter 启动快捷键由各页面自行处理
  useEffect(() => {
    document.title = "Cwj Craft Launcher";
  }, []);

  return (
    <div className="h-full flex flex-col bg-ccl-bg">
      <TitleBar />
      {update?.must_update ? (
        <ForceUpdateModal update={update} />
      ) : (
        update && <UpdateBanner update={update} />
      )}
      <div className="flex-1 flex min-h-0">
        <Sidebar />
        <main className="flex-1 flex flex-col min-w-0">
          <div className="flex-1 overflow-auto p-6" key={location.pathname}>
            {location.pathname === "/" && update?.cloud_online && (
              <AnnouncementCard update={update} />
            )}
            <Outlet />
          </div>
          <DownloadDock />
        </main>
      </div>
      <Toast />
    </div>
  );
}
