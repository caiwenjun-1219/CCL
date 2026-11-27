import { useEffect, useMemo, useRef, useState } from "react";
import { useNavigate } from "react-router-dom";
import { ChevronDown, Loader2, UserCircle2, AlertCircle, Boxes } from "lucide-react";
import { useApp } from "../store/app";
import { api } from "../lib/tauri";
import { isMobile, MOBILE_LAUNCH_HINT } from "../lib/env";

const LOADER_LABEL: Record<string, string> = {
  vanilla: "原版",
  fabric: "Fabric",
  quilt: "Quilt",
  forge: "Forge",
  neoforge: "NeoForge",
};

/** PCL 首页左栏：账户 / 皮肤区 / 大启动按钮 / 版本选择 */
export default function LaunchSide() {
  const navigate = useNavigate();
  const {
    instances,
    accounts,
    running,
    currentInstanceId,
    setCurrentInstance,
    setRunning,
    showToast,
    loadAccounts,
  } = useApp();
  const [launching, setLaunching] = useState(false);
  const [pickerOpen, setPickerOpen] = useState(false);
  const wrapRef = useRef<HTMLDivElement>(null);

  // 选中实例：优先用记忆的，其次第一个
  const current = useMemo(
    () => instances.find((i) => i.id === currentInstanceId) || instances[0] || null,
    [instances, currentInstanceId]
  );
  const isRunning = current ? !!running[current.id] : false;
  const account = accounts[0] || null;

  useEffect(() => {
    if (current && current.id !== currentInstanceId) setCurrentInstance(current.id);
  }, [current?.id]);

  // 点击浮层外部关闭
  useEffect(() => {
    if (!pickerOpen) return;
    const onDown = (e: MouseEvent) => {
      if (wrapRef.current && !wrapRef.current.contains(e.target as Node)) setPickerOpen(false);
    };
    document.addEventListener("mousedown", onDown);
    return () => document.removeEventListener("mousedown", onDown);
  }, [pickerOpen]);

  const play = async () => {
    if (!current) return;
    if (isMobile()) {
      showToast(MOBILE_LAUNCH_HINT, "bad");
      return;
    }
    setLaunching(true);
    try {
      const info = await api.launchInstance(current.id, null);
      setRunning(current.id, true);
      if (accounts.length === 0) loadAccounts();
      showToast(`已启动 ${info.version_id}（Java ${info.java_major}）`);
    } catch (e) {
      showToast(String(e), "bad");
    } finally {
      setLaunching(false);
    }
  };

  const stop = async () => {
    if (!current) return;
    try {
      await api.stopInstance(current.id);
      setRunning(current.id, false);
    } catch (e) {
      showToast(String(e), "bad");
    }
  };

  const versionLabel = current
    ? current.loader === "vanilla"
      ? current.minecraft_version
      : `${LOADER_LABEL[current.loader] || current.loader} ${current.loader_version || ""}`
    : "";
  const notInstalled = !!current && current.loader !== "vanilla" && !current.installed_version_id;

  return (
    <aside className="pcl-sidebar pcl-launch-side w-[300px]" ref={wrapRef}>
      {/* 顶部：当前账户（PCL 的正版/离线登录方式胶囊） */}
      <div className="ls-account px-5 pt-5 flex justify-center shrink-0">
        <button
          className="h-[27px] inline-flex items-center gap-1.5 pl-1.5 pr-3 rounded-[13.5px] bg-ccl-gray8 hover:bg-ccl-brush8 border border-transparent hover:border-ccl-brush6 transition-colors max-w-full"
          onClick={() => navigate("/more/account")}
          title="账户管理与登录方式"
        >
          <span className="w-[19px] h-[19px] rounded-full bg-ccl-accent text-white text-[10px] font-bold flex items-center justify-center shrink-0 overflow-hidden">
            {account ? account.name.slice(0, 1).toUpperCase() : <UserCircle2 size={13} />}
          </span>
          <span className="text-[13px] text-ccl-accent font-semibold truncate max-w-[180px]">
            {account ? account.name : "未登录账户"}
          </span>
          <ChevronDown size={12} className="text-ccl-gray3 shrink-0" />
        </button>
      </div>

      {/* 中部：当前实例概览（占 PCL 皮肤预览的位置） */}
      <div className="ls-preview flex-1 min-h-0 flex flex-col items-center justify-center px-5 gap-3">
        {current ? (
          <>
            <div
              className="w-[86px] h-[86px] rounded-[3px] flex items-center justify-center text-[28px] font-black text-white select-none shrink-0"
              style={{
                background: "linear-gradient(135deg, #4890f5, #1370f3)",
                boxShadow: "0 6px 18px rgba(19,112,243,.28)",
              }}
            >
              {current.name.slice(0, 1).toUpperCase()}
            </div>
            <div className="text-center w-full">
              <div className="text-[15px] font-bold truncate">{current.name}</div>
              <div className="text-[12px] text-ccl-gray3 mt-0.5 truncate">
                {versionLabel}
                {isRunning && <span className="text-ccl-accent ml-1.5">· 运行中</span>}
              </div>
            </div>
            <div className="w-full mt-1 text-[11.5px] text-ccl-gray3 space-y-1">
              <div className="flex justify-between">
                <span>游戏版本</span>
                <span className="text-ccl-gray1">{current.minecraft_version}</span>
              </div>
              <div className="flex justify-between">
                <span>内存</span>
                <span className="text-ccl-gray1">{current.max_memory || 4096} MB</span>
              </div>
              {notInstalled && (
                <div className="flex items-center gap-1 text-ccl-warn pt-1">
                  <AlertCircle size={12} /> 该版本尚未安装
                </div>
              )}
            </div>
          </>
        ) : (
          <div className="flex flex-col items-center gap-3 text-center">
            <div className="w-[76px] h-[76px] rounded-[4px] border-2 border-dashed border-ccl-gray5 flex items-center justify-center text-ccl-gray5">
              <Boxes size={30} strokeWidth={1.5} />
            </div>
            <div className="text-[12.5px] text-ccl-gray3 leading-relaxed">
              还没有游戏实例
              <br />
              <span className="text-[11.5px] text-ccl-gray4">点击下方版本选择下载一个</span>
            </div>
          </div>
        )}
      </div>

      {/* 底部：启动按钮 + 版本选择（PCL：54px 主按钮 + 35px 次按钮） */}
      <div className="ls-actions px-5 pb-5 shrink-0 flex flex-col gap-2.5">
        {isRunning ? (
          <button className="pcl-launch-btn !bg-[#e34b4b] !border-[#e34b4b] hover:!bg-[#ee5b5b]" onClick={stop}>
            结束游戏
            <span className="sub">{current?.name}</span>
          </button>
        ) : (
          <button
            className="pcl-launch-btn"
            onClick={play}
            disabled={!current || launching || notInstalled || isMobile()}
            title={isMobile() ? MOBILE_LAUNCH_HINT : undefined}
          >
            {launching ? (
              <span className="flex items-center gap-2">
                <Loader2 size={16} className="animate-spin" /> 正在启动
              </span>
            ) : (
              "启动游戏"
            )}
            <span className="sub">
              {current ? (isMobile() ? "安卓启动功能开发中" : versionLabel || "—") : "未选择版本"}
            </span>
          </button>
        )}

        <button
          className="btn ls-pick !h-[35px]"
          onClick={() => (instances.length ? setPickerOpen((v) => !v) : navigate("/download/game"))}
        >
          版本选择
          <ChevronDown size={13} className={pickerOpen ? "rotate-180 transition-transform" : "transition-transform"} />
        </button>
      </div>

      {/* 版本选择浮层：PCL 标志性的实例列表（白底 + 42px 行 + 浅蓝 hover） */}
      {pickerOpen && instances.length > 0 && (
        <div className="absolute left-4 right-4 bottom-[104px] z-30 bg-white rounded-[3px] border border-ccl-gray6 shadow-[0_8px_28px_rgba(0,0,0,.18)] overflow-hidden">
          <div className="max-h-[286px] overflow-y-auto py-1">
            {instances.map((inst) => {
              const active = inst.id === current?.id;
              const sub =
                inst.loader === "vanilla"
                  ? inst.minecraft_version
                  : `${inst.minecraft_version} · ${LOADER_LABEL[inst.loader] || inst.loader}`;
              return (
                <div
                  key={inst.id}
                  className="pcl-row !rounded-none !h-[42px] !py-0"
                  data-active={active}
                  onClick={() => {
                    setCurrentInstance(inst.id);
                    setPickerOpen(false);
                  }}
                >
                  <span className="w-[26px] h-[26px] rounded-[2px] flex items-center justify-center text-[11px] font-bold text-white shrink-0 bg-ccl-accent">
                    {inst.name.slice(0, 1).toUpperCase()}
                  </span>
                  <span className="flex-1 min-w-0">
                    <span className={`block text-[13px] truncate ${active ? "text-ccl-accent font-semibold" : ""}`}>
                      {inst.name}
                    </span>
                    <span className="block text-[11px] text-ccl-gray3 truncate">{sub}</span>
                  </span>
                  {running[inst.id] && <span className="tag tag-green">运行中</span>}
                </div>
              );
            })}
          </div>
          <div className="border-t border-ccl-gray6 p-1.5 flex gap-1.5">
            <button
              className="btn !h-[26px] !text-[12px] flex-1"
              onClick={() => {
                setPickerOpen(false);
                navigate("/download/game");
              }}
            >
              下载新版本
            </button>
            <button
              className="btn !h-[26px] !text-[12px] flex-1"
              onClick={() => {
                setPickerOpen(false);
                navigate("/more/version");
              }}
            >
              版本管理
            </button>
          </div>
        </div>
      )}
    </aside>
  );
}
