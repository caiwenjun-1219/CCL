// 模组 / 整合包推荐位：按 Modrinth 分类拉取热门项目，支持直接安装
import { useCallback, useEffect, useState } from "react";
import {
  Download,
  Loader2,
  Heart,
  AlertTriangle,
  Sparkles,
  RefreshCw,
  Package,
  ArrowDownToLine,
} from "lucide-react";
import { api, onInstallProgress, onModpackProgress, type ProjectHit } from "../lib/tauri";
import { useApp } from "../store/app";

// ---------------------------------------------------------------- 分类表

export const MOD_CATS: { slug: string; label: string }[] = [
  { slug: "", label: "综合热门" },
  { slug: "optimization", label: "性能优化" },
  { slug: "utility", label: "实用工具" },
  { slug: "decoration", label: "画面美化" },
  { slug: "technology", label: "科技机械" },
  { slug: "magic", label: "魔法" },
  { slug: "adventure", label: "冒险探索" },
  { slug: "storage", label: "仓储物流" },
  { slug: "worldgen", label: "地形生成" },
];

export const PACK_CATS: { slug: string; label: string }[] = [
  { slug: "", label: "综合热门" },
  { slug: "adventure", label: "冒险" },
  { slug: "technology", label: "科技" },
  { slug: "magic", label: "魔法" },
  { slug: "quests", label: "任务驱动" },
  { slug: "combat", label: "战斗" },
  { slug: "multiplayer", label: "多人联机" },
  { slug: "challenging", label: "硬核" },
  { slug: "optimization", label: "轻量优化" },
];

// ---------------------------------------------------------------- 小组件

function fmtCount(n: number): string {
  if (n >= 1e8) return (n / 1e8).toFixed(1).replace(/\.0$/, "") + " 亿";
  if (n >= 1e4) return (n / 1e4).toFixed(n >= 1e5 ? 0 : 1).replace(/\.0$/, "") + " 万";
  return n.toLocaleString("zh-CN");
}

function CatBar({
  cats,
  value,
  onChange,
}: {
  cats: { slug: string; label: string }[];
  value: string;
  onChange: (v: string) => void;
}) {
  return (
    <div className="flex flex-wrap items-center gap-1.5">
      {cats.map((c) => {
        const on = c.slug === value;
        return (
          <button
            key={c.slug || "all"}
            onClick={() => onChange(c.slug)}
            className={`h-7 px-3 rounded-[3px] text-[12px] transition-colors border ${
              on
                ? "bg-ccl-accent text-white border-ccl-accent font-semibold"
                : "bg-white text-ccl-gray1 border-ccl-gray6 hover:border-ccl-accent hover:text-ccl-accent"
            }`}
          >
            {c.label}
          </button>
        );
      })}
    </div>
  );
}

function Icon({ hit }: { hit: ProjectHit }) {
  return hit.icon_url ? (
    <img
      src={hit.icon_url}
      alt=""
      loading="lazy"
      className="w-10 h-10 rounded-[3px] object-cover bg-ccl-panel2 shrink-0"
    />
  ) : (
    <div className="w-10 h-10 rounded-[3px] bg-ccl-accent/10 border border-ccl-accent/25 flex items-center justify-center text-[12px] font-bold text-ccl-accent shrink-0">
      {hit.title.slice(0, 2).toUpperCase()}
    </div>
  );
}

function Meta({ hit }: { hit: ProjectHit }) {
  return (
    <div className="flex items-center gap-3 text-[10.5px] text-ccl-muted/80">
      <span className="inline-flex items-center gap-1">
        <ArrowDownToLine size={10} />
        {fmtCount(hit.downloads)}
      </span>
      <span className="inline-flex items-center gap-1">
        <Heart size={10} />
        {fmtCount(hit.follows)}
      </span>
      {hit.categories.length > 0 && (
        <span className="truncate text-ccl-muted/60">{hit.categories.slice(0, 2).join(" · ")}</span>
      )}
    </div>
  );
}

function Skeleton({ n, tall }: { n: number; tall?: boolean }) {
  return (
    <>
      {Array.from({ length: n }).map((_, i) => (
        <div
          key={i}
          className={`card !p-3 animate-pulse ${tall ? "h-[132px]" : "h-[104px]"} bg-white`}
        >
          <div className="flex gap-2.5">
            <div className="w-10 h-10 rounded-[3px] bg-ccl-panel2" />
            <div className="flex-1 space-y-1.5 pt-1">
              <div className="h-2.5 w-2/3 bg-ccl-panel2 rounded" />
              <div className="h-2 w-full bg-ccl-panel2/70 rounded" />
              <div className="h-2 w-4/5 bg-ccl-panel2/70 rounded" />
            </div>
          </div>
        </div>
      ))}
    </>
  );
}

// ---------------------------------------------------------------- 模组推荐

export function ModRecommend({ instanceId }: { instanceId: string }) {
  const { instances, showToast } = useApp();
  const inst = instances.find((i) => i.id === instanceId);
  const isVanilla = !!inst && inst.loader === "vanilla";

  const [cat, setCat] = useState("");
  const [hits, setHits] = useState<ProjectHit[]>([]);
  const [loading, setLoading] = useState(true);
  const [err, setErr] = useState<string | null>(null);
  const [openSlug, setOpenSlug] = useState<string | null>(null);
  const [versions, setVersions] = useState<
    { key: string; label: string; url: string; filename: string; sha1: string }[]
  >([]);
  const [vLoading, setVLoading] = useState(false);
  const [installing, setInstalling] = useState<string | null>(null);
  const [installed, setInstalled] = useState<string[]>([]);

  const gv = inst?.minecraft_version || null;
  const loader = inst && inst.loader !== "vanilla" ? inst.loader : null;

  const refreshInstalled = useCallback(async () => {
    if (!instanceId) return;
    try {
      const list = await api.listMods(instanceId);
      setInstalled(list.map((m) => m.file_name.toLowerCase()));
    } catch {
      setInstalled([]);
    }
  }, [instanceId]);

  useEffect(() => {
    refreshInstalled();
  }, [refreshInstalled]);

  /** 用 slug 粗匹配已安装的 jar（文件名通常以项目 slug 开头，如 sodium-fabric-0.6.jar） */
  const isInstalled = (slug: string) =>
    slug.length >= 5 && installed.some((n) => n.includes(slug.toLowerCase()));

  const load = useCallback(async () => {
    setLoading(true);
    setErr(null);
    setOpenSlug(null);
    try {
      setHits(await api.modrinthRecommend("mod", cat || null, gv, loader, "downloads", 0, 12));
    } catch (e) {
      setErr(String(e));
      setHits([]);
    } finally {
      setLoading(false);
    }
  }, [cat, gv, loader]);

  useEffect(() => {
    load();
  }, [load]);

  const openVersions = async (hit: ProjectHit) => {
    if (openSlug === hit.slug) {
      setOpenSlug(null);
      return;
    }
    setOpenSlug(hit.slug);
    setVersions([]);
    setVLoading(true);
    try {
      const list = await api.modrinthVersions(hit.slug, gv, loader);
      setVersions(
        list.map((v: Record<string, unknown>) => {
          const files = (v.files as { url: string; filename: string; sha1: string }[]) || [];
          const primary = files[0] || { url: "", filename: "", sha1: "" };
          return {
            key: String(v.id || v.version_number),
            label: `${v.version_number || v.name} · ${((v.game_versions as string[]) || [])
              .slice(0, 2)
              .join(", ")}`,
            url: primary.url,
            filename: primary.filename,
            sha1: primary.sha1 || "",
          };
        })
      );
    } catch (e) {
      showToast(String(e), "bad");
      setOpenSlug(null);
    } finally {
      setVLoading(false);
    }
  };

  const install = async (v: { filename: string; url: string; sha1: string }) => {
    if (!instanceId || !v.url) {
      showToast("没有可下载的文件", "bad");
      return;
    }
    setInstalling(v.filename);
    try {
      await api.modrinthInstall(instanceId, v.url, v.filename, v.sha1 || null);
      showToast(`${v.filename} 已装入 ${inst?.name || "实例"}`);
      setOpenSlug(null);
      refreshInstalled();
    } catch (e) {
      showToast(String(e), "bad");
    } finally {
      setInstalling(null);
    }
  };

  if (!instanceId) {
    return (
      <div className="card text-[12px] text-ccl-muted text-center py-10">
        还没有实例。先在「下载」页装一个版本，推荐就会按它的 MC 版本与加载器筛选。
      </div>
    );
  }

  return (
    <div>
      <div className="flex flex-wrap items-center gap-2 mb-2.5">
        <Sparkles size={14} className="text-ccl-accent" />
        <span className="text-[12px] font-bold">推荐模组</span>
        <span className="text-[11px] text-ccl-muted">
          {loader
            ? `按 ${inst?.minecraft_version} · ${loader} 筛选，按热度排序`
            : `按 ${inst?.minecraft_version} 筛选（未限定加载器），按热度排序`}
        </span>
        <button className="btn btn-ghost !py-1 !px-2 ml-auto" onClick={load} disabled={loading}>
          <RefreshCw size={11} className={loading ? "animate-spin" : ""} />
        </button>
      </div>

      <div className="mb-3.5">
        <CatBar cats={MOD_CATS} value={cat} onChange={setCat} />
      </div>

      {isVanilla && (
        <div className="flex items-start gap-2 mb-3.5 px-3 py-2 rounded-[3px] bg-ccl-warn/8 border border-ccl-warn/30">
          <AlertTriangle size={13} className="text-ccl-warn mt-0.5 shrink-0" />
          <p className="text-[11.5px] text-ccl-gray2 leading-relaxed">
            当前实例是<strong>原版</strong>，模组不会被加载。建议先用 Fabric / Forge / NeoForge 建一个模组实例，
            推荐内容会自动切换成对应加载器的可用模组。
          </p>
        </div>
      )}

      {err ? (
        <div className="card py-7 text-center">
          <p className="text-[12px] text-ccl-danger mb-2.5 px-4 break-all">{err}</p>
          <button className="btn btn-ghost !h-7 !text-[12px]" onClick={load}>
            <RefreshCw size={11} /> 重新加载
          </button>
        </div>
      ) : loading ? (
        <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-2.5">
          <Skeleton n={6} />
        </div>
      ) : hits.length === 0 ? (
        <div className="card text-[12px] text-ccl-muted py-8 text-center">
          该分类下暂无适配 {inst?.minecraft_version} 的内容，换一个分类试试。
        </div>
      ) : (
        <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-2.5">
          {hits.map((hit) => {
            const open = openSlug === hit.slug;
            return (
              <div
                key={hit.project_id}
                className={`card !p-3 transition-colors ${
                  open ? "border-ccl-accent/60 bg-ccl-brush8/40" : "hover:border-ccl-accent/45"
                }`}
              >
                <div className="flex items-start gap-2.5">
                  <Icon hit={hit} />
                  <div className="min-w-0 flex-1">
                    <div className="flex items-center gap-1.5">
                      <span className="text-[13px] font-bold truncate leading-tight">{hit.title}</span>
                      {isInstalled(hit.slug) && (
                        <span className="shrink-0 text-[9.5px] leading-[14px] px-1 rounded-[2px] bg-[#eaf7ef] text-[#2f8a57] border border-[#bfe6cf]">
                          已装
                        </span>
                      )}
                    </div>
                    <div className="text-[10.5px] text-ccl-muted truncate">by {hit.author}</div>
                    <p className="text-[11px] text-ccl-gray2 leading-snug line-clamp-2 mt-1">
                      {hit.description}
                    </p>
                  </div>
                </div>

                <div className="flex items-center mt-2.5 pt-2 border-t border-ccl-gray7">
                  <Meta hit={hit} />
                  <button
                    className="btn btn-primary !h-6 !px-2.5 !text-[11px] ml-auto"
                    onClick={() => openVersions(hit)}
                  >
                    {open ? "收起" : <><Download size={10} /> 安装</>}
                  </button>
                </div>

                {open && (
                  <div className="mt-2 bg-ccl-panel2 rounded-[3px] p-1.5 max-h-44 overflow-auto">
                    {vLoading ? (
                      <div className="text-[11px] text-ccl-muted text-center py-3">
                        <Loader2 size={13} className="animate-spin inline" /> 读取版本…
                      </div>
                    ) : versions.length === 0 ? (
                      <div className="text-[11px] text-ccl-muted text-center py-3">
                        没有适配当前实例的版本
                      </div>
                    ) : (
                      versions.map((v) => (
                        <button
                          key={v.key}
                          disabled={!!installing}
                          onClick={() => install(v)}
                          className="w-full text-left px-2 py-1.5 rounded-[2px] text-[11px] hover:bg-white flex items-center gap-2"
                        >
                          <span className="truncate flex-1">{v.label}</span>
                          {installing === v.filename ? (
                            <Loader2 size={11} className="animate-spin text-ccl-accent shrink-0" />
                          ) : (
                            <Download size={11} className="text-ccl-accent shrink-0" />
                          )}
                        </button>
                      ))
                    )}
                  </div>
                )}
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}

// ---------------------------------------------------------------- 整合包推荐

export function PackRecommend({ onInstalled }: { onInstalled?: () => void }) {
  const { showToast, loadInstances } = useApp();
  const [cat, setCat] = useState("");
  const [hits, setHits] = useState<ProjectHit[]>([]);
  const [loading, setLoading] = useState(true);
  const [err, setErr] = useState<string | null>(null);
  const [installingSlug, setInstallingSlug] = useState<string | null>(null);
  const [progress, setProgress] = useState<{ phase: string; done: number; total: number } | null>(null);

  useEffect(() => {
    let alive = true;
    const unsubs: (() => void)[] = [];
    const track = (p: { phase: string; done: number; total: number }) => {
      if (alive) setProgress(p);
    };
    // 整合包导入 = 装 MC 本体（install-progress）+ 下载模组（modpack-progress），两个都要跟
    onModpackProgress(track).then((f) => (alive ? unsubs.push(f) : f()));
    onInstallProgress(track).then((f) => (alive ? unsubs.push(f) : f()));
    return () => {
      alive = false;
      unsubs.forEach((f) => f());
    };
  }, []);

  const load = useCallback(async () => {
    setLoading(true);
    setErr(null);
    try {
      setHits(await api.modrinthRecommend("modpack", cat || null, null, null, "downloads", 0, 9));
    } catch (e) {
      setErr(String(e));
      setHits([]);
    } finally {
      setLoading(false);
    }
  }, [cat]);

  useEffect(() => {
    load();
  }, [load]);

  const install = async (hit: ProjectHit) => {
    setInstallingSlug(hit.slug);
    setProgress({ phase: "获取整合包信息", done: 0, total: 0 });
    try {
      const created = await api.modrinthInstallModpack(hit.slug, null);
      showToast(`整合包已就绪：${created.name}`, "ok");
      loadInstances();
      onInstalled?.();
    } catch (e) {
      showToast(String(e), "bad");
    } finally {
      setInstallingSlug(null);
      setProgress(null);
    }
  };

  const busy = !!installingSlug;

  return (
    <div>
      <div className="flex flex-wrap items-center gap-2 mb-2.5">
        <Package size={14} className="text-ccl-accent" />
        <span className="text-[12px] font-bold">精选整合包</span>
        <span className="text-[11px] text-ccl-muted">点一下直接装好 MC 版本、加载器与全部模组</span>
        <button className="btn btn-ghost !py-1 !px-2 ml-auto" onClick={load} disabled={loading || busy}>
          <RefreshCw size={11} className={loading ? "animate-spin" : ""} />
        </button>
      </div>

      <div className="mb-3.5">
        <CatBar cats={PACK_CATS} value={cat} onChange={setCat} />
      </div>

      {progress && (
        <div className="card !py-2.5 mb-3">
          <div className="flex justify-between text-[11px] text-ccl-muted mb-1.5">
            <span className="truncate">
              <Loader2 size={11} className="animate-spin inline mr-1" />
              {progress.phase}
            </span>
            {progress.total > 0 && (
              <span className="shrink-0 ml-2">
                {progress.done} / {progress.total}
              </span>
            )}
          </div>
          <div className="h-1.5 bg-ccl-gray7 rounded-[2px] overflow-hidden">
            <div
              className={`h-full bg-ccl-accent transition-all ${
                progress.total > 0 ? "" : "animate-pulse"
              }`}
              style={{
                width: progress.total > 0 ? `${(progress.done / progress.total) * 100}%` : "38%",
              }}
            />
          </div>
        </div>
      )}

      {err ? (
        <div className="card py-7 text-center">
          <p className="text-[12px] text-ccl-danger mb-2.5 px-4 break-all">{err}</p>
          <button className="btn btn-ghost !h-7 !text-[12px]" onClick={load}>
            <RefreshCw size={11} /> 重新加载
          </button>
        </div>
      ) : loading ? (
        <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-2.5">
          <Skeleton n={3} tall />
        </div>
      ) : hits.length === 0 ? (
        <div className="card text-[12px] text-ccl-muted py-8 text-center">该分类暂无整合包</div>
      ) : (
        <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-2.5">
          {hits.map((hit) => {
            const my = installingSlug === hit.slug;
            return (
              <div
                key={hit.project_id}
                className="card !p-3 flex flex-col hover:border-ccl-accent/45 transition-colors"
              >
                <div className="flex items-start gap-2.5">
                  <Icon hit={hit} />
                  <div className="min-w-0 flex-1">
                    <div className="text-[13px] font-bold truncate leading-tight">{hit.title}</div>
                    <div className="text-[10.5px] text-ccl-muted truncate">by {hit.author}</div>
                  </div>
                </div>
                <p className="text-[11px] text-ccl-gray2 leading-snug line-clamp-2 mt-2 flex-1">
                  {hit.description}
                </p>
                <div className="flex items-center mt-2.5 pt-2 border-t border-ccl-gray7">
                  <Meta hit={hit} />
                  <button
                    className="btn btn-primary !h-6 !px-2.5 !text-[11px] ml-auto"
                    disabled={busy}
                    onClick={() => install(hit)}
                  >
                    {my ? <Loader2 size={10} className="animate-spin" /> : <Download size={10} />}
                    {my ? "安装中" : "一键安装"}
                  </button>
                </div>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
