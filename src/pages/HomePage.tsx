import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import { Play, Square, FolderOpen, Terminal, Package, Plus, Loader2 } from "lucide-react";
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
  const { instances, accounts, running, setRunning, appendLog, showToast, loadAccounts } = useApp();
  const [launching, setLaunching] = useState<string | null>(null);
  const [logFor, setLogFor] = useState<string | null>(null);
  const logs = useApp((s) => s.logs);
  // 各实例最新截图（经 ccl:// 协议直接加载本地图片）
  const [shots, setShots] = useState<Record<string, string[]>>({});

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
      // 启动可能自动创建了离线账户，刷新列表保证账户页/提示卡同步
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

  if (instances.length === 0) {
    return (
      <div className="h-full flex flex-col items-center justify-center text-center">
        <div className="w-16 h-16 rounded-2xl bg-ccl-accent flex items-center justify-center text-lg font-extrabold text-white mb-5">
          CCL
        </div>
        <h2 className="text-lg font-bold mb-2">欢迎使用 Cwj Craft Launcher</h2>
        <p className="text-[13px] text-ccl-muted mb-6">
          还没有游戏实例。创建一个实例，或导入一个整合包开始游戏。
        </p>
        <div className="flex gap-3">
          <button className="btn btn-primary" onClick={() => navigate("/instances")}>
            <Plus size={14} /> 创建实例
          </button>
          <button className="btn btn-ghost" onClick={() => navigate("/modpacks")}>
            <Package size={14} /> 导入整合包
          </button>
        </div>
      </div>
    );
  }

  return (
    <div>
      <div className="flex items-center justify-between mb-4">
        <h1 className="text-base font-bold">游戏实例</h1>
        <div className="flex gap-2">
          <button className="btn btn-ghost" onClick={() => navigate("/modpacks")}>
            <Package size={13} /> 导入整合包
          </button>
          <button className="btn btn-primary" onClick={() => navigate("/instances")}>
            <Plus size={13} /> 新建实例
          </button>
        </div>
      </div>

      {/* PCL 式版本卡片列表：白底圆角行 + 大绿色启动按钮 */}
      <div className="flex flex-col gap-2.5">
        {instances.map((inst) => {
          const isRunning = running[inst.id];
          const versionLabel =
            inst.loader === "vanilla"
              ? inst.minecraft_version
              : `${inst.minecraft_version} · ${LOADER_LABEL[inst.loader] || inst.loader}`;
          return (
            <div
              key={inst.id}
              className="card hover:border-ccl-accent/50 transition-colors flex items-center gap-4 !py-3.5"
            >
              <div className="w-11 h-11 rounded-lg bg-ccl-accent/10 border border-ccl-accent/25 flex items-center justify-center text-[13px] font-bold text-ccl-accent shrink-0">
                {inst.name.slice(0, 2).toUpperCase()}
              </div>
              <div className="min-w-0 flex-1">
                <div className="font-bold text-[14px] truncate flex items-center gap-2">
                  {inst.name}
                  {isRunning && <span className="tag tag-green">运行中</span>}
                </div>
                <div className="text-[12px] text-ccl-muted mt-0.5 truncate">
                  {versionLabel}
                  {inst.loader_version ? ` ${inst.loader_version}` : ""}
                  {" · "}
                  {inst.max_memory || 4096} MB
                  {inst.loader !== "vanilla" && !inst.installed_version_id && (
                    <span className="text-ccl-warn ml-1.5">· 版本未安装</span>
                  )}
                </div>
                {(shots[inst.id] || []).length > 0 && (
                  <div className="flex gap-1.5 mt-1.5">
                    {(shots[inst.id] || []).slice(0, 4).map((name) => (
                      <img
                        key={name}
                        src={cclUrl(
                          `instances/${inst.id}/screenshots/${encodeURIComponent(name)}`
                        )}
                        alt=""
                        loading="lazy"
                        title={name}
                        className="w-9 h-9 rounded object-cover bg-ccl-panel2 border border-ccl-border"
                      />
                    ))}
                  </div>
                )}
              </div>

              <div className="flex items-center gap-2 shrink-0">
                {isRunning ? (
                  <>
                    <button className="btn btn-danger" onClick={() => stop(inst.id)}>
                      <Square size={13} /> 停止
                    </button>
                    <button
                      className="btn btn-ghost !px-2.5"
                      onClick={() => setLogFor(inst.id)}
                      title="查看日志"
                    >
                      <Terminal size={13} />
                    </button>
                  </>
                ) : (
                  <>
                    <button
                      className="btn btn-primary !px-5 !py-2"
                      disabled={
                        launching === inst.id ||
                        (inst.loader !== "vanilla" && !inst.installed_version_id)
                      }
                      onClick={() => play(inst.id)}
                    >
                      {launching === inst.id ? (
                        <Loader2 size={14} className="animate-spin" />
                      ) : (
                        <Play size={14} />
                      )}
                      启动
                    </button>
                    <button
                      className="btn btn-ghost !px-2.5"
                      onClick={() => navigate(`/versions?instance=${inst.id}`)}
                      title="管理版本"
                    >
                      <FolderOpen size={13} />
                    </button>
                    <button
                      className="btn btn-ghost !px-2.5"
                      onClick={() => navigate(`/mods?instance=${inst.id}`)}
                      title="管理模组"
                    >
                      <Package size={13} />
                    </button>
                  </>
                )}
              </div>
            </div>
          );
        })}
      </div>

      {/* 日志抽屉 */}
      {logFor && (
        <div className="fixed inset-0 bg-black/40 z-40 flex items-end" onClick={() => setLogFor(null)}>
          <div
            className="bg-ccl-panel border-t border-ccl-border w-full h-[45vh] flex flex-col rounded-t-2xl"
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
            <pre className="flex-1 overflow-auto p-4 text-[11px] font-mono text-[#5a6270] whitespace-pre-wrap">
              {(logs[logFor] || []).join("\n") || "暂无日志输出"}
            </pre>
          </div>
        </div>
      )}

      {accounts.length === 0 && (
        <div className="mt-5 card border-[#e3c890] bg-[#fdf6e6] text-[12px] text-[#8a6420]">
          尚未添加任何游戏账户。启动游戏前请先到「账户」页添加（支持微软正版登录与离线模式）。
        </div>
      )}
    </div>
  );
}
