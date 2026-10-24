import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import { Play, Square, FolderOpen, Terminal, Package, Plus, Loader2, Bell, X, ChevronRight } from "lucide-react";
import { useApp } from "../store/app";
import { api, cclUrl } from "../lib/tauri";

const LOADER_LABEL: Record<string, string> = {
  vanilla: "原版",
  fabric: "Fabric",
  quilt: "Quilt",
  forge: "Forge",
  neoforge: "NeoForge",
};

/** PCL 启动页右栏：实例列表（行式，PCL MyListItem 风格） */
export default function HomePage() {
  const navigate = useNavigate();
  const {
    instances,
    accounts,
    update,
    running,
    currentInstanceId,
    setCurrentInstance,
    setRunning,
    showToast,
    loadAccounts,
  } = useApp();
  const [launching, setLaunching] = useState<string | null>(null);
  const [logFor, setLogFor] = useState<string | null>(null);
  const logs = useApp((s) => s.logs);
  const [shots, setShots] = useState<Record<string, string[]>>({});
  const [annDismissed, setAnnDismissed] = useState(() => {
    const seen = localStorage.getItem("ccl-announcement-seen");
    return !!update && seen === `${update.announcement}|${update.updated_at}`;
  });

  useEffect(() => {
    let alive = true;
    if (instances.length === 0) return;
    Promise.all(
      instances.map((i) =>
        api
          .listScreenshots(i.id)
          .then((list) => [i.id, list] as const)
          .catch(() => [i.id, [] as string[]] as const)
      )
    ).then((pairs) => {
      if (!alive) return;
      const m: Record<string, string[]> = {};
      for (const [id, list] of pairs) m[id] = list;
      setShots(m);
    });
    return () => {
      alive = false;
    };
  }, [instances]);

  const play = async (id: string) => {
    setLaunching(id);
    try {
      const info = await api.launchInstance(id, null);
      setRunning(id, true);
      if (accounts.length === 0) loadAccounts();
      showToast(`已启动 ${info.version_id}（Java ${info.java_major}）`);
    } catch (e) {
      showToast(String(e), "bad");
    } finally {
      setLaunching(null);
    }
  };

  const stop = async (id: string) => {
    try {
      await api.stopInstance(id);
      setRunning(id, false);
    } catch (e) {
      showToast(String(e), "bad");
    }
  };

  const showAnn = !!update?.announcement && !annDismissed;

  /* ------------------------------------------------------------- 空态 */
  if (instances.length === 0) {
    return (
      <div className="h-full flex flex-col items-center justify-center text-center select-none">
        <div
          className="w-[72px] h-[72px] rounded-[4px] flex items-center justify-center text-[26px] font-black text-white mb-5"
          style={{ background: "linear-gradient(135deg,#4890f5,#1370f3)", boxShadow: "0 8px 22px rgba(19,112,243,.3)" }}
        >
          C
        </div>
        <h2 className="text-[16px] font-bold mb-1.5">还没有游戏实例</h2>
        <p className="text-[12.5px] text-ccl-gray3 mb-6 leading-relaxed">
          下载一个游戏版本，或导入一个整合包开始游戏。
        </p>
        <div className="flex gap-2.5">
          <button className="btn btn-primary !h-[34px] !px-5" onClick={() => navigate("/download/game")}>
            <Plus size={14} /> 下载游戏
          </button>
          <button className="btn !h-[34px] !px-5" onClick={() => navigate("/download/pack")}>
            <Package size={14} /> 导入整合包
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className="max-w-[920px]">
      {/* 云控公告（PCL 的浅蓝提示条） */}
      {showAnn && update && (
        <div className="flex items-start gap-2 bg-ccl-brush8 border border-ccl-brush6 rounded-[3px] px-3 py-2 mb-4 text-[12.5px] text-ccl-brush2">
          <Bell size={14} className="mt-0.5 shrink-0" />
          <div className="flex-1 whitespace-pre-wrap leading-relaxed">{update.announcement}</div>
          <button
            className="text-ccl-gray3 hover:text-ccl-gray1 shrink-0"
            onClick={() => {
              localStorage.setItem("ccl-announcement-seen", `${update.announcement}|${update.updated_at}`);
              setAnnDismissed(true);
            }}
          >
            <X size={14} />
          </button>
        </div>
      )}

      {/* 标题行 */}
      <div className="flex items-end justify-between mb-3">
        <div>
          <h1 className="pcl-title">游戏实例</h1>
          <p className="pcl-subtitle">点击任意实例可设为当前启动版本</p>
        </div>
        <div className="flex gap-2">
          <button className="btn !h-[28px] !text-[12px]" onClick={() => navigate("/download/pack")}>
            <Package size={12} /> 整合包
          </button>
          <button className="btn btn-primary !h-[28px] !text-[12px]" onClick={() => navigate("/download/game")}>
            <Plus size={12} /> 新建实例
          </button>
        </div>
      </div>

      {/* 实例列表 */}
      <div className="rounded-[3px] overflow-hidden">
        {instances.map((inst, i) => {
          const isRunning = running[inst.id];
          const active = inst.id === currentInstanceId;
          const versionLabel =
            inst.loader === "vanilla"
              ? inst.minecraft_version
              : `${LOADER_LABEL[inst.loader] || inst.loader} ${inst.loader_version || ""}`.trim();
          const shot = (shots[inst.id] || [])[0];
          return (
            <div
              key={inst.id}
              className="flex items-center gap-3 px-3 h-[58px] rounded-[3px] cursor-pointer transition-colors group"
              style={{
                borderTop: i === 0 ? "none" : "1px solid var(--color-ccl-gray6)",
                background: active ? "var(--color-ccl-brush8)" : undefined,
              }}
              onMouseEnter={(e) => {
                if (!active) e.currentTarget.style.background = "#f7fafe";
              }}
              onMouseLeave={(e) => {
                if (!active) e.currentTarget.style.background = "";
              }}
              onClick={() => setCurrentInstance(inst.id)}
            >
              {/* 图标 / 截图缩略图 */}
              {shot ? (
                <img
                  src={cclUrl(`instances/${inst.id}/screenshots/${encodeURIComponent(shot)}`)}
                  alt=""
                  className="w-[38px] h-[38px] rounded-[2px] object-cover shrink-0 border border-ccl-gray6"
                  loading="lazy"
                />
              ) : (
                <span
                  className="w-[38px] h-[38px] rounded-[2px] flex items-center justify-center text-[14px] font-bold text-white shrink-0"
                  style={{ background: active ? "#1370f3" : "linear-gradient(135deg,#7eaef8,#4890f5)" }}
                >
                  {inst.name.slice(0, 1).toUpperCase()}
                </span>
              )}

              <div className="min-w-0 flex-1">
                <div className="flex items-center gap-2">
                  <span
                    className={`text-[13.5px] truncate ${active ? "text-ccl-accent font-bold" : "font-semibold"}`}
                  >
                    {inst.name}
                  </span>
                  {isRunning && <span className="tag tag-green">运行中</span>}
                  {inst.loader !== "vanilla" && !inst.installed_version_id && (
                    <span className="tag tag-orange">未安装</span>
                  )}
                </div>
                <div className="text-[11.5px] text-ccl-gray3 truncate mt-0.5">
                  {versionLabel} · {inst.max_memory || 4096} MB
                </div>
              </div>

              <div className="flex items-center gap-1.5 shrink-0" onClick={(e) => e.stopPropagation()}>
                {isRunning ? (
                  <>
                    <button className="btn btn-danger !h-[26px] !text-[12px] !px-2.5" onClick={() => stop(inst.id)}>
                      <Square size={11} /> 停止
                    </button>
                    <button className="btn !h-[26px] !px-2" onClick={() => setLogFor(inst.id)} title="查看日志">
                      <Terminal size={12} />
                    </button>
                  </>
                ) : (
                  <>
                    <button
                      className="btn btn-primary !h-[26px] !text-[12px] !px-3"
                      disabled={launching === inst.id || (inst.loader !== "vanilla" && !inst.installed_version_id)}
                      onClick={() => play(inst.id)}
                    >
                      {launching === inst.id ? (
                        <Loader2 size={12} className="animate-spin" />
                      ) : (
                        <Play size={11} />
                      )}
                      启动
                    </button>
                    <button
                      className="btn !h-[26px] !px-2"
                      onClick={() => navigate(`/download/game?instance=${inst.id}`)}
                      title="管理版本"
                    >
                      <FolderOpen size={12} />
                    </button>
                    <button
                      className="btn !h-[26px] !px-2"
                      onClick={() => navigate(`/download/mod?instance=${inst.id}`)}
                      title="管理模组"
                    >
                      <Package size={12} />
                    </button>
                  </>
                )}
                <ChevronRight size={14} className="text-ccl-gray4 opacity-0 group-hover:opacity-100 transition-opacity" />
              </div>
            </div>
          );
        })}
      </div>

      {accounts.length === 0 && (
        <div className="mt-4 flex items-center gap-2 border border-[#f0dcb4] bg-[#fdf8ea] rounded-[3px] px-3 py-2 text-[12px] text-[#8a6420]">
          尚未添加游戏账户，启动前请先到「更多 → 游戏账户」登录。
          <button className="text-ccl-accent hover:underline ml-auto shrink-0" onClick={() => navigate("/more/account")}>
            去登录
          </button>
        </div>
      )}

      {/* 日志弹窗（PCL 风格模态） */}
      {logFor && (
        <div className="pcl-modal-mask" onClick={() => setLogFor(null)}>
          <div className="pcl-modal w-[720px] max-w-[86vw]" onClick={(e) => e.stopPropagation()}>
            <div className="pcl-modal-head">
              <span>运行日志 · {instances.find((i) => i.id === logFor)?.name}</span>
              <button className="text-ccl-gray3 hover:text-ccl-gray1" onClick={() => setLogFor(null)}>
                <X size={15} />
              </button>
            </div>
            <div className="pcl-modal-body !p-0">
              <pre className="h-[46vh] overflow-auto p-3 text-[11.5px] font-mono text-ccl-gray1 whitespace-pre-wrap bg-[#fbfbfc]">
                {(logs[logFor] || []).join("\n") || "暂无日志输出"}
              </pre>
            </div>
            <div className="pcl-modal-foot">
              <button className="btn" onClick={() => setLogFor(null)}>
                关闭
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
