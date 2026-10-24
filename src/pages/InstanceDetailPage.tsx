// 实例详情 / 版本设置（对齐 PCL 的「版本设置」页）
// 左侧竖排页签：概览 / 游戏设置 / 模组 / 存档 / 资源包 / 光影 / 日志

import { useEffect, useMemo, useRef, useState } from "react";
import { useNavigate, useParams, useSearchParams } from "react-router-dom";
import { open } from "@tauri-apps/plugin-dialog";
import {
  ArrowLeft,
  Play,
  Square,
  FolderOpen,
  LayoutGrid,
  Settings2,
  Puzzle,
  Save,
  Package,
  Sun,
  FileText,
  Loader2,
  Copy,
  Trash2,
  Coffee,
  HardDrive,
} from "lucide-react";
import { useApp } from "../store/app";
import { isMobile, MOBILE_LAUNCH_HINT } from "../lib/env";
import {
  api,
  cclUrl,
  fmtSize,
  type Instance,
  type InstancePaths,
  type JavaInfo,
} from "../lib/tauri";
import {
  SavesPanel,
  ResourcepacksPanel,
  ShaderpacksPanel,
  LogsPanel,
  ModsSummary,
} from "../components/InstanceRes";

const TABS = [
  { key: "overview", label: "概览", icon: LayoutGrid },
  { key: "settings", label: "游戏设置", icon: Settings2 },
  { key: "mods", label: "模组", icon: Puzzle },
  { key: "saves", label: "存档", icon: Save },
  { key: "resourcepacks", label: "资源包", icon: Package },
  { key: "shaderpacks", label: "光影", icon: Sun },
  { key: "logs", label: "日志", icon: FileText },
] as const;

type TabKey = (typeof TABS)[number]["key"];

const LOADER_LABEL: Record<string, string> = {
  vanilla: "原版",
  fabric: "Fabric",
  quilt: "Quilt",
  forge: "Forge",
  neoforge: "NeoForge",
};

function fmtTime(iso: string | null | undefined) {
  if (!iso) return "从未启动";
  const t = Date.parse(iso);
  if (Number.isNaN(t)) return "—";
  const diff = Date.now() - t;
  const m = Math.floor(diff / 60000);
  if (m < 1) return "刚刚";
  if (m < 60) return `${m} 分钟前`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h} 小时前`;
  const d = Math.floor(h / 24);
  if (d < 30) return `${d} 天前`;
  return new Date(t).toLocaleDateString("zh-CN");
}

export default function InstanceDetailPage() {
  const { id = "" } = useParams();
  const navigate = useNavigate();
  const [params, setParams] = useSearchParams();
  const { instances, loadInstances, running, setRunning, setCurrentInstance, showToast } = useApp();
  const [inst, setInst] = useState<Instance | null>(null);
  const [launching, setLaunching] = useState(false);

  const tab = (params.get("tab") as TabKey) || "overview";

  useEffect(() => {
    loadInstances();
  }, []);

  useEffect(() => {
    const found = instances.find((i) => i.id === id) ?? null;
    setInst(found);
    if (found) setCurrentInstance(found.id);
  }, [instances, id]);

  const isRunning = !!running[id];

  const play = async () => {
    if (!inst) return;
    if (isMobile()) {
      showToast(MOBILE_LAUNCH_HINT, "bad");
      return;
    }
    setLaunching(true);
    try {
      const info = await api.launchInstance(inst.id, null);
      setRunning(inst.id, true);
      showToast(`已启动 ${info.version_id}（Java ${info.java_major}）`);
    } catch (e) {
      showToast(String(e), "bad");
    } finally {
      setLaunching(false);
    }
  };

  const stop = async () => {
    try {
      await api.stopInstance(id);
      setRunning(id, false);
    } catch (e) {
      showToast(String(e), "bad");
    }
  };

  if (!inst) {
    return (
      <div className="p-8 text-center text-[13px] text-ccl-gray3">
        <Loader2 size={16} className="inline animate-spin mr-1.5" />
        正在读取实例…
      </div>
    );
  }

  return (
    <div className="h-full flex flex-col min-h-0">
      {/* ---------------- 顶栏 ---------------- */}
      <div className="flex items-center gap-3 pb-3 border-b border-ccl-border shrink-0">
        <button className="btn btn-ghost !px-2 !py-1.5" title="返回版本管理" onClick={() => navigate("/more/version")}>
          <ArrowLeft size={14} />
        </button>
        <div
          className="w-9 h-9 rounded-[3px] flex items-center justify-center text-white text-[15px] font-black shrink-0"
          style={{ background: "linear-gradient(140deg,#3d8bf7,#1370f3)" }}
        >
          {inst.name.slice(0, 1).toUpperCase()}
        </div>
        <div className="min-w-0">
          <div className="text-[15px] font-bold text-ccl-brush1 truncate leading-tight">{inst.name}</div>
          <div className="text-[11.5px] text-ccl-gray3 flex items-center gap-1.5 mt-0.5">
            <span>{inst.minecraft_version}</span>
            <span className="text-ccl-gray5">·</span>
            <span>
              {LOADER_LABEL[inst.loader] || inst.loader}
              {inst.loader_version ? ` ${inst.loader_version}` : ""}
            </span>
            <span className="text-ccl-gray5">·</span>
            <span>{fmtTime(inst.last_played)}</span>
          </div>
        </div>
        <div className="flex-1" />
        <button
          className="btn btn-ghost !px-2.5 !py-1.5"
          title="打开游戏目录"
          onClick={() => api.openInstanceDir(inst.id).catch((e) => showToast(String(e), "bad"))}
        >
          <FolderOpen size={14} />
        </button>
        {isRunning ? (
          <button className="btn btn-danger !px-3.5" onClick={stop}>
            <Square size={12} /> 结束游戏
          </button>
        ) : (
          <button className="btn btn-primary !px-4" onClick={play} disabled={launching}>
            {launching ? <Loader2 size={13} className="animate-spin" /> : <Play size={12} />} 启动游戏
          </button>
        )}
      </div>

      {/* ---------------- 主体 ---------------- */}
      <div className="flex flex-1 min-h-0 pt-3 gap-4">
        <aside className="w-[112px] shrink-0 space-y-0.5">
          {TABS.map((t) => {
            const Icon = t.icon;
            const active = tab === t.key;
            return (
              <button
                key={t.key}
                onClick={() => setParams({ tab: t.key }, { replace: true })}
                className={`w-full flex items-center gap-2 px-2.5 py-[7px] rounded-[3px] text-[12.5px] transition-colors ${
                  active
                    ? "bg-ccl-brush8 text-ccl-accent font-semibold"
                    : "text-ccl-gray1 hover:bg-ccl-gray8"
                }`}
              >
                <Icon size={13} className={active ? "text-ccl-accent" : "text-ccl-gray3"} />
                {t.label}
              </button>
            );
          })}
        </aside>

        <main className="flex-1 min-w-0 overflow-auto pr-1 pb-4">
          {tab === "overview" && <Overview inst={inst} onGoTab={(k) => setParams({ tab: k })} />}
          {tab === "settings" && (
            <SettingsPanel key={inst.id} inst={inst} onSaved={() => loadInstances()} />
          )}
          {tab === "mods" && (
            <div className="space-y-3">
              <ModsSummary instanceId={inst.id} />
              <div className="rounded-[3px] border border-ccl-border bg-white px-4 py-3.5">
                <div className="text-[12.5px] text-ccl-gray2 leading-relaxed">
                  模组列表、启停、批量操作与在线安装请到「模组管理」页，那里功能更完整。
                </div>
                <button
                  className="btn btn-primary mt-2.5 !py-1 !px-3"
                  onClick={() => navigate(`/more/mods?instance=${inst.id}`)}
                >
                  打开模组管理
                </button>
              </div>
            </div>
          )}
          {tab === "saves" && <SavesPanel instanceId={inst.id} />}
          {tab === "resourcepacks" && <ResourcepacksPanel instanceId={inst.id} />}
          {tab === "shaderpacks" && <ShaderpacksPanel instanceId={inst.id} />}
          {tab === "logs" && <LogsPanel instanceId={inst.id} />}
        </main>
      </div>
    </div>
  );
}

/* ------------------------------------------------------------------ 概览 */

function Overview({ inst, onGoTab }: { inst: Instance; onGoTab: (k: TabKey) => void }) {
  const [size, setSize] = useState<number | null>(null);
  const [shots, setShots] = useState<string[]>([]);
  const [paths, setPaths] = useState<InstancePaths | null>(null);
  const { showToast } = useApp();

  useEffect(() => {
    api.instanceSize(inst.id).then(setSize).catch(() => setSize(null));
    api.listScreenshots(inst.id).then(setShots).catch(() => setShots([]));
    api.instancePaths(inst.id).then(setPaths).catch(() => setPaths(null));
  }, [inst.id]);

  const rows: { label: string; value: React.ReactNode }[] = [
    { label: "游戏版本", value: inst.minecraft_version },
    {
      label: "模组加载器",
      value: `${LOADER_LABEL[inst.loader] || inst.loader}${inst.loader_version ? ` ${inst.loader_version}` : ""}`,
    },
    { label: "启动版本 ID", value: inst.installed_version_id || inst.minecraft_version },
    {
      label: "内存",
      value: `${inst.min_memory ? `${inst.min_memory} ~ ` : ""}${inst.max_memory || 4096} MB`,
    },
    { label: "Java", value: inst.java_path ? inst.java_path : "自动选择" },
    { label: "窗口", value: inst.window_width && inst.window_height ? `${inst.window_width} × ${inst.window_height}` : "跟随启动器" },
    { label: "磁盘占用", value: size === null ? "统计中…" : fmtSize(size) },
  ];

  return (
    <div className="space-y-3.5">
      <div className="rounded-[4px] border border-ccl-border bg-white p-4">
        <div className="text-[12.5px] font-semibold text-ccl-brush1 mb-2.5">版本信息</div>
        <div className="grid grid-cols-2 gap-x-6 gap-y-1.5">
          {rows.map((r) => (
            <div key={r.label} className="flex items-baseline gap-2 text-[12px] min-w-0">
              <span className="text-ccl-gray3 shrink-0 w-[76px] whitespace-nowrap">{r.label}</span>
              <span className="text-ccl-brush1 truncate" title={typeof r.value === "string" ? r.value : undefined}>
                {r.value}
              </span>
            </div>
          ))}
        </div>
        <div className="flex items-center gap-2 mt-3 pt-3 border-t border-ccl-gray7">
          <span className="text-ccl-gray3 text-[12px] shrink-0 w-[76px] whitespace-nowrap">MC 文件夹</span>
          <span
            className="text-[11.5px] text-ccl-gray2 font-mono truncate flex-1"
            title={paths?.mc_root || inst.game_dir}
          >
            {paths?.mc_root || inst.game_dir}
          </span>
          {paths && (
            <span
              className={`shrink-0 tag !text-[10px] !py-0 ${
                paths.isolated ? "tag-green" : "tag-orange"
              }`}
              title={paths?.isolation_label}
            >
              {paths.isolated ? "版本隔离" : "共享目录"}
            </span>
          )}
        </div>
        <div className="flex items-center gap-2 mt-2">
          <span className="text-ccl-gray3 text-[12px] shrink-0 w-[76px] whitespace-nowrap">游戏目录</span>
          <span
            className="text-[11.5px] text-ccl-gray2 font-mono truncate flex-1"
            title={paths?.game_dir || inst.game_dir}
          >
            {paths?.game_dir || inst.game_dir}
          </span>
          {paths?.legacy && (
            <span
              className="shrink-0 tag tag-orange !text-[10px] !py-0"
              title="旧版布局，可在「设置 → 启动设置 → 迁移旧实例」搬进 Minecraft 文件夹"
            >
              旧布局
            </span>
          )}
          <button
            className="btn btn-ghost !py-0.5 !px-1.5"
            title="复制路径"
            onClick={() =>
              navigator.clipboard.writeText(inst.game_dir).then(
                () => showToast("已复制路径"),
                () => showToast("复制失败", "bad")
              )
            }
          >
            <Copy size={11} />
          </button>
          <button
            className="btn btn-ghost !py-0.5 !px-1.5"
            onClick={() => api.openInstanceDir(inst.id).catch((e) => showToast(String(e), "bad"))}
          >
            <FolderOpen size={11} />
          </button>
        </div>
      </div>

      <div className="grid grid-cols-4 gap-2.5">
        {[
          { key: "mods" as TabKey, label: "模组", icon: Puzzle },
          { key: "saves" as TabKey, label: "存档", icon: Save },
          { key: "resourcepacks" as TabKey, label: "资源包", icon: Package },
          { key: "shaderpacks" as TabKey, label: "光影", icon: Sun },
        ].map((b) => {
          const Icon = b.icon;
          return (
            <button
              key={b.key}
              onClick={() => onGoTab(b.key)}
              className="rounded-[3px] border border-ccl-border bg-white py-3 flex flex-col items-center gap-1 hover:border-ccl-brush5 hover:bg-ccl-brush8/50 transition-colors"
            >
              <Icon size={16} className="text-ccl-accent" />
              <span className="text-[12px] text-ccl-brush1">{b.label}</span>
            </button>
          );
        })}
      </div>

      <div className="rounded-[4px] border border-ccl-border bg-white p-3">
        <div className="flex items-center justify-between mb-2">
          <span className="text-[12.5px] font-semibold text-ccl-brush1">游戏截图</span>
          <span className="text-[11px] text-ccl-gray3">{shots.length} 张</span>
        </div>
        {shots.length === 0 ? (
          <div className="py-6 text-center text-[12px] text-ccl-gray3">
            还没有截图。游戏内按 F2 截图后会显示在这里。
          </div>
        ) : (
          <div className="grid grid-cols-4 gap-2">
            {shots.slice(0, 8).map((s) => (
              <a
                key={s}
                href={cclUrl(`instances/${inst.id}/screenshots/${encodeURIComponent(s)}`)}
                target="_blank"
                rel="noreferrer"
                className="block aspect-video rounded-[2px] overflow-hidden border border-ccl-gray7 bg-ccl-gray8"
              >
                <img
                  src={cclUrl(`instances/${inst.id}/screenshots/${encodeURIComponent(s)}`)}
                  alt={s}
                  className="w-full h-full object-cover"
                  loading="lazy"
                />
              </a>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

/* -------------------------------------------------------------- 游戏设置 */

function SettingsPanel({ inst, onSaved }: { inst: Instance; onSaved: () => void }) {
  const { showToast, settings } = useApp();
  const [javaList, setJavaList] = useState<JavaInfo[]>([]);
  const [saving, setSaving] = useState(false);
  const [form, setForm] = useState({
    name: inst.name,
    min_memory: inst.min_memory ?? 0,
    max_memory: inst.max_memory ?? settings.max_memory ?? 4096,
    java_path: inst.java_path ?? "",
    jvm_args: inst.jvm_args ?? "",
    game_args: inst.game_args ?? "",
    window_width: inst.window_width ?? 0,
    window_height: inst.window_height ?? 0,
    fullscreen: inst.fullscreen ?? false,
    auto_join_server: inst.auto_join_server ?? "",
    version_isolation: inst.version_isolation ?? 0,
  });
  const timer = useRef<number | null>(null);
  /** 待提交的改动：连续编辑多个字段时合并成一次写入，避免互相覆盖 */
  const pending = useRef<Record<string, unknown>>({});

  useEffect(() => {
    api.scanJavas().then(setJavaList).catch(() => setJavaList([]));
  }, []);

  const flush = async () => {
    const body = pending.current;
    if (Object.keys(body).length === 0) return;
    pending.current = {};
    setSaving(true);
    try {
      await api.updateInstance(inst.id, body);
      onSaved();
    } catch (e) {
      showToast(String(e), "bad");
    } finally {
      setSaving(false);
    }
  };

  // 离开页面时把未提交的改动补写一次，防止丢设置
  useEffect(() => {
    return () => {
      if (timer.current) window.clearTimeout(timer.current);
      if (Object.keys(pending.current).length > 0) {
        const body = pending.current;
        pending.current = {};
        api.updateInstance(inst.id, body).catch(() => {});
      }
    };
  }, [inst.id]);

  // 输入变化 → 防抖自动保存（PCL 风格：改完即生效）
  const push = (patch: Record<string, unknown>) => {
    pending.current = { ...pending.current, ...patch };
    if (timer.current) window.clearTimeout(timer.current);
    timer.current = window.setTimeout(flush, 400);
  };

  const set = <K extends keyof typeof form>(key: K, value: (typeof form)[K]) => {
    setForm((f) => ({ ...f, [key]: value }));
    push({ [key]: value });
  };

  const pickJava = async () => {
    try {
      const picked = await open({
        multiple: false,
        filters: [{ name: "Java 可执行文件", extensions: ["exe"] }],
      });
      if (!picked || Array.isArray(picked)) return;
      set("java_path", picked as string);
    } catch (e) {
      showToast(String(e), "bad");
    }
  };

  const removeInstance = async () => {
    if (!confirm(`确定删除实例「${inst.name}」？\n游戏文件、存档、模组将一并删除，且不可恢复。`)) return;
    try {
      await api.deleteInstance(inst.id);
      showToast("实例已删除");
      onSaved();
      location.hash = "#/more/version";
    } catch (e) {
      showToast(String(e), "bad");
    }
  };

  const memGb = (form.max_memory / 1024).toFixed(1);

  return (
    <div className="space-y-3.5">
      {saving && (
        <div className="text-[11.5px] text-ccl-accent flex items-center gap-1">
          <Loader2 size={11} className="animate-spin" /> 正在保存…
        </div>
      )}

      {/* 基本信息 */}
      <Section title="基本信息">
        <Row label="实例名称">
          <input
            className="field !w-[240px]"
            value={form.name}
            onChange={(e) => setForm((f) => ({ ...f, name: e.target.value }))}
            onBlur={() => form.name.trim() && push({ name: form.name.trim() })}
          />
        </Row>
        <Row label="Minecraft">
          <span className="text-[12.5px] text-ccl-brush1">{inst.minecraft_version}</span>
        </Row>
        <Row label="加载器">
          <span className="text-[12.5px] text-ccl-brush1">
            {LOADER_LABEL[inst.loader] || inst.loader}
            {inst.loader_version ? ` ${inst.loader_version}` : ""}
          </span>
        </Row>
        <Row
          label="版本隔离"
          hint="隔离后存档、模组、资源包都存在实例文件夹里；不隔离则与 Minecraft 文件夹共用"
        >
          <select
            className="field !w-[240px]"
            value={form.version_isolation}
            onChange={(e) => set("version_isolation", Number(e.target.value))}
          >
            <option value={0}>跟随全局设置</option>
            <option value={1}>开启（独立存档与模组）</option>
            <option value={2}>关闭（共用 Minecraft 文件夹）</option>
          </select>
        </Row>
        {inst.folder_name && (
          <Row label="实例文件夹" hint="位于 <Minecraft 文件夹>\\versions\\ 下">
            <span className="text-[12px] text-ccl-brush1 font-mono">{inst.folder_name}</span>
          </Row>
        )}
      </Section>

      {/* 运行 */}
      <Section title="运行设置" icon={<Coffee size={13} />}>
        <Row label="最大内存" hint="建议设为物理内存的一半，分配过多反而拖慢启动">
          <div className="flex items-center gap-2.5">
            <input
              type="range"
              min={1024}
              max={32768}
              step={512}
              value={form.max_memory}
              onChange={(e) => set("max_memory", Number(e.target.value))}
              className="w-[200px] accent-[#1370f3]"
            />
            <input
              className="field !w-[92px] text-center"
              type="number"
              step={512}
              value={form.max_memory}
              onChange={(e) => set("max_memory", Number(e.target.value) || 1024)}
            />
            <span className="text-[11.5px] text-ccl-gray3 whitespace-nowrap">
              MB · {memGb} GB
            </span>
          </div>
        </Row>
        <Row label="最小内存" hint="对应 -Xms；留 0 表示不设置">
          <div className="flex items-center gap-2">
            <input
              className="field !w-[92px] text-center"
              type="number"
              step={512}
              value={form.min_memory}
              onChange={(e) => set("min_memory", Number(e.target.value) || 0)}
            />
            <span className="text-[11.5px] text-ccl-gray3">MB</span>
          </div>
        </Row>
        <Row label="Java" hint="不确定就用自动，会按 MC 版本要求挑选">
          <div className="flex items-center gap-1.5">
            <select
              className="field !w-[330px]"
              value={form.java_path}
              onChange={(e) => set("java_path", e.target.value)}
            >
              <option value="">自动选择（全局：{settings.java_path || "自动"}）</option>
              {javaList.map((j) => (
                <option key={j.path} value={j.path}>
                  Java {j.major} · {j.vendor} {j.version} — {j.path}
                </option>
              ))}
              {form.java_path && !javaList.some((j) => j.path === form.java_path) && (
                <option value={form.java_path}>已指定：{form.java_path}</option>
              )}
            </select>
            <button className="btn btn-ghost !py-1 !px-2" title="浏览 java.exe" onClick={pickJava}>
              <FolderOpen size={12} />
            </button>
          </div>
        </Row>
        <Row label="JVM 参数" hint="空格分隔，如 -XX:+UseG1GC；留空则使用全局设置">
          <input
            className="field !w-[440px] font-mono !text-[12px]"
            placeholder={settings.jvm_args || "（未设置）"}
            value={form.jvm_args}
            onChange={(e) => set("jvm_args", e.target.value)}
          />
        </Row>
        <Row label="游戏参数" hint="追加到游戏参数末尾">
          <input
            className="field !w-[440px] font-mono !text-[12px]"
            placeholder="（未设置）"
            value={form.game_args}
            onChange={(e) => set("game_args", e.target.value)}
          />
        </Row>
      </Section>

      {/* 窗口 */}
      <Section title="游戏窗口" icon={<HardDrive size={13} />}>
        <Row label="窗口尺寸" hint="留 0 表示跟随启动器窗口">
          <div className="flex items-center gap-1.5">
            <input
              className="field !w-[80px] text-center"
              type="number"
              value={form.window_width}
              onChange={(e) => set("window_width", Number(e.target.value) || 0)}
            />
            <span className="text-ccl-gray4 text-[12px]">×</span>
            <input
              className="field !w-[80px] text-center"
              type="number"
              value={form.window_height}
              onChange={(e) => set("window_height", Number(e.target.value) || 0)}
            />
          </div>
        </Row>
        <Row label="全屏启动">
          <button
            className="pcl-switch"
            data-on={form.fullscreen}
            onClick={() => set("fullscreen", !form.fullscreen)}
            title={form.fullscreen ? "已开启" : "已关闭"}
          />
        </Row>
        <Row label="自动进入服务器" hint="启动后直接连入该地址，格式 host 或 host:port">
          <input
            className="field !w-[300px] font-mono !text-[12px]"
            placeholder="mc.example.com:25565"
            value={form.auto_join_server}
            onChange={(e) => set("auto_join_server", e.target.value)}
          />
        </Row>
      </Section>

      {/* 危险区 */}
      <Section title="危险操作">
        <div className="flex items-center justify-between">
          <div className="text-[12px] text-ccl-gray2">
            删除实例会连同游戏文件、存档、模组一起删除，不可恢复。
          </div>
          <button className="btn btn-danger !py-1 !px-3" onClick={removeInstance}>
            <Trash2 size={12} /> 删除实例
          </button>
        </div>
      </Section>
    </div>
  );
}

function Section({
  title,
  icon,
  children,
}: {
  title: string;
  icon?: React.ReactNode;
  children: React.ReactNode;
}) {
  return (
    <div className="rounded-[4px] border border-ccl-border bg-white">
      <div className="px-4 py-2.5 border-b border-ccl-gray7 text-[12.5px] font-semibold text-ccl-brush1 flex items-center gap-1.5">
        {icon}
        {title}
      </div>
      <div className="px-4 py-1.5">{children}</div>
    </div>
  );
}

function Row({
  label,
  hint,
  children,
}: {
  label: string;
  hint?: string;
  children: React.ReactNode;
}) {
  return (
    <div className="flex items-center gap-3 py-[7px] border-b border-ccl-gray7/60 last:border-0">
      <div className="w-[92px] shrink-0 text-[12.5px] text-ccl-gray1">{label}</div>
      <div className="flex-1 min-w-0">
        {children}
        {hint && <div className="text-[11px] text-ccl-gray4 mt-[3px] leading-snug">{hint}</div>}
      </div>
    </div>
  );
}
