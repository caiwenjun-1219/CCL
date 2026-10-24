import { useEffect, useRef, useState } from "react";
import { Link, Outlet, useLocation, useNavigate } from "react-router-dom";
import {
  Download,
  Package,
  Puzzle,
  Coffee,
  Boxes,
  User,
  Info,
  SlidersHorizontal,
  Palette,
  Settings2,
  Cpu,
  ArrowUpCircle,
  X,
  ChevronUp,
  Check,
} from "lucide-react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useApp } from "../store/app";
import { api, fmtSize, fmtSpeed, type UpdateInfo } from "../lib/tauri";
import { PclIcon, CclLogo } from "./PclIcon";
import LaunchSide from "./LaunchSide";
import SubNav, { type SubNavGroup } from "./SubNav";

// 浏览器（vite preview）下 getCurrentWindow 不可用，降级为 null 便于纯前端调试
const appWindow = (() => {
  try {
    return getCurrentWindow();
  } catch {
    return null;
  }
})();

/* ------------------------------------------------------------------ 导航定义 */

const TOP_NAV = [
  { key: "launch", label: "启动", to: "/", icon: "play" as const, scale: 0.92 },
  { key: "download", label: "下载", to: "/download/game", icon: "download" as const, scale: 0.9 },
  { key: "link", label: "联机", to: "/link", icon: "link" as const, scale: 1.05 },
  { key: "setup", label: "设置", to: "/settings", icon: "setup" as const, scale: 1.08 },
  { key: "more", label: "更多", to: "/more/version", icon: "more" as const, scale: 0.92 },
];

const DOWNLOAD_NAV: SubNavGroup[] = [
  {
    items: [
      { to: "/download/game", label: "游戏本体", icon: Download },
      { to: "/download/mod", label: "模组", icon: Puzzle },
      { to: "/download/pack", label: "整合包", icon: Package },
      { to: "/download/java", label: "Java", icon: Coffee },
    ],
  },
];

const SETTINGS_NAV: SubNavGroup[] = [
  {
    items: [
      { to: "/settings?tab=launch", label: "启动", icon: SlidersHorizontal },
      { to: "/settings?tab=ui", label: "个性化", icon: Palette },
      { to: "/settings?tab=system", label: "系统", icon: Cpu },
      { to: "/settings?tab=other", label: "其他", icon: Settings2 },
    ],
  },
];

const MORE_NAV: SubNavGroup[] = [
  {
    items: [
      { to: "/more/version", label: "版本管理", icon: Boxes },
      { to: "/more/account", label: "游戏账户", icon: User },
      { to: "/more/about", label: "关于", icon: Info },
    ],
  },
];

/* ------------------------------------------------------------------ 标题栏 */

function TitleBar() {
  const { pathname } = useLocation();
  const navigate = useNavigate();

  const isActive = (key: string) => {
    if (key === "launch") return pathname === "/";
    if (key === "download") return pathname.startsWith("/download") || pathname.startsWith("/mod");
    if (key === "link") return pathname.startsWith("/link");
    if (key === "setup") return pathname.startsWith("/settings");
    return pathname.startsWith("/more") || pathname.startsWith("/instances");
  };

  const win = (fn: () => void) => () => {
    try {
      fn();
    } catch {
      /* 非 Tauri 环境忽略 */
    }
  };

  return (
    <div className="pcl-titlebar" data-tauri-drag-region>
      {/* Logo 区（可拖拽） */}
      <div
        className="flex items-center pl-5 pr-2 shrink-0"
        data-tauri-drag-region
        onDoubleClick={win(() => appWindow?.toggleMaximize())}
      >
        <CclLogo width={50} />
      </div>

      {/* 中间页签 */}
      <div className="flex-1 flex items-center justify-center min-w-0" data-tauri-drag-region>
        <div className="flex items-center">
          {TOP_NAV.map((n) => (
            <button
              key={n.key}
              className="pcl-tab"
              data-active={isActive(n.key)}
              onClick={() => navigate(n.to)}
              title={n.label}
            >
              <PclIcon name={n.icon} size={16} scale={n.scale} />
              <span>{n.label}</span>
            </button>
          ))}
        </div>
      </div>

      {/* 右侧窗口按钮（PCL：最小化 + 关闭） */}
      <div className="flex items-center gap-1 pr-3 shrink-0">
        <button className="pcl-winbtn" onClick={win(() => appWindow?.minimize())} title="最小化">
          <PclIcon name="min" size={11} />
        </button>
        <button className="pcl-winbtn" onClick={win(() => appWindow?.close())} title="关闭">
          <PclIcon name="close" size={12} />
        </button>
      </div>
    </div>
  );
}

/* ------------------------------------------------------------------ 下载任务中心 */

function DownloadCenter() {
  const downloads = useApp((s) => s.downloads);
  const [open, setOpen] = useState(false);
  const list = Object.values(downloads).filter((d) => !d.finished);
  const active = list.length > 0;

  return (
    <>
      <button
        className="pcl-float-btn relative"
        data-accent={active}
        onClick={() => setOpen((v) => !v)}
        title={active ? `下载任务 ${list.length}` : "任务管理"}
        style={(open ? {} : { display: "flex" }) as React.CSSProperties}
      >
        <Download size={17} />
        {active && (
          <span className="absolute -top-0.5 -right-0.5 min-w-[16px] h-[16px] px-1 rounded-full bg-[#e34b4b] text-white text-[10px] font-bold flex items-center justify-center border border-white">
            {list.length}
          </span>
        )}
      </button>

      {open && (
        <div className="absolute right-[20px] bottom-[64px] w-[380px] max-h-[340px] bg-white rounded-[3px] border border-ccl-gray6 shadow-[0_10px_32px_rgba(0,0,0,.2)] overflow-hidden z-30">
          <div className="h-[36px] px-3 flex items-center justify-between border-b border-ccl-gray6">
            <span className="text-[13px] font-bold">下载任务</span>
            <button className="text-ccl-gray3 hover:text-ccl-text" onClick={() => setOpen(false)}>
              <X size={14} />
            </button>
          </div>
          <div className="max-h-[300px] overflow-y-auto p-2">
            {list.length === 0 ? (
              <div className="py-8 text-center text-[12px] text-ccl-gray3">当前没有进行中的任务</div>
            ) : (
              list.map((d) => {
                const pct = d.total > 0 ? Math.floor((d.loaded / d.total) * 100) : 0;
                return (
                  <div key={d.id} className="px-1 py-1.5">
                    <div className="flex justify-between text-[12px] mb-1">
                      <span className="truncate pr-2 text-ccl-text">{d.name}</span>
                      <span className="text-ccl-gray3 shrink-0">
                        {d.total > 0 ? `${fmtSize(d.loaded)} / ${fmtSize(d.total)}` : fmtSpeed(d.speed)}
                      </span>
                    </div>
                    <div className="h-[4px] bg-ccl-gray7 rounded-[2px] overflow-hidden">
                      <div
                        className="h-full rounded-[2px] transition-all"
                        style={{
                          width: `${pct}%`,
                          background: "linear-gradient(90deg,#4890f5,#1370f3)",
                        }}
                      />
                    </div>
                  </div>
                );
              })
            )}
          </div>
        </div>
      )}
    </>
  );
}

/* ------------------------------------------------------------------ 更新提示 */

function UpdateBar({ update }: { update: UpdateInfo }) {
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
    <div className="shrink-0 h-[34px] px-4 flex items-center gap-2 bg-ccl-brush8 border-b border-ccl-brush6 text-[12px] text-ccl-brush2">
      <ArrowUpCircle size={14} className="text-ccl-accent shrink-0" />
      <span className="truncate">
        发现新版本 <b>v{update.latest_version}</b>
        <span className="text-ccl-gray3 ml-2">当前 v{update.current_version}</span>
      </span>
      <div className="flex-1" />
      <button className="btn btn-primary !h-[24px] !text-[12px] !px-3" onClick={install} disabled={installing}>
        {installing ? "下载中…" : "立即更新"}
      </button>
      <button
        className="text-ccl-gray3 hover:text-ccl-gray1 px-1"
        onClick={() => setDismissed(true)}
        title="稍后"
      >
        <X size={14} />
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
    import("@tauri-apps/api/event")
      .then(({ listen }) =>
        listen<{ loaded: number; total: number }>("update-download", (e) => {
          if (e.payload.total > 0) setProgress(Math.floor((e.payload.loaded / e.payload.total) * 100));
        })
      )
      .then((u) => (unlisten = u));
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
    <div className="pcl-modal-mask">
      <div className="pcl-modal w-[400px]">
        <div className="pcl-modal-head justify-center">
          <span>必须更新才能继续使用</span>
        </div>
        <div className="pcl-modal-body text-center">
          <ArrowUpCircle size={34} className="mx-auto text-ccl-accent mb-2" />
          <p className="text-[12.5px] text-ccl-gray2">
            当前版本 v{update.current_version} 已低于最低支持版本 v{update.latest_version}
            <br />
            请更新到最新版本后重新打开启动器
          </p>
          {update.release_notes && (
            <pre className="text-left text-[12px] bg-ccl-gray8 rounded-[3px] p-2.5 mt-3 whitespace-pre-wrap max-h-28 overflow-auto">
              {update.release_notes}
            </pre>
          )}
          {installing && (
            <div className="h-[4px] bg-ccl-gray7 rounded-[2px] overflow-hidden mt-3">
              <div className="h-full bg-ccl-accent" style={{ width: `${progress}%` }} />
            </div>
          )}
        </div>
        <div className="pcl-modal-foot">
          <button className="btn btn-primary !px-4" onClick={install} disabled={installing}>
            {installing ? `下载中 ${progress}%` : "下载并安装更新"}
          </button>
        </div>
      </div>
    </div>
  );
}

function Toast() {
  const toast = useApp((s) => s.toast);
  if (!toast) return null;
  return (
    <div className="pcl-toast" data-kind={toast.kind === "bad" ? "bad" : "ok"}>
      {toast.kind === "ok" ? <Check size={14} /> : <X size={14} />}
      <span className="truncate">{toast.text}</span>
    </div>
  );
}

/* ------------------------------------------------------------------ 主布局 */

export default function Layout() {
  const { pathname } = useLocation();
  const { loadSettings, loadInstances, loadAccounts, checkUpdate, settings, setDownload, setRunning, appendLog } =
    useApp();
  const mainRef = useRef<HTMLDivElement>(null);
  const [showTop, setShowTop] = useState(false);
  const [maxed, setMaxed] = useState(false);

  // 最大化时去掉透明外边距与圆角（否则四周会露出桌面）
  useEffect(() => {
    let un: (() => void) | undefined;
    try {
      appWindow?.isMaximized().then(setMaxed).catch(() => {});
      appWindow
        ?.onResized(() => {
          appWindow?.isMaximized().then(setMaxed).catch(() => {});
        })
        .then((u) => (un = u))
        .catch(() => {});
    } catch {
      /* 非 Tauri 环境忽略 */
    }
    return () => un?.();
  }, []);

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

  useEffect(() => {
    if (!settings.auto_check_update) return;
    const timer = setInterval(checkUpdate, 30 * 60 * 1000);
    return () => clearInterval(timer);
  }, [settings.auto_check_update]);

  useEffect(() => {
    document.title = "Cwj Craft Launcher";
  }, []);

  const update = useApp((s) => s.update);

  // 左侧栏：启动页由 LaunchSide 自带，其余页面用 152px 子导航
  const side: { groups: SubNavGroup[] } | null = pathname.startsWith("/download")
    ? { groups: DOWNLOAD_NAV }
    : pathname.startsWith("/settings")
      ? { groups: SETTINGS_NAV }
      : pathname.startsWith("/more")
        ? { groups: MORE_NAV }
        : null;

  return (
    <div className="pcl-window" data-max={maxed}>
      <div className="pcl-frame">
        <TitleBar />
        {update?.must_update ? <ForceUpdateModal update={update} /> : update && <UpdateBar update={update} />}

        <div className="flex-1 flex min-h-0 relative">
          {pathname === "/" ? (
            <LaunchSide />
          ) : (
            side && (
              <aside className="pcl-sidebar w-[152px]">
                <SubNav groups={side.groups} footer={`CCL v${update?.current_version || "1.0.0"}`} />
              </aside>
            )
          )}

          <main className="flex-1 min-w-0 relative flex flex-col">
            <div
              ref={mainRef}
              className="flex-1 overflow-auto px-6 py-5 pcl-page"
              key={pathname}
              onScroll={(e) => setShowTop((e.target as HTMLDivElement).scrollTop > 240)}
            >
              <Outlet />
            </div>

            {/* 右下浮动按钮组（PCL MyExtraButton） */}
            <div className="pcl-float">
              {showTop && (
                <button
                  className="pcl-float-btn"
                  onClick={() => mainRef.current?.scrollTo({ top: 0, behavior: "smooth" })}
                  title="返回顶部"
                >
                  <ChevronUp size={18} />
                </button>
              )}
              <DownloadCenter />
            </div>
          </main>
        </div>

        <Toast />
      </div>
    </div>
  );
}
