import { useEffect, useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";
import {
  Play,
  Square,
  FolderOpen,
  Terminal,
  Package,
  Plus,
  Loader2,
  ChevronUp,
  Boxes,
  Download,
} from "lucide-react";
import { useApp } from "../store/app";
import { api, cclUrl } from "../lib/tauri";

const LOADER_LABEL: Record<string, string> = {
  vanilla: "原版",
  fabric: "Fabric",
  quilt: "Quilt",
  forge: "Forge",
  neoforge: "NeoForge",
};

export default function HomePage() {
  const navigate = useNavigate();
  const { instances, accounts, running, setRunning, showToast, loadAccounts } = useApp();
  const logs = useApp((s) => s.logs);
  const [launching, setLaunching] = useState<string | null>(null);
  const [logFor, setLogFor] = useState<string | null>(null);
  const [picked, setPicked] = useState<string | null>(null);
  const [menuOpen, setMenuOpen] = useState(false);
  const [shots, setShots] = useState<Record<string, string[]>>({});

  // 当前选中的实例：默认第一个
  const currentId = useMemo(() => {
    if (picked && instances.some((i) => i.id === picked)) return picked;
    return instances[0]?.id ?? null;
  }, [picked, instances]);
  const current = instances.find((i) => i.id === currentId) || null;
  const isRunning = current ? !!running[current.id] : false;

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

  const versionLabel = (inst: (typeof instances)[number]) =>
    inst.loader === "vanilla"
      ? inst.minecraft_version
      : `${inst.minecraft_version} · ${LOADER_LABEL[inst.loader] || inst.loader}${
          inst.loader_version ? ` ${inst.loader_version}` : ""
        }`;

  /* ---------------- 空状态：HMCL 的「未找到游戏实例」---------------- */
  if (instances.length === 0) {
    return (
      <div className="h-full flex flex-col items-center justify-center text-center">
        <div
          className="w-14 h-14 rounded-[10px] flex items-center justify-center mb-4"
          style={{ background: "rgba(92,107,192,0.12)", color: "var(--hm-primary)" }}
        >
          <Boxes size={26} strokeWidth={1.7} />
        </div>
        <h2 className="text-[17px] font-bold mb-1.5">未找到游戏实例</h2>
        <p className="text-[12.5px] text-ccl-muted mb-5 leading-relaxed">
          下载一个游戏版本，或导入整合包来创建第一个实例。
        </p>
        <div className="flex gap-2.5">
          <button className="btn btn-primary" onClick={() => navigate("/versions")}>
            <Download size={14} /> 下载游戏
          </button>
          <button className="btn btn-ghost" onClick={() => navigate("/modpacks")}>
            <Package size={14} /> 导入整合包
          </button>
          <button className="btn btn-ghost" onClick={() => navigate("/instances")}>
            <Plus size={14} /> 新建实例
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className="hm-home">
      <div className="flex items-start justify-between mb-2">
        <div>
          <h1 className="hm-page-title !mb-0">游戏实例</h1>
          <div className="hm-page-sub">
            共 {instances.length} 个实例 · 点击选中后在右下角启动
          </div>
        </div>
        <div className="flex gap-2 pt-1">
          <button className="btn btn-ghost" onClick={() => navigate("/modpacks")}>
            <Package size={13} /> 导入整合包
          </button>
          <button className="btn btn-primary" onClick={() => navigate("/instances")}>
            <Plus size={13} /> 新建实例
          </button>
        </div>
      </div>

      {/* HMCL 式两行列表：图标 + 标题 13px + 副标题 10.5px */}
      <div className="hm-inst-list mt-2">
        {instances.map((inst) => {
          const runningNow = !!running[inst.id];
          const selected = inst.id === currentId;
          const shot = (shots[inst.id] || [])[0];
          const missing = inst.loader !== "vanilla" && !inst.installed_version_id;
          return (
            <div
              key={inst.id}
              className={`hm-inst-item${selected ? " selected" : ""}`}
              onClick={() => {
                setPicked(inst.id);
                setMenuOpen(false);
              }}
            >
              {shot ? (
                <img
                  src={cclUrl(`instances/${inst.id}/screenshots/${encodeURIComponent(shot)}`)}
                  alt=""
                  className="w-[38px] h-[38px] rounded-[8px] object-cover shrink-0 border border-ccl-border"
                />
              ) : (
                <span className="hm-inst-icon">{inst.name.slice(0, 2).toUpperCase()}</span>
              )}

              <div className="min-w-0 flex-1">
                <div className="flex items-center gap-2">
                  <span className="hm-inst-name truncate">{inst.name}</span>
                  {runningNow && <span className="tag tag-green">运行中</span>}
                  {missing && <span className="tag tag-orange">版本未安装</span>}
                </div>
                <div className="hm-inst-sub truncate">
                  {versionLabel(inst)} · {inst.max_memory || 4096} MB
                </div>
              </div>

              <div className="hm-inst-actions" onClick={(e) => e.stopPropagation()}>
                {runningNow ? (
                  <>
                    <button className="btn btn-danger !py-1 !px-3" onClick={() => stop(inst.id)}>
                      <Square size={12} /> 停止
                    </button>
                    <button
                      className="btn btn-ghost !py-1 !px-2"
                      onClick={() => setLogFor(inst.id)}
                      title="查看日志"
                    >
                      <Terminal size={13} />
                    </button>
                  </>
                ) : (
                  <>
                    <button
                      className="btn btn-ghost !py-1 !px-2"
                      onClick={() => navigate(`/versions?instance=${inst.id}`)}
                      title="管理版本"
                    >
                      <FolderOpen size={13} />
                    </button>
                    <button
                      className="btn btn-ghost !py-1 !px-2"
                      onClick={() => navigate(`/mods?instance=${inst.id}`)}
                      title="管理模组"
                    >
                      <Package size={13} />
                    </button>
                    <button
                      className="btn btn-primary !py-1 !px-4"
                      disabled={launching === inst.id || missing}
                      onClick={() => play(inst.id)}
                    >
                      {launching === inst.id ? (
                        <Loader2 size={13} className="animate-spin" />
                      ) : (
                        <Play size={13} />
                      )}
                      启动
                    </button>
                  </>
                )}
              </div>
            </div>
          );
        })}
      </div>

      {/* 右下角分离式启动按钮（HMCL launch-pane：200px + 27px） */}
      {current && (
        <div className="hm-launch-pane">
          {menuOpen && (
            <div className="hm-popmenu">
              {instances.map((i) => (
                <div
                  key={i.id}
                  className={`hm-popmenu-item${i.id === currentId ? " active" : ""}`}
                  onClick={() => {
                    setPicked(i.id);
                    setMenuOpen(false);
                  }}
                >
                  <span className="truncate flex-1">{i.name}</span>
                  <span className="text-[10px] text-ccl-muted shrink-0">{i.minecraft_version}</span>
                </div>
              ))}
              <div
                className="hm-popmenu-item border-t border-ccl-border mt-1 pt-2"
                onClick={() => {
                  setMenuOpen(false);
                  navigate("/instances");
                }}
              >
                <FolderOpen size={13} /> 管理实例…
              </div>
            </div>
          )}

          <button
            className="hm-launch-btn"
            disabled={launching === current.id || (!isRunning && current.loader !== "vanilla" && !current.installed_version_id)}
            onClick={() => (isRunning ? stop(current.id) : play(current.id))}
            title={isRunning ? "停止游戏" : "启动游戏"}
          >
            {launching === current.id ? (
              <span className="hm-launch-label flex items-center gap-1.5">
                <Loader2 size={13} className="animate-spin" /> 启动中
              </span>
            ) : (
              <span className="hm-launch-label">{isRunning ? "停止游戏" : "启动游戏"}</span>
            )}
            <span className="hm-launch-version">{versionLabel(current)}</span>
          </button>
          <button
            className="hm-launch-more"
            onClick={() => setMenuOpen((v) => !v)}
            title="切换实例"
          >
            <ChevronUp
              size={15}
              style={{ transform: menuOpen ? "rotate(180deg)" : undefined, transition: "transform .18s" }}
            />
          </button>
        </div>
      )}

      {/* 日志抽屉 */}
      {logFor && (
        <div className="hm-dialog-mask !items-end !bg-black/45" onClick={() => setLogFor(null)}>
          <div
            className="w-full h-[45vh] flex flex-col rounded-t-[6px]"
            style={{ background: "var(--hm-surface-low)" }}
            onClick={(e) => e.stopPropagation()}
          >
            <div className="flex items-center justify-between px-4 h-11 border-b border-ccl-border">
              <span className="text-[13px] font-bold">
                运行日志 · {instances.find((i) => i.id === logFor)?.name}
              </span>
              <button className="btn btn-ghost !py-1 !px-3" onClick={() => setLogFor(null)}>
                关闭
              </button>
            </div>
            <pre className="flex-1 overflow-auto p-4 text-[11px] font-mono text-[#454651] whitespace-pre-wrap">
              {(logs[logFor] || []).join("\n") || "暂无日志输出"}
            </pre>
          </div>
        </div>
      )}

      {accounts.length === 0 && (
        <div className="mt-4 card flex items-center gap-2 text-[12px]" style={{ background: "#ffdeac", borderColor: "#f6bd58", color: "#5f4100" }}>
          <span>尚未添加游戏账户，启动前请先到「游戏账户」添加（支持微软正版登录与离线模式）。</span>
          <button className="btn btn-ghost !py-0.5 !px-2.5 !text-[11px] ml-auto shrink-0" onClick={() => navigate("/accounts")}>
            去添加
          </button>
        </div>
      )}
    </div>
  );
}
