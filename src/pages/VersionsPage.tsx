import { useEffect, useState } from "react";
import { useSearchParams } from "react-router-dom";
import { Download, Trash2, RefreshCw, Loader2, Check } from "lucide-react";
import { useApp } from "../store/app";
import { api, onInstallProgress } from "../lib/tauri";

interface ManifestEntry {
  id: string;
  vtype: string;
  url: string;
}

export default function VersionsPage() {
  const [params] = useSearchParams();
  const { instances, loadInstances, showToast } = useApp();
  const [instanceId, setInstanceId] = useState(params.get("instance") || "");
  const [manifest, setManifest] = useState<ManifestEntry[]>([]);
  const [installed, setInstalled] = useState<string[]>([]);
  const [filter, setFilter] = useState("");
  const [onlyRelease, setOnlyRelease] = useState(true);
  const [downloading, setDownloading] = useState<string | null>(null);
  const [progress, setProgress] = useState<{ phase: string; done: number; total: number } | null>(null);

  // 加载器安装
  const [loaderKind, setLoaderKind] = useState("fabric");
  const [loaderVersions, setLoaderVersions] = useState<string[]>([]);
  const [loaderVersion, setLoaderVersion] = useState("");
  const [installingLoader, setInstallingLoader] = useState(false);

  useEffect(() => {
    if (instances.length > 0 && !instanceId) setInstanceId(instances[0].id);
  }, [instances]);

  useEffect(() => {
    api.versionManifest().then(setManifest).catch((e) => showToast(String(e), "bad"));
    let un: (() => void) | undefined;
    let alive = true;
    onInstallProgress(setProgress).then((f) => {
      if (alive) un = f;
    });
    return () => {
      alive = false;
      un?.();
    };
  }, []);

  const instance = instances.find((i) => i.id === instanceId);

  const refreshInstalled = async () => {
    if (!instance) return;
    setInstalled(await api.localVersions(instance.game_dir));
  };

  useEffect(() => {
    refreshInstalled();
  }, [instanceId, instances]);

  const install = async (versionId: string) => {
    if (!instance) return;
    setDownloading(versionId);
    setProgress({ phase: "开始下载", done: 0, total: 0 });
    try {
      await api.installVersion(instance.game_dir, versionId);
      // 原版安装后同步到实例，保证「就绪」状态与启动解析一致
      if (instance.installed_version_id !== versionId) {
        await api.updateInstance(instance.id, { installed_version_id: versionId });
        loadInstances();
      }
      showToast(`${versionId} 安装完成`);
      refreshInstalled();
    } catch (e) {
      showToast(String(e), "bad");
    } finally {
      setDownloading(null);
      setProgress(null);
    }
  };

  const installLoader = async () => {
    if (!instance || !loaderVersion) return;
    setInstallingLoader(true);
    try {
      const cmd =
        loaderKind === "fabric" || loaderKind === "quilt"
          ? api.installLoader(instance.game_dir, instance.minecraft_version, loaderKind, loaderVersion)
          : api.installForge(instance.game_dir, instance.minecraft_version, loaderKind, loaderVersion);
      const installedId = await cmd;
      await api.updateInstance(instance.id, { installed_version_id: installedId, loader_version: loaderVersion });
      showToast(`${loaderKind} 安装完成：${installedId}`);
      refreshInstalled();
    } catch (e) {
      showToast(String(e), "bad");
    } finally {
      setInstallingLoader(false);
      setProgress(null);
    }
  };

  useEffect(() => {
    if (!instance) return;
    const isForge = loaderKind === "forge" || loaderKind === "neoforge";
    const cmd = isForge
      ? api.forgeVersions(loaderKind, instance.minecraft_version)
      : api.loaderVersions(loaderKind, instance.minecraft_version);
    cmd
      .then((list) => {
        setLoaderVersions(list);
        setLoaderVersion(list[0] || "");
      })
      .catch(() => setLoaderVersions([]));
  }, [loaderKind, instance?.id, instance?.minecraft_version]);

  const filtered = manifest.filter((v) => {
    if (onlyRelease && v.vtype !== "release") return false;
    if (filter && !v.id.includes(filter)) return false;
    return true;
  });

  return (
    <div>
      <div className="flex items-center gap-3 mb-5">
        <h1 className="text-lg font-bold">版本管理</h1>
        <select className="field !w-56" value={instanceId} onChange={(e) => setInstanceId(e.target.value)}>
          {instances.map((i) => (
            <option key={i.id} value={i.id}>
              {i.name}
            </option>
          ))}
        </select>
        <div className="flex-1" />
        {progress && (
          <span className="text-[12px] text-ccl-muted flex items-center gap-2">
            <Loader2 size={12} className="animate-spin" />
            {progress.phase}
            {progress.total > 0 && ` ${Math.floor((progress.done / progress.total) * 100)}%`}
          </span>
        )}
      </div>

      {/* 加载器安装 */}
      {instance && (
        <div className="card mb-5">
          <h2 className="text-[13px] font-bold mb-3">安装模组加载器（当前实例：{instance.minecraft_version}）</h2>
          <div className="flex flex-wrap items-center gap-2">
            {["fabric", "quilt", "neoforge", "forge"].map((k) => (
              <button
                key={k}
                className={`btn !py-1.5 !px-3 ${loaderKind === k ? "btn-primary" : "btn-ghost"}`}
                onClick={() => setLoaderKind(k)}
              >
                {k === "neoforge" ? "NeoForge" : k === "fabric" ? "Fabric" : k === "quilt" ? "Quilt" : "Forge"}
              </button>
            ))}
            <select
              className="field !w-44"
              value={loaderVersion}
              onChange={(e) => setLoaderVersion(e.target.value)}
            >
              {loaderVersions.map((v) => (
                <option key={v} value={v}>
                  {v}
                </option>
              ))}
            </select>
            <button
              className="btn btn-primary"
              disabled={installingLoader || !loaderVersion}
              onClick={installLoader}
            >
              {installingLoader ? <Loader2 size={13} className="animate-spin" /> : <Download size={13} />}
              安装
            </button>
            {loaderVersions.length === 0 && (
              <span className="text-[12px] text-[#e0c06a]">该版本暂无此加载器</span>
            )}
          </div>
        </div>
      )}

      {/* 已安装 */}
      <div className="card mb-5">
        <div className="flex items-center justify-between mb-3">
          <h2 className="text-[13px] font-bold">已安装版本（{installed.length}）</h2>
          <button className="btn btn-ghost !py-1 !px-3" onClick={refreshInstalled}>
            <RefreshCw size={12} /> 刷新
          </button>
        </div>
        {installed.length === 0 ? (
          <p className="text-[12px] text-ccl-muted">尚未安装任何版本</p>
        ) : (
          <div className="flex flex-wrap gap-2">
            {installed.map((v) => (
              <span key={v} className="inline-flex items-center gap-2 tag tag-green !py-1.5 !px-3">
                {v}
                {v === instance?.installed_version_id && <Check size={11} />}
                <button
                  className="text-ccl-muted hover:text-red-400"
                  onClick={async () => {
                    if (!confirm(`删除版本 ${v}？`)) return;
                    await api.deleteVersion(instance!.game_dir, v);
                    refreshInstalled();
                  }}
                >
                  <Trash2 size={11} />
                </button>
              </span>
            ))}
          </div>
        )}
      </div>

      {/* 可安装 */}
      <div className="card">
        <div className="flex items-center gap-3 mb-3">
          <h2 className="text-[13px] font-bold">可安装版本</h2>
          <input
            className="field !w-48"
            placeholder="搜索版本号…"
            value={filter}
            onChange={(e) => setFilter(e.target.value)}
          />
          <label className="flex items-center gap-1.5 text-[12px] text-ccl-muted">
            <input type="checkbox" checked={onlyRelease} onChange={(e) => setOnlyRelease(e.target.checked)} />
            仅正式版
          </label>
        </div>
        <div className="grid grid-cols-2 md:grid-cols-3 lg:grid-cols-4 gap-2 max-h-96 overflow-auto">
          {filtered.map((v) => {
            const isInstalled = installed.includes(v.id);
            return (
              <div
                key={v.id}
                className="flex items-center justify-between bg-ccl-bg border border-ccl-border rounded-lg px-3 py-2"
              >
                <div>
                  <div className="text-[12px] font-semibold">{v.id}</div>
                  <div className="text-[10px] text-ccl-muted">{v.vtype === "release" ? "正式版" : "快照"}</div>
                </div>
                {isInstalled ? (
                  <Check size={14} className="text-ccl-accent2" />
                ) : (
                  <button
                    className="btn btn-ghost !py-1 !px-2"
                    disabled={downloading === v.id}
                    onClick={() => install(v.id)}
                    title="安装"
                  >
                    {downloading === v.id ? (
                      <Loader2 size={12} className="animate-spin" />
                    ) : (
                      <Download size={12} />
                    )}
                  </button>
                )}
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}
