import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { Play, Square, FolderOpen, Terminal, Package, Plus, Loader2 } from "lucide-react";
import { useApp } from "../store/app";
import { api } from "../lib/tauri";

const LOADER_LABEL: Record<string, string> = {
  vanilla: "原版",
  fabric: "Fabric",
  quilt: "Quilt",
  forge: "Forge",
  neoforge: "NeoForge",
};

export default function HomePage() {
  const navigate = useNavigate();
  const { instances, accounts, running, setRunning, appendLog, showToast } = useApp();
  const [launching, setLaunching] = useState<string | null>(null);
  const [logFor, setLogFor] = useState<string | null>(null);
  const logs = useApp((s) => s.logs);

  const play = async (id: string) => {
    setLaunching(id);
    try {
      const info = await api.launchInstance(id, null);
      setRunning(id, true);
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
        <div className="w-16 h-16 rounded-2xl bg-gradient-to-br from-[#5db95f] to-[#3c8c3f] flex items-center justify-center text-lg font-extrabold text-[#0f1420] mb-5">
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
      <div className="flex items-center justify-between mb-5">
        <h1 className="text-lg font-bold">游戏实例</h1>
        <div className="flex gap-2">
          <button className="btn btn-ghost" onClick={() => navigate("/modpacks")}>
            <Package size={13} /> 导入整合包
          </button>
          <button className="btn btn-primary" onClick={() => navigate("/instances")}>
            <Plus size={13} /> 新建实例
          </button>
        </div>
      </div>

      <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-4">
        {instances.map((inst) => {
          const isRunning = running[inst.id];
          const versionLabel =
            inst.loader === "vanilla"
              ? inst.minecraft_version
              : `${inst.minecraft_version} · ${LOADER_LABEL[inst.loader] || inst.loader}`;
          return (
            <div
              key={inst.id}
              className="card hover:border-[#3a4a6e] transition-colors flex flex-col"
            >
              <div className="flex items-start gap-3 mb-3">
                <div className="w-11 h-11 rounded-xl bg-[#223052] flex items-center justify-center text-[13px] font-bold text-[#9fb4ff] shrink-0">
                  {inst.name.slice(0, 2).toUpperCase()}
                </div>
                <div className="min-w-0 flex-1">
                  <div className="font-bold text-[14px] truncate">{inst.name}</div>
                  <div className="text-[12px] text-ccl-muted mt-0.5">{versionLabel}</div>
                </div>
                {isRunning && (
                  <span className="tag tag-green shrink-0">运行中</span>
                )}
              </div>

              <div className="flex flex-wrap gap-1.5 mb-4">
                <span className="tag tag-gray">
                  {inst.max_memory || 4096} MB
                </span>
                {inst.loader !== "vanilla" && (
                  <span className="tag">{LOADER_LABEL[inst.loader] || inst.loader}</span>
                )}
                {!inst.installed_version_id && inst.loader !== "vanilla" && (
                  <span className="tag tag-orange">未安装</span>
                )}
              </div>

              <div className="flex gap-2 mt-auto">
                {isRunning ? (
                  <>
                    <button
                      className="btn btn-danger flex-1"
                      onClick={() => stop(inst.id)}
                    >
                      <Square size={13} /> 停止
                    </button>
                    <button
                      className="btn btn-ghost"
                      onClick={() => setLogFor(inst.id)}
                      title="查看日志"
                    >
                      <Terminal size={13} />
                    </button>
                  </>
                ) : (
                  <>
                    <button
                      className="btn btn-primary flex-1"
                      disabled={launching === inst.id || (inst.loader !== "vanilla" && !inst.installed_version_id)}
                      onClick={() => play(inst.id)}
                    >
                      {launching === inst.id ? (
                        <Loader2 size={13} className="animate-spin" />
                      ) : (
                        <Play size={13} />
                      )}
                      启动
                    </button>
                    <button
                      className="btn btn-ghost"
                      onClick={() => navigate(`/versions?instance=${inst.id}`)}
                      title="管理版本"
                    >
                      <FolderOpen size={13} />
                    </button>
                    <button
                      className="btn btn-ghost"
                      onClick={() => navigate(`/mods?instance=${inst.id}`)}
                      title="管理模组"
                    >
                      <Package size={13} />
                    </button>                  </>
                )}
              </div>
            </div>
          );
        })}
      </div>

      {/* 日志抽屉 */}
      {logFor && (
        <div className="fixed inset-0 bg-black/60 z-40 flex items-end" onClick={() => setLogFor(null)}>
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
            <pre className="flex-1 overflow-auto p-4 text-[11px] font-mono text-[#aeb6c8] whitespace-pre-wrap">
              {(logs[logFor] || []).join("\n") || "暂无日志输出"}
            </pre>
          </div>
        </div>
      )}

      {accounts.length === 0 && (
        <div className="mt-6 card border-[#6e5a35] bg-[#2a2418] text-[12px] text-[#e0c06a]">
          尚未添加任何游戏账户。启动游戏前请先到「账户」页添加（支持微软正版登录与离线模式）。
        </div>
      )}
    </div>
  );
}
