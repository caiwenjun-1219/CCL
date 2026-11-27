import { useEffect, useState } from "react";
import { useNavigate, useSearchParams } from "react-router-dom";
import { Search, Download, Loader2, Sparkles, Wrench } from "lucide-react";
import { useApp } from "../store/app";
import { api, type ModHit, type CfModHit } from "../lib/tauri";
import { ModRecommend } from "../components/Recommend";

/**
 * 模组市场：只负责「找模组 + 装模组」。
 * 已安装模组的启停 / 删除 / 目录操作在独立的「更多 → 模组管理」页（/more/mods）。
 */
export default function ModsPage() {
  const [params] = useSearchParams();
  const navigate = useNavigate();
  const { instances, settings, showToast } = useApp();
  const [instanceId, setInstanceId] = useState(params.get("instance") || "");
  const [view, setView] = useState<"recommend" | "search">(
    params.get("view") === "search" ? "search" : "recommend"
  );
  const [tab, setTab] = useState<"modrinth" | "curseforge">("modrinth");
  const [query, setQuery] = useState("");
  const [hits, setHits] = useState<(ModHit | CfModHit)[]>([]);
  const [searching, setSearching] = useState(false);
  const [versions, setVersions] = useState<{ key: string; label: string; payload: Record<string, unknown> }[]>([]);
  const [pickingFor, setPickingFor] = useState<string | null>(null);
  const [installing, setInstalling] = useState(false);

  useEffect(() => {
    if (instances.length > 0 && !instanceId) setInstanceId(instances[0].id);
  }, [instances]);

  const inst = instances.find((i) => i.id === instanceId);

  const doSearch = async () => {
    if (!instanceId) return;
    setSearching(true);
    setVersions([]);
    try {
      const gv = inst?.minecraft_version || null;
      const loader = inst?.loader;
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
      const i = instances.find((x) => x.id === instanceId)!;
      if (tab === "modrinth") {
        const list = await api.modrinthVersions(
          (hit as ModHit).slug,
          i.minecraft_version,
          i.loader !== "vanilla" ? i.loader : null
        );
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
        const list = await api.cfFiles((hit as CfModHit).id, i.minecraft_version);
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
      showToast("模组已安装，可在「更多 → 模组管理」里启停");
      setVersions([]);
    } catch (e) {
      showToast(String(e), "bad");
    } finally {
      setInstalling(false);
    }
  };

  return (
    <div>
      <div className="flex flex-wrap items-center gap-3 mb-4">
        <h1 className="pcl-title">模组市场</h1>
        <select className="field !w-52" value={instanceId} onChange={(e) => setInstanceId(e.target.value)}>
          {instances.length === 0 && <option value="">（还没有实例）</option>}
          {instances.map((i) => (
            <option key={i.id} value={i.id}>
              {i.name}
            </option>
          ))}
        </select>

        {/* 推荐 / 搜索 */}
        <div className="flex gap-0.5 bg-ccl-panel2 border border-ccl-gray6 rounded-[3px] p-0.5">
          <button
            className={`flex items-center gap-1 px-3 h-[26px] rounded-[2px] text-[12px] font-semibold ${
              view === "recommend" ? "bg-ccl-accent text-white" : "text-ccl-muted hover:text-ccl-accent"
            }`}
            onClick={() => setView("recommend")}
          >
            <Sparkles size={11} /> 推荐
          </button>
          <button
            className={`flex items-center gap-1 px-3 h-[26px] rounded-[2px] text-[12px] font-semibold ${
              view === "search" ? "bg-ccl-accent text-white" : "text-ccl-muted hover:text-ccl-accent"
            }`}
            onClick={() => setView("search")}
          >
            <Search size={11} /> 搜索
          </button>
        </div>

        {view === "search" && (
          <div className="flex gap-0.5 bg-ccl-panel2 border border-ccl-gray6 rounded-[3px] p-0.5">
            <button
              className={`px-3 h-[26px] rounded-[2px] text-[12px] font-semibold ${
                tab === "modrinth" ? "bg-ccl-accent text-white" : "text-ccl-muted hover:text-ccl-accent"
              }`}
              onClick={() => setTab("modrinth")}
            >
              Modrinth
            </button>
            <button
              className={`px-3 h-[26px] rounded-[2px] text-[12px] font-semibold ${
                tab === "curseforge" ? "bg-ccl-accent text-white" : "text-ccl-muted hover:text-ccl-accent"
              }`}
              onClick={() => setTab("curseforge")}
            >
              CurseForge
            </button>
          </div>
        )}

        <div className="flex-1" />
        <button
          className="btn btn-ghost"
          onClick={() => navigate(`/more/mods?instance=${instanceId}`)}
          title="启停 / 删除已安装的模组"
        >
          <Wrench size={13} /> 模组管理
        </button>
      </div>

      {view === "recommend" ? (
        <ModRecommend instanceId={instanceId} />
      ) : (
        <>
          <div className="flex gap-2 mb-4">
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

          <div className="card !p-0 overflow-hidden">
            <div className="px-4 py-2.5 border-b border-ccl-border text-[12px] font-bold text-ccl-muted">
              搜索结果（{hits.length}）
            </div>
            <div className="max-h-[calc(100vh-300px)] overflow-auto">
              {hits.length === 0 ? (
                <p className="text-[12px] text-ccl-muted p-10 text-center">
                  输入关键词搜索模组，安装时会自动匹配当前实例的 MC 版本与加载器
                </p>
              ) : (
                hits.map((hit) => (
                  <div key={hitKey(hit)} className="px-4 py-3 border-b border-ccl-border/50 hover:bg-ccl-gray8">
                    <div className="flex items-start gap-2.5">
                      {hit.icon_url ? (
                        <img src={hit.icon_url} className="w-9 h-9 rounded-[3px] object-cover bg-ccl-bg" alt="" />
                      ) : (
                        <div className="w-9 h-9 rounded-[3px] bg-ccl-accent/10 border border-ccl-accent/25 flex items-center justify-center text-[11px] font-bold text-ccl-accent">
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
                      <div className="mt-2 bg-ccl-gray8 border border-ccl-gray6 rounded-[3px] p-2 max-h-40 overflow-auto">
                        {versions.map((v) => (
                          <button
                            key={v.key}
                            className="w-full text-left text-[11px] px-2 py-1.5 rounded-[2px] hover:bg-white flex items-center justify-between gap-2"
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
        </>
      )}
    </div>
  );
}

function hitKey(hit: ModHit | CfModHit): string {
  return "slug" in hit ? hit.slug : String(hit.id);
}

function hitName(hit: ModHit | CfModHit): string {
  return "title" in hit ? hit.title : hit.name;
}
