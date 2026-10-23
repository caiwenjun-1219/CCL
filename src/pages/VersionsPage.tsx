import { useEffect, useMemo, useState } from "react";
import { useSearchParams } from "react-router-dom";
import {
  Download,
  Trash2,
  RefreshCw,
  Loader2,
  Check,
  FlaskConical,
  Plus,
  X,
} from "lucide-react";
import { useApp } from "../store/app";
import {
  api,
  fmtSize,
  onInstallProgress,
  onTestProgress,
  type ManifestEntry,
  type VersionTestReport,
} from "../lib/tauri";

const LOADER_LABEL: Record<string, string> = {
  fabric: "Fabric",
  quilt: "Quilt",
  forge: "Forge",
  neoforge: "NeoForge",
};

const TYPE_LABEL: Record<string, string> = {
  release: "正式版",
  snapshot: "测试版",
  old_beta: "Beta 版",
  old_alpha: "Alpha 版",
};

/** PCL 风格分区：正式版 / 测试版 / Beta / Alpha */
const GROUPS: { key: string; label: string }[] = [
  { key: "release", label: "正式版" },
  { key: "snapshot", label: "测试版" },
  { key: "old_beta", label: "Beta 版" },
  { key: "old_alpha", label: "Alpha 版" },
];

const FILTERS = [{ key: "all", label: "全部" }, ...GROUPS];

export default function VersionsPage() {
  const [params] = useSearchParams();
  const { instances, loadInstances, showToast } = useApp();
  const [instanceId, setInstanceId] = useState(params.get("instance") || "");
  const [manifest, setManifest] = useState<ManifestEntry[]>([]);
  const [manifestLoading, setManifestLoading] = useState(true);
  const [manifestError, setManifestError] = useState("");
  const [installed, setInstalled] = useState<string[]>([]);
  const [filter, setFilter] = useState("");
  const [groupFilter, setGroupFilter] = useState("all");
  const [selected, setSelected] = useState("");
  const [downloading, setDownloading] = useState<string | null>(null);
  const [progress, setProgress] = useState<{ phase: string; done: number; total: number } | null>(
    null
  );

  // 每个版本单独测试
  const [testing, setTesting] = useState<string | null>(null);
  const [testPhase, setTestPhase] = useState("");
  const [reports, setReports] = useState<Record<string, VersionTestReport>>({});

  // 加载器安装
  const [loaderKind, setLoaderKind] = useState("fabric");
  const [loaderVersions, setLoaderVersions] = useState<string[]>([]);
  const [loaderVersion, setLoaderVersion] = useState("");
  const [installingLoader, setInstallingLoader] = useState(false);
  const [creatingInstance, setCreatingInstance] = useState(false);
  const [loadingLoader, setLoadingLoader] = useState(false);
  const [loaderError, setLoaderError] = useState("");

  const instance = instances.find((i) => i.id === instanceId);

  useEffect(() => {
    if (instances.length > 0 && !instanceId) setInstanceId(instances[0].id);
  }, [instances]);

  const loadManifest = () => {
    setManifestLoading(true);
    setManifestError("");
    api
      .versionManifest()
      .then((list) => {
        setManifest(list);
        if (list.length === 0) setManifestError("版本清单为空，请重试");
      })
      .catch((e) => {
        setManifest([]);
        setManifestError(String(e));
      })
      .finally(() => setManifestLoading(false));
  };

  useEffect(() => {
    loadManifest();
    let un: (() => void) | undefined;
    let alive = true;
    onInstallProgress(setProgress).then((f) => {
      if (alive) un = f;
    });
    onTestProgress((p) => {
      if (alive) setTestPhase(p.phase);
    }).then((f) => {
      if (alive) f; // 保持订阅存活
      const orig = f;
      if (alive) un = un || orig;
    });
    return () => {
      alive = false;
      un?.();
    };
  }, []);

  const refreshInstalled = async () => {
    if (!instance) return;
    setInstalled(await api.localVersions(instance.game_dir));
  };

  useEffect(() => {
    refreshInstalled();
  }, [instanceId, instances]);

  // 默认选中：实例已安装版本 → 实例 MC 版本 → 最新正式版
  useEffect(() => {
    if (selected || manifest.length === 0) return;
    const prefer =
      (instance?.installed_version_id &&
        manifest.find((v) => v.id === instance.installed_version_id)) ||
      (instance && manifest.find((v) => v.id === instance.minecraft_version)) ||
      manifest.find((v) => v.vtype === "release");
    if (prefer) setSelected(prefer.id);
  }, [manifest, instanceId, instances, selected]);

  const selectedEntry = manifest.find((v) => v.id === selected);
  const mcVersion = selectedEntry?.id || instance?.minecraft_version || "";
  const selectedReport = selected ? reports[selected] : undefined;

  const latestRelease = manifest.find((v) => v.vtype === "release")?.id;
  const latestSnapshot = manifest.find((v) => v.vtype === "snapshot")?.id;

  const groups = useMemo(() => {
    const q = filter.trim().toLowerCase();
    return GROUPS.map((g) => ({
      ...g,
      items: manifest.filter(
        (v) =>
          v.vtype === g.key &&
          (groupFilter === "all" || groupFilter === g.key) &&
          (q === "" || v.id.toLowerCase().includes(q))
      ),
    })).filter((g) => g.items.length > 0);
  }, [manifest, filter, groupFilter]);

  const totalCount = groups.reduce((n, g) => n + g.items.length, 0);

  const install = async (versionId: string) => {
    if (!instance) return;
    setDownloading(versionId);
    setProgress({ phase: "开始下载", done: 0, total: 0 });
    try {
      await api.installVersion(instance.game_dir, versionId);
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

  // PCL 式「装完即一个独立版本」：直接为该版本创建新实例
  const installAsNewInstance = async (versionId: string) => {
    setCreatingInstance(true);
    setDownloading(versionId);
    setProgress({ phase: "创建实例", done: 0, total: 0 });
    try {
      const inst = await api.createInstance(versionId, versionId, "vanilla", null, null);
      setInstanceId(inst.id);
      setProgress({ phase: "开始下载", done: 0, total: 0 });
      await api.installVersion(inst.game_dir, versionId);
      await api.updateInstance(inst.id, { installed_version_id: versionId });
      loadInstances();
      showToast(`已创建实例「${versionId}」并完成安装`);
      refreshInstalled();
    } catch (e) {
      showToast(String(e), "bad");
    } finally {
      setDownloading(null);
      setCreatingInstance(false);
      setProgress(null);
    }
  };

  // 每个版本单独测试：隔离环境中完整安装并逐项校验
  const runTest = async (versionId: string) => {
    setTesting(versionId);
    setTestPhase("连接镜像源…");
    try {
      const report = await api.testVersion(versionId);
      setReports((r) => ({ ...r, [versionId]: report }));
      showToast(
        report.ok ? `${versionId} 单独测试通过` : `${versionId} 单独测试未通过`,
        report.ok ? "ok" : "bad"
      );
    } catch (e) {
      showToast(String(e), "bad");
    } finally {
      setTesting(null);
      setTestPhase("");
    }
  };

  const installLoader = async () => {
    if (!instance || !mcVersion || !loaderVersion) return;
    setInstallingLoader(true);
    try {
      const cmd =
        loaderKind === "fabric" || loaderKind === "quilt"
          ? api.installLoader(instance.game_dir, mcVersion, loaderKind, loaderVersion)
          : api.installForge(instance.game_dir, mcVersion, loaderKind, loaderVersion);
      const installedId = await cmd;
      await api.updateInstance(instance.id, {
        installed_version_id: installedId,
        loader_version: loaderVersion,
      });
      showToast(`${LOADER_LABEL[loaderKind]} 安装完成：${installedId}`);
      refreshInstalled();
    } catch (e) {
      showToast(String(e), "bad");
    } finally {
      setInstallingLoader(false);
      setProgress(null);
    }
  };

  useEffect(() => {
    if (!mcVersion) return;
    const isForge = loaderKind === "forge" || loaderKind === "neoforge";
    const cmd = isForge
      ? api.forgeVersions(loaderKind, mcVersion)
      : api.loaderVersions(loaderKind, mcVersion);
    setLoadingLoader(true);
    setLoaderError("");
    setLoaderVersions([]);
    setLoaderVersion("");
    cmd
      .then((list) => {
        setLoaderVersions(list);
        setLoaderVersion(list[0] || "");
        if (list.length === 0) setLoaderError(`该 MC 版本暂无 ${LOADER_LABEL[loaderKind]} 版本`);
      })
      .catch((e) => {
        setLoaderVersions([]);
        setLoaderError(String(e));
      })
      .finally(() => setLoadingLoader(false));
  }, [loaderKind, mcVersion]);

  if (instances.length === 0) {
    return (
      <div className="h-full flex flex-col items-center justify-center text-center">
        <Download size={40} className="text-ccl-muted mb-4" />
        <h2 className="text-lg font-bold mb-2">还没有游戏实例</h2>
        <p className="text-[13px] text-ccl-muted mb-6">
          下载页需要至少一个实例作为安装目标。你也可以直接在右侧选择「安装为新实例」。
          <br />
          所有版本均可在此浏览和单独测试，无需实例。
        </p>
        <button className="btn btn-primary" onClick={() => (window.location.hash = "#/instances")}>
          前往创建实例
        </button>
      </div>
    );
  }

  return (
    <div className="h-full flex flex-col min-h-0">
      {/* 顶部工具栏 */}
      <div className="flex flex-wrap items-center gap-2 mb-3 shrink-0">
        <h1 className="text-lg font-bold mr-1">下载</h1>
        <select
          className="field !w-44"
          value={instanceId}
          onChange={(e) => setInstanceId(e.target.value)}
          title="安装目标实例"
        >
          {instances.map((i) => (
            <option key={i.id} value={i.id}>
              {i.name}
            </option>
          ))}
        </select>
        <input
          className="field !w-48"
          placeholder="搜索版本号…"
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
        />
        <div className="flex gap-1">
          {FILTERS.map((g) => (
            <button
              key={g.key}
              className={`btn !py-1 !px-2.5 !text-[12px] ${
                groupFilter === g.key ? "btn-primary" : "btn-ghost"
              }`}
              onClick={() => setGroupFilter(g.key)}
            >
              {g.label}
            </button>
          ))}
        </div>
        <span className="text-[12px] text-ccl-muted">共 {totalCount} 个版本</span>
        <div className="flex-1" />
        {progress && (
          <span className="text-[12px] text-ccl-muted flex items-center gap-2">
            <Loader2 size={12} className="animate-spin" />
            {progress.phase}
            {progress.total > 0 && ` ${Math.floor((progress.done / progress.total) * 100)}%`}
          </span>
        )}
      </div>

      <div className="flex-1 flex gap-4 min-h-0">
        {/* 左：PCL 风格分区版本列表（全部版本） */}
        <div className="w-[320px] shrink-0 card !p-0 overflow-auto min-h-0">
          {manifestLoading && (
            <p className="p-4 text-[12px] text-ccl-muted flex items-center gap-2">
              <Loader2 size={12} className="animate-spin" /> 正在获取版本清单…
            </p>
          )}
          {!manifestLoading && manifestError && (
            <div className="p-4">
              <p className="text-[12px] text-ccl-warn mb-2">版本清单获取失败</p>
              <p className="text-[11px] text-ccl-muted mb-3 break-all">{manifestError}</p>
              <button className="btn btn-ghost !py-1 !px-2.5 !text-[12px]" onClick={loadManifest}>
                重试
              </button>
            </div>
          )}
          {!manifestLoading && !manifestError && groups.length === 0 && (
            <p className="p-4 text-[12px] text-ccl-muted">没有匹配的版本</p>
          )}
          {groups.map((g) => (
            <div key={g.key}>
              <div className="sticky top-0 z-10 bg-ccl-panel2 px-3 py-1.5 text-[11px] font-bold text-ccl-muted border-b border-ccl-border flex justify-between">
                <span>{g.label}</span>
                <span>{g.items.length}</span>
              </div>
              {g.items.map((v) => {
                const rep = reports[v.id];
                return (
                  <button
                    key={v.id}
                    onClick={() => setSelected(v.id)}
                    className={`w-full text-left px-3 py-1.5 flex items-center gap-2 border-b border-ccl-border/40 text-[12px] transition-colors ${
                      selected === v.id ? "bg-[#e4f3e4]" : "hover:bg-black/4"
                    }`}
                  >
                    <span className="font-semibold truncate flex-1">{v.id}</span>
                    {v.id === latestRelease && <span className="tag tag-green !py-0 !px-1.5">最新</span>}
                    {v.id === latestSnapshot && <span className="tag tag-orange !py-0 !px-1.5">快照</span>}
                    {installed.includes(v.id) && <Check size={12} className="text-ccl-accent2 shrink-0" />}
                    {rep &&
                      (rep.ok ? (
                        <span className="text-ccl-accent2 text-[10px] shrink-0" title="测试通过">✓</span>
                      ) : (
                        <span className="text-ccl-danger text-[10px] shrink-0" title="测试未通过">✗</span>
                      ))}
                    <span className="text-[10px] text-ccl-muted w-[62px] text-right shrink-0">
                      {v.release_time.slice(0, 10)}
                    </span>
                  </button>
                );
              })}
            </div>
          ))}
        </div>

        {/* 右：详情 + 操作 + 报告 */}
        <div className="flex-1 min-w-0 overflow-auto flex flex-col gap-4">
          {selectedEntry ? (
            <div className="card">
              <div className="flex items-start justify-between gap-3 mb-3">
                <div>
                  <h2 className="text-xl font-bold leading-tight">{selectedEntry.id}</h2>
                  <div className="flex flex-wrap gap-1.5 mt-1.5">
                    <span className="tag">{TYPE_LABEL[selectedEntry.vtype] || selectedEntry.vtype}</span>
                    {selectedEntry.id === latestRelease && <span className="tag tag-green">最新正式版</span>}
                    {selectedEntry.id === latestSnapshot && <span className="tag tag-orange">最新测试版</span>}
                    {installed.includes(selectedEntry.id) && <span className="tag tag-green">已安装</span>}
                    {selectedReport && (
                      <span className={`tag ${selectedReport.ok ? "tag-green" : "tag-orange"}`}>
                        {selectedReport.ok ? "测试通过" : "测试未通过"}
                      </span>
                    )}
                  </div>
                </div>
              </div>

              <div className="grid grid-cols-2 gap-x-6 gap-y-1 text-[12px] mb-4">
                <div>
                  <span className="text-ccl-muted">发布时间：</span>
                  {selectedEntry.release_time.slice(0, 10)}
                </div>
                <div>
                  <span className="text-ccl-muted">最后修改：</span>
                  {selectedEntry.time.slice(0, 10)}
                </div>
                <div className="col-span-2 truncate">
                  <span className="text-ccl-muted">清单地址：</span>
                  <span className="font-mono text-[11px]">{selectedEntry.url}</span>
                </div>
              </div>

              <div className="flex flex-wrap gap-2">
                <button
                  className="btn btn-primary"
                  disabled={!instance || downloading === selectedEntry.id}
                  onClick={() => install(selectedEntry.id)}
                >
                  {downloading === selectedEntry.id ? (
                    <Loader2 size={13} className="animate-spin" />
                  ) : (
                    <Download size={13} />
                  )}
                  {instance ? `安装到「${instance.name}」` : "请先创建实例"}
                </button>
                <button
                  className="btn btn-ghost"
                  disabled={!!downloading || creatingInstance}
                  onClick={() => installAsNewInstance(selectedEntry.id)}
                  title="为该版本单独创建一个实例并安装"
                >
                  <Plus size={13} /> 安装为新实例
                </button>
                <button
                  className="btn btn-ghost"
                  disabled={testing === selectedEntry.id}
                  onClick={() => runTest(selectedEntry.id)}
                  title="在隔离临时目录中完整安装并逐项校验，不影响任何实例"
                >
                  {testing === selectedEntry.id ? (
                    <Loader2 size={13} className="animate-spin" />
                  ) : (
                    <FlaskConical size={13} />
                  )}
                  单独测试
                </button>
              </div>

              {testing === selectedEntry.id && (
                <div className="mt-3 text-[12px] text-ccl-muted flex items-center gap-2">
                  <Loader2 size={12} className="animate-spin" />
                  正在测试 {selectedEntry.id}：{testPhase || "准备中"}
                  <span className="text-[10px]">（旧版本依赖较多，可能需要几分钟）</span>
                </div>
              )}

              {selectedReport && (
                <div className="mt-4 border border-ccl-border rounded-lg p-3">
                  <div className="flex items-center gap-2 mb-2 text-[13px] font-bold">
                    {selectedReport.ok ? (
                      <Check size={14} className="text-ccl-accent2" />
                    ) : (
                      <X size={14} className="text-ccl-danger" />
                    )}
                    单独测试报告 · {selectedReport.version_id}
                    <span className="ml-auto text-[11px] text-ccl-muted font-normal">
                      下载 {fmtSize(selectedReport.total_size)} · 耗时{" "}
                      {(selectedReport.elapsed_ms / 1000).toFixed(1)}s
                    </span>
                  </div>
                  <div className="flex flex-col gap-1">
                    {selectedReport.steps.map((s, i) => (
                      <div key={i} className="flex items-start gap-2 text-[12px]">
                        {s.ok ? (
                          <Check size={12} className="text-ccl-accent2 mt-0.5 shrink-0" />
                        ) : (
                          <X size={12} className="text-ccl-danger mt-0.5 shrink-0" />
                        )}
                        <span className="font-semibold shrink-0 w-[92px]">{s.name}</span>
                        <span className="text-ccl-muted break-all">{s.detail}</span>
                      </div>
                    ))}
                  </div>
                  {!selectedReport.ok && (
                    <div className="mt-2 text-[11px] text-ccl-warn break-all">
                      测试目录已保留：{selectedReport.test_dir}
                    </div>
                  )}
                </div>
              )}
            </div>
          ) : (
            <div className="card text-center text-ccl-muted text-[13px] py-10">
              从左侧选择一个版本
            </div>
          )}

          {/* 加载器安装（跟随选中版本，PCL 安装选项风格） */}
          {mcVersion && (
            <div className="card">
              <h2 className="text-[13px] font-bold mb-3">
                为 {mcVersion} 安装模组加载器
                {instance ? `（安装到「${instance.name}」）` : "（请先选择实例）"}
              </h2>
              <div className="flex flex-wrap items-center gap-2">
                {["fabric", "quilt", "neoforge", "forge"].map((k) => (
                  <button
                    key={k}
                    className={`btn !py-1.5 !px-3 ${
                      loaderKind === k ? "btn-primary" : "btn-ghost"
                    }`}
                    onClick={() => setLoaderKind(k)}
                  >
                    {LOADER_LABEL[k]}
                  </button>
                ))}
                {loadingLoader ? (
                  <span className="text-[12px] text-ccl-muted flex items-center gap-1.5">
                    <Loader2 size={12} className="animate-spin" /> 正在获取{" "}
                    {LOADER_LABEL[loaderKind]} 版本列表…
                  </span>
                ) : (
                  <select
                    className="field !w-44"
                    value={loaderVersion}
                    onChange={(e) => setLoaderVersion(e.target.value)}
                    disabled={loaderVersions.length === 0}
                  >
                    {loaderVersions.map((v) => (
                      <option key={v} value={v}>
                        {v}
                      </option>
                    ))}
                  </select>
                )}
                <button
                  className="btn btn-primary"
                  disabled={installingLoader || !loaderVersion || !instance}
                  onClick={installLoader}
                >
                  {installingLoader ? (
                    <Loader2 size={13} className="animate-spin" />
                  ) : (
                    <Download size={13} />
                  )}
                  安装
                </button>
                {!loadingLoader && loaderError && (
                  <span className="text-[12px] text-ccl-warn">{loaderError}</span>
                )}
              </div>
              {!loadingLoader && !loaderError && loaderVersions.length > 0 && (
                <p className="text-[11px] text-ccl-muted mt-2">
                  共 {loaderVersions.length} 个 {LOADER_LABEL[loaderKind]} 版本，默认选中最新稳定版
                </p>
              )}
            </div>
          )}

          {/* 已安装版本 */}
          <div className="card">
            <div className="flex items-center justify-between mb-3">
              <h2 className="text-[13px] font-bold">
                已安装版本（{installed.length}）
                {instance ? ` · ${instance.name}` : ""}
              </h2>
              <button className="btn btn-ghost !py-1 !px-3" onClick={refreshInstalled}>
                <RefreshCw size={12} /> 刷新
              </button>
            </div>
            {installed.length === 0 ? (
              <p className="text-[12px] text-ccl-muted">尚未安装任何版本</p>
            ) : (
              <div className="flex flex-wrap gap-2">
                {installed.map((v) => (
                  <span
                    key={v}
                    className="inline-flex items-center gap-2 tag tag-green !py-1.5 !px-3"
                  >
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
        </div>
      </div>
    </div>
  );
}
