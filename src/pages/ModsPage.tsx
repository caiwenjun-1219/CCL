import { useEffect, useState } from "react";
import { useSearchParams } from "react-router-dom";
import { Search, Download, Trash2, Power, RefreshCw, FolderOpen, Loader2 } from "lucide-react";
import { useApp } from "../store/app";
import { api, fmtSize, type ModHit, type CfModHit, type ModInfo } from "../lib/tauri";

export default function ModsPage() {
  const [params] = useSearchParams();
  const { instances, settings, showToast } = useApp();
  const [instanceId, setInstanceId] = useState(params.get("instance") || "");
  const [tab, setTab] = useState<"modrinth" | "curseforge">("modrinth");
  const [query, setQuery] = useState("");
  const [hits, setHits] = useState<(ModHit | CfModHit)[]>([]);
  const [searching, setSearching] = useState(false);
  const [mods, setMods] = useState<ModInfo[]>([]);
  const [versions, setVersions] = useState<{ key: string; label: string; payload: Record<string, unknown> }[]>([]);
  const [pickingFor, setPickingFor] = useState<string | null>(null);
  const [installing, setInstalling] = useState(false);

  useEffect(() => {
    if (instances.length > 0 && !instanceId) setInstanceId(instances[0].id);
  }, [instances]);

  const refreshMods = async () => {
    if (!instanceId) return;
    setMods(await api.listMods(instanceId));
  };

  useEffect(() => {
    refreshMods();
  }, [instanceId]);

  const doSearch = async () => {
    if (!instanceId) return;
    setSearching(true);
    try {
      const gv = instances.find((i) => i.id === instanceId)?.minecraft_version || null;
      const loader = instances.find((i) => i.id === instanceId)?.loader;
      const loaderForSearch = loader && loader !== "vanilla" ? loader : null;
      if (tab === "modrinth") {
        setHits(await api.modrinthSearch(query, gv, loaderForSearch, 0));
      } else {
        if (!settings.curseforge_api_key) {
          showToast("请先在设置页填写 CurseForge API Key", "bad");
          setHits([]);
          return;
        }
        setHits(await api.cfSearch(query, gv, 0));
      }
    } catch (e) {
      showToast(String(e), "bad");
    } finally {
      setSearching(false);
    }
  };

  const pickVersion = async (hit: ModHit | CfModHit) => {
    if (!instanceId) return;
    setPickingFor(hitKey(hit));
    setVersions([]);
    try {
      const inst = instances.find((i) => i.id === instanceId)!;
      if (tab === "modrinth") {
        const list = await api.modrinthVersions((hit as ModHit).slug, inst.minecraft_version, inst.loader !== "vanilla" ? inst.loader : null);
        setVersions(
          list.map((v: Record<string, unknown>) => {
            const files = (v.files as { url: string; filename: string; sha1: string }[]) || [];
            const primary = files[0] || { url: "", filename: "", sha1: "" };
            return {
              key: String(v.version_number || v.name || v.id),
              label: `${v.version_number || v.name} · ${(v.game_versions as string[])?.slice(0, 3).join(", ")}`,
              payload: {
                source: "modrinth",
                download_url: primary.url,
                filename: primary.filename,
                sha1: primary.sha1,
              },
            };
          })
        );
      } else {
        const list = await api.cfFiles((hit as CfModHit).id, inst.minecraft_version);
        setVersions(
          list.map((f: Record<string, unknown>) => ({
            key: String(f.id),
            label: `${f.fileName}`,
            payload: {
              source: "curseforge",
              download_url: f.downloadUrl,
              filename: f.fileName,
            },
          }))
        );
      }
    } catch (e) {
      showToast(String(e), "bad");
    } finally {
      setPickingFor(null);
    }
  };

  const installVersion = async (payload: Record<string, unknown>) => {
    if (!instanceId) return;
    setInstalling(true);
    try {
      if (payload.source === "modrinth") {
        await api.modrinthInstall(
          instanceId,
          payload.download_url as string,
          payload.filename as string,
          (payload.sha1 as string) || null
        );
      } else {
        await api.cfInstall(instanceId, payload.download_url as string, payload.filename as string);
      }
      showToast("模组已安装");
      setVersions([]);
      refreshMods();
    } catch (e) {
      showToast(String(e), "bad");
    } finally {
      setInstalling(false);
    }
  };

  return (
    <div>
      <div className="flex flex-wrap items-center gap-3 mb-4">
        <h1 className="text-lg font-bold">模组市场</h1>
        <select className="field !w-52" value={instanceId} onChange={(e) => setInstanceId(e.target.value)}>
          {instances.map((i) => (
            <option key={i.id} value={i.id}>
              {i.name}
            </option>
          ))}
        </select>
        <div className="flex gap-1 bg-ccl-panel border border-ccl-border rounded-lg p-0.5">
          <button
            className={`px-3 py-1 rounded-md text-[12px] font-semibold ${tab === "modrinth" ? "bg-[#2f6b3a] text-white" : "text-ccl-muted"}`}
            onClick={() => setTab("modrinth")}
          >
            Modrinth
          </button>
          <button
            className={`px-3 py-1 rounded-md text-[12px] font-semibold ${tab === "curseforge" ? "bg-[#2f6b3a] text-white" : "text-ccl-muted"}`}
            onClick={() => setTab("curseforge")}
          >
            CurseForge
          </button>
        </div>
      </div>

      <div className="flex gap-2 mb-5">
        <input
          className="field"
          placeholder="搜索模组，如 JEI / 机械动力…"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && doSearch()}
        />
        <button className="btn btn-primary" onClick={doSearch} disabled={searching}>
          {searching ? <Loader2 size={13} className="animate-spin" /> : <Search size={13} />} 搜索
        </button>
      </div>

      <div className="grid grid-cols-1 lg:grid-cols-2 gap-5">
        {/* 搜索结果 */}
        <div className="card !p-0 overflow-hidden">
          <div className="px-4 py-2.5 border-b border-ccl-border text-[12px] font-bold text-ccl-muted">
            搜索结果（{hits.length}）
          </div>
          <div className="max-h-[520px] overflow-auto">
            {hits.length === 0 ? (
              <p className="text-[12px] text-ccl-muted p-4 text-center">
                输入关键词搜索模组，安装时会自动匹配当前实例的 MC 版本与加载器
              </p>
            ) : (
              hits.map((hit) => (
                <div key={hitKey(hit)} className="px-4 py-3 border-b border-ccl-border/50 hover:bg-white/3">
                  <div className="flex items-start gap-2.5">
                    {hit.icon_url ? (
                      <img src={hit.icon_url} className="w-9 h-9 rounded-lg object-cover bg-ccl-bg" alt="" />
                    ) : (
                      <div className="w-9 h-9 rounded-lg bg-[#223052] flex items-center justify-center text-[11px] font-bold text-[#9fb4ff]">
                        {hitName(hit).slice(0, 2).toUpperCase()}
                      </div>
                    )}
                    <div className="min-w-0 flex-1">
                      <div className="text-[13px] font-bold truncate">{hitName(hit)}</div>
                      <div className="text-[11px] text-ccl-muted line-clamp-2">
                        {"summary" in hit ? hit.summary : hit.description}
                      </div>
                      <div className="text-[10px] text-ccl-muted/70 mt-1">
                        {"author" in hit && hit.author ? `作者 ${hit.author} · ` : ""}
                        {("download_count" in hit ? hit.download_count : hit.downloads).toLocaleString()} 次下载
                      </div>
                    </div>
                    <button
                      className="btn btn-ghost !py-1 !px-2.5 shrink-0"
                      onClick={() => pickVersion(hit)}
                      disabled={pickingFor === hitKey(hit)}
                    >
                      {pickingFor === hitKey(hit) ? <Loader2 size={12} className="animate-spin" /> : <Download size={12} />}
                    </button>
                  </div>
                  {pickingFor === hitKey(hit) && versions.length > 0 && (
                    <div className="mt-2 bg-ccl-bg rounded-lg p-2 max-h-40 overflow-auto">
                      {versions.map((v) => (
                        <button
                          key={v.key}
                          className="w-full text-left text-[11px] px-2 py-1.5 rounded hover:bg-white/5 flex items-center justify-between gap-2"
                          disabled={installing}
                          onClick={() => installVersion(v.payload)}
                        >
                          <span className="truncate">{v.label}</span>
                          <Download size={11} className="shrink-0 text-ccl-accent2" />
                        </button>
                      ))}
                    </div>
                  )}
                </div>
              ))
            )}
          </div>
        </div>

        {/* 已安装模组 */}
        <div className="card !p-0 overflow-hidden">
          <div className="px-4 py-2.5 border-b border-ccl-border flex items-center">
            <span className="text-[12px] font-bold text-ccl-muted flex-1">已安装模组（{mods.length}）</span>
            <button className="btn btn-ghost !py-1 !px-2.5" onClick={refreshMods}>
              <RefreshCw size={11} />
            </button>
            <button
              className="btn btn-ghost !py-1 !px-2.5 ml-1"
              onClick={() => instanceId && api.openModsDir(instanceId)}
              title="打开模组目录"
            >
              <FolderOpen size={11} />
            </button>
          </div>
          <div className="max-h-[520px] overflow-auto">
            {mods.length === 0 ? (
              <p className="text-[12px] text-ccl-muted p-4 text-center">该实例还没有安装模组</p>
            ) : (
              mods.map((m) => (
                <div key={m.file_name} className="px-4 py-2.5 border-b border-ccl-border/50 flex items-center gap-2.5">
                  <Power
                    size={13}
                    className={m.enabled ? "text-ccl-accent2" : "text-ccl-muted/50"}
                  />
                  <div className="min-w-0 flex-1">
                    <div className={`text-[12px] font-semibold truncate ${m.enabled ? "" : "text-ccl-muted line-through"}`}>
                      {m.project_name}
                    </div>
                    <div className="text-[10px] text-ccl-muted/70 truncate">{m.file_name} · {fmtSize(m.size)}</div>
                  </div>
                  <button
                    className="btn btn-ghost !py-1 !px-2"
                    title={m.enabled ? "禁用" : "启用"}
                    onClick={async () => {
                      await api.toggleMod(instanceId, m.file_name, !m.enabled);
                      refreshMods();
                    }}
                  >
                    <Power size={11} />
                  </button>
                  <button
                    className="btn btn-danger !py-1 !px-2"
                    title="删除"
                    onClick={async () => {
                      if (!confirm(`删除模组 ${m.project_name}？`)) return;
                      await api.deleteMod(instanceId, m.file_name);
                      refreshMods();
                    }}
                  >
                    <Trash2 size={11} />
                  </button>
                </div>
              ))
            )}
          </div>
        </div>
      </div>
    </div>
  );
}

function hitKey(hit: ModHit | CfModHit): string {
  return "slug" in hit ? hit.slug : String(hit.id);
}

function hitName(hit: ModHit | CfModHit): string {
  return "title" in hit ? hit.title : hit.name;
}
