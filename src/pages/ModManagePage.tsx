import { useEffect, useMemo, useState } from "react";
import { useNavigate, useSearchParams } from "react-router-dom";
import {
  Search,
  Trash2,
  RefreshCw,
  FolderOpen,
  Loader2,
  Plus,
  Power,
  PowerOff,
  PackageOpen,
} from "lucide-react";
import { useApp } from "../store/app";
import { api, fmtSize, type ModInfo } from "../lib/tauri";

type Filter = "all" | "enabled" | "disabled";

export default function ModManagePage() {
  const [params] = useSearchParams();
  const navigate = useNavigate();
  const { instances, showToast } = useApp();
  const [instanceId, setInstanceId] = useState(params.get("instance") || "");
  const [mods, setMods] = useState<ModInfo[]>([]);
  const [loading, setLoading] = useState(false);
  const [query, setQuery] = useState("");
  const [filter, setFilter] = useState<Filter>("all");
  const [picked, setPicked] = useState<Set<string>>(new Set());
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (!instanceId && instances.length > 0) setInstanceId(instances[0].id);
  }, [instances, instanceId]);

  const inst = instances.find((i) => i.id === instanceId);

  const refresh = async () => {
    if (!instanceId) {
      setMods([]);
      return;
    }
    setLoading(true);
    try {
      setMods(await api.listMods(instanceId));
    } catch (e) {
      showToast(String(e), "bad");
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    setPicked(new Set());
    setQuery("");
    refresh();
  }, [instanceId]);

  const stats = useMemo(() => {
    const enabled = mods.filter((m) => m.enabled).length;
    const size = mods.reduce((s, m) => s + m.size, 0);
    return { total: mods.length, enabled, disabled: mods.length - enabled, size };
  }, [mods]);

  const shown = useMemo(() => {
    const q = query.trim().toLowerCase();
    return mods.filter((m) => {
      if (filter === "enabled" && !m.enabled) return false;
      if (filter === "disabled" && m.enabled) return false;
      if (!q) return true;
      return [m.mod_name, m.project_name, m.file_name, m.mod_id, m.mod_version]
        .filter(Boolean)
        .some((v) => String(v).toLowerCase().includes(q));
    });
  }, [mods, filter, query]);

  const allPicked = shown.length > 0 && shown.every((m) => picked.has(m.file_name));

  const togglePick = (fileName: string) => {
    setPicked((prev) => {
      const next = new Set(prev);
      next.has(fileName) ? next.delete(fileName) : next.add(fileName);
      return next;
    });
  };

  const togglePickAll = () => {
    setPicked(allPicked ? new Set() : new Set(shown.map((m) => m.file_name)));
  };

  const setEnabled = async (list: ModInfo[], enabled: boolean) => {
    const targets = list.filter((m) => m.enabled !== enabled);
    if (targets.length === 0) return;
    setBusy(true);
    let failed = 0;
    for (const m of targets) {
      try {
        await api.toggleMod(instanceId, m.file_name, enabled);
      } catch {
        failed += 1;
      }
    }
    await refresh();
    setBusy(false);
    // 批量操作完成后回到浏览态，避免批量条一直占着工具条
    if (list.length > 1) setPicked(new Set());
    if (failed > 0) showToast(`${failed} 个模组切换失败`, "bad");
    else showToast(enabled ? `已启用 ${targets.length} 个模组` : `已禁用 ${targets.length} 个模组`, "ok");
  };

  const removeMany = async (list: ModInfo[]) => {
    if (list.length === 0) return;
    if (!confirm(`确定删除这 ${list.length} 个模组文件？此操作不可撤销。`)) return;
    setBusy(true);
    let failed = 0;
    for (const m of list) {
      try {
        await api.deleteMod(instanceId, m.file_name);
      } catch {
        failed += 1;
      }
    }
    setPicked(new Set());
    await refresh();
    setBusy(false);
    if (failed > 0) showToast(`${failed} 个模组删除失败`, "bad");
    else showToast(`已删除 ${list.length} 个模组`, "ok");
  };

  const pickedMods = mods.filter((m) => picked.has(m.file_name));

  return (
    <div>
      {/* 标题区 */}
      <div className="flex flex-wrap items-center gap-3 mb-3">
        <h1 className="pcl-title">模组管理</h1>
        <select
          className="field !w-52"
          value={instanceId}
          onChange={(e) => setInstanceId(e.target.value)}
        >
          {instances.length === 0 && <option value="">（还没有实例）</option>}
          {instances.map((i) => (
            <option key={i.id} value={i.id}>
              {i.name}
            </option>
          ))}
        </select>
        <div className="flex-1" />
        <button
          className="btn btn-ghost"
          onClick={() => navigate(`/download/mod?instance=${instanceId}&view=search`)}
          disabled={!instanceId}
          title="去模组市场下载新模组"
        >
          <Plus size={13} /> 获取模组
        </button>
        <button className="btn btn-ghost" onClick={refresh} disabled={!instanceId || loading}>
          <RefreshCw size={13} className={loading ? "animate-spin" : ""} /> 刷新
        </button>
        <button
          className="btn btn-ghost"
          onClick={() => api.openModsDir(instanceId)}
          disabled={!instanceId}
          title="在资源管理器中打开 mods 目录"
        >
          <FolderOpen size={13} /> 目录
        </button>
      </div>

      {/* 统计条 */}
      {instanceId && (
        <div className="flex flex-wrap items-center gap-x-4 gap-y-1 text-[12px] text-ccl-gray2 mb-4">
          <span>
            实例 <b className="text-ccl-gray1">{inst?.name || "—"}</b>
          </span>
          <span className="text-ccl-gray5">|</span>
          <span>
            共 <b className="text-ccl-gray1">{stats.total}</b> 个
          </span>
          <span className="text-ccl-accent">
            启用 {stats.enabled}
          </span>
          <span className="text-ccl-gray3">禁用 {stats.disabled}</span>
          <span className="text-ccl-gray5">|</span>
          <span>占用 {fmtSize(stats.size)}</span>
          {inst && inst.loader === "vanilla" && (
            <span className="text-[#c08a1a] bg-[#fff7e6] border border-[#ffe0a3] rounded-[3px] px-2 py-0.5">
              原版实例，模组不会生效
            </span>
          )}
        </div>
      )}

      {/* 工具条 */}
      <div className="flex flex-wrap items-center gap-2 mb-3">
        <div className="relative flex-1 min-w-[200px] max-w-[320px]">
          <Search size={13} className="absolute left-2.5 top-1/2 -translate-y-1/2 text-ccl-gray3" />
          <input
            className="field !pl-8"
            placeholder="筛选模组名 / 文件名 / modId"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
        </div>

        <div className="flex gap-0.5 bg-ccl-panel2 border border-ccl-gray6 rounded-[3px] p-0.5">
          {(
            [
              ["all", `全部 ${stats.total}`],
              ["enabled", `启用 ${stats.enabled}`],
              ["disabled", `禁用 ${stats.disabled}`],
            ] as [Filter, string][]
          ).map(([k, label]) => (
            <button
              key={k}
              className={`px-3 h-[26px] rounded-[2px] text-[12px] font-semibold ${
                filter === k ? "bg-ccl-accent text-white" : "text-ccl-muted hover:text-ccl-accent"
              }`}
              onClick={() => setFilter(k)}
            >
              {label}
            </button>
          ))}
        </div>

        <div className="flex-1" />

        {picked.size > 0 ? (
          <>
            <span className="text-[12px] text-ccl-gray2">已选 {picked.size} 个</span>
            <button className="btn btn-ghost" onClick={() => setEnabled(pickedMods, true)} disabled={busy}>
              <Power size={12} /> 启用
            </button>
            <button className="btn btn-ghost" onClick={() => setEnabled(pickedMods, false)} disabled={busy}>
              <PowerOff size={12} /> 禁用
            </button>
            <button className="btn btn-danger" onClick={() => removeMany(pickedMods)} disabled={busy}>
              <Trash2 size={12} /> 删除
            </button>
            <button className="btn btn-ghost" onClick={() => setPicked(new Set())} disabled={busy}>
              取消
            </button>
          </>
        ) : (
          <>
            <button
              className="btn btn-ghost"
              onClick={() => setEnabled(shown, true)}
              disabled={busy || shown.length === 0}
            >
              <Power size={12} /> 全部启用
            </button>
            <button
              className="btn btn-ghost"
              onClick={() => setEnabled(shown, false)}
              disabled={busy || shown.length === 0}
            >
              <PowerOff size={12} /> 全部禁用
            </button>
          </>
        )}
      </div>

      {/* 列表 */}
      <div className="card !p-0 overflow-hidden">
        <div className="h-[34px] px-3 flex items-center gap-2 border-b border-ccl-border bg-ccl-gray8">
          <input
            type="checkbox"
            className="w-3.5 h-3.5 accent-ccl-accent cursor-pointer"
            checked={allPicked}
            onChange={togglePickAll}
            disabled={shown.length === 0}
            title="全选当前列表"
          />
          <span className="text-[12px] font-bold text-ccl-muted">
            {query || filter !== "all" ? `筛选结果 ${shown.length}` : `已安装模组 ${shown.length}`}
          </span>
          {busy && <Loader2 size={12} className="animate-spin text-ccl-accent" />}
        </div>

        {!instanceId ? (
          <EmptyState text="还没有可用实例，先在「版本管理」里创建一个。" />
        ) : loading && mods.length === 0 ? (
          <div className="py-16 flex items-center justify-center text-[12px] text-ccl-gray3 gap-2">
            <Loader2 size={14} className="animate-spin" /> 正在读取模组…
          </div>
        ) : shown.length === 0 ? (
          <EmptyState
            text={
              mods.length === 0
                ? "这个实例还没有装模组。"
                : "没有符合筛选条件的模组。"
            }
            action={
              mods.length === 0 ? (
                <button
                  className="btn btn-primary mt-3"
                  onClick={() => navigate(`/download/mod?instance=${instanceId}`)}
                >
                  <PackageOpen size={13} /> 去模组市场
                </button>
              ) : undefined
            }
          />
        ) : (
          <div className="max-h-[calc(100vh-330px)] overflow-auto">
            {shown.map((m) => {
              const title = m.mod_name || m.project_name;
              return (
                <div
                  key={m.file_name}
                  className={`px-3 py-2.5 border-b border-ccl-border/60 flex items-center gap-2.5 hover:bg-ccl-gray8 ${
                    picked.has(m.file_name) ? "bg-ccl-brush8/60" : ""
                  }`}
                >
                  <input
                    type="checkbox"
                    className="w-3.5 h-3.5 accent-ccl-accent cursor-pointer shrink-0"
                    checked={picked.has(m.file_name)}
                    onChange={() => togglePick(m.file_name)}
                  />
                  <div
                    className={`w-8 h-8 rounded-[3px] shrink-0 grid place-items-center text-[12px] font-bold ${
                      m.enabled
                        ? "bg-ccl-accent/10 text-ccl-accent border border-ccl-accent/25"
                        : "bg-ccl-gray7 text-ccl-gray3 border border-ccl-gray6"
                    }`}
                  >
                    {title.slice(0, 1).toUpperCase()}
                  </div>
                  <div className="min-w-0 flex-1">
                    <div className="flex items-center gap-1.5 min-w-0">
                      <span
                        className={`text-[12.5px] font-bold truncate ${
                          m.enabled ? "text-ccl-gray1" : "text-ccl-gray3"
                        }`}
                      >
                        {title}
                      </span>
                      {m.mod_version && (
                        <span className="text-[10px] text-ccl-gray3 bg-ccl-gray8 border border-ccl-gray6 rounded-[2px] px-1 shrink-0">
                          {m.mod_version}
                        </span>
                      )}
                      {!m.enabled && (
                        <span className="text-[10px] text-ccl-gray3 shrink-0">（已禁用）</span>
                      )}
                    </div>
                    <div className="text-[10.5px] text-ccl-gray3 truncate">
                      {m.mod_id ? `${m.mod_id} · ` : ""}
                      {m.file_name} · {fmtSize(m.size)}
                    </div>
                  </div>
                  <button
                    className="pcl-switch"
                    data-on={m.enabled}
                    disabled={busy}
                    title={m.enabled ? "禁用（改为 .jar.disabled）" : "启用"}
                    onClick={() => setEnabled([m], !m.enabled)}
                  >
                    <i />
                  </button>
                  <button
                    className="btn icon-danger !py-1 !px-2 shrink-0"
                    title="删除"
                    disabled={busy}
                    onClick={() => removeMany([m])}
                  >
                    <Trash2 size={11} />
                  </button>
                </div>
              );
            })}
          </div>
        )}
      </div>
    </div>
  );
}

function EmptyState({ text, action }: { text: string; action?: React.ReactNode }) {
  return (
    <div className="py-16 text-center">
      <PackageOpen size={30} className="mx-auto text-ccl-gray4 mb-2" />
      <p className="text-[12.5px] text-ccl-gray3">{text}</p>
      {action}
    </div>
  );
}
