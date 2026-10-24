// 实例资源面板：存档 / 资源包 / 光影包 / 日志
// 均由实例详情页（InstanceDetailPage）内嵌渲染

import { useEffect, useMemo, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import {
  Archive,
  Copy,
  FolderOpen,
  Image as ImageIcon,
  Loader2,
  RefreshCw,
  Trash2,
  Upload,
  AlertTriangle,
} from "lucide-react";
import { useApp } from "../store/app";
import { api, fmtSize, type LogFile, type PackInfo, type SaveInfo } from "../lib/tauri";

/* ------------------------------------------------------------------ 通用 */

function Empty({ text, action }: { text: string; action?: React.ReactNode }) {
  return (
    <div className="py-12 text-center text-[12.5px] text-ccl-gray3">
      <div>{text}</div>
      {action && <div className="mt-3">{action}</div>}
    </div>
  );
}

function Loading() {
  return (
    <div className="py-12 text-center text-ccl-gray3 text-[12.5px]">
      <Loader2 size={16} className="inline animate-spin mr-1.5" />
      加载中…
    </div>
  );
}

function fmtTime(secs: number) {
  if (!secs) return "—";
  const d = new Date(secs * 1000);
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`;
}

function PanelHead({
  title,
  count,
  onOpenDir,
  onRefresh,
  extra,
}: {
  title: string;
  count: number;
  onOpenDir: () => void;
  onRefresh: () => void;
  extra?: React.ReactNode;
}) {
  return (
    <div className="flex items-center justify-between mb-3">
      <div className="text-[13px] font-semibold text-ccl-brush1">
        {title}
        <span className="ml-1.5 text-[12px] font-normal text-ccl-gray3">{count}</span>
      </div>
      <div className="flex items-center gap-1.5">
        {extra}
        <button className="btn btn-ghost !py-1 !px-2" title="刷新" onClick={onRefresh}>
          <RefreshCw size={12} />
        </button>
        <button className="btn btn-ghost !py-1 !px-2" title="打开文件夹" onClick={onOpenDir}>
          <FolderOpen size={12} />
        </button>
      </div>
    </div>
  );
}

/* ------------------------------------------------------------------ 存档 */

export function SavesPanel({ instanceId }: { instanceId: string }) {
  const { showToast } = useApp();
  const [list, setList] = useState<SaveInfo[]>([]);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState<string | null>(null);

  const load = () => {
    setLoading(true);
    api
      .listSaves(instanceId)
      .then(setList)
      .catch((e) => showToast(String(e), "bad"))
      .finally(() => setLoading(false));
  };

  useEffect(load, [instanceId]);

  const backup = async (dir: string) => {
    setBusy(dir);
    try {
      const dest = await api.backupSave(instanceId, dir);
      showToast(`已备份到 ${dest.split(/[\\/]/).pop()}`);
    } catch (e) {
      showToast(String(e), "bad");
    } finally {
      setBusy(null);
    }
  };

  const remove = async (dir: string, name: string) => {
    if (!confirm(`确定删除存档「${name}」？该操作不可撤销。\n建议先点「备份」再删除。`)) return;
    try {
      await api.deleteSave(instanceId, dir);
      showToast("已删除存档");
      load();
    } catch (e) {
      showToast(String(e), "bad");
    }
  };

  return (
    <div>
      <PanelHead
        title="存档"
        count={list.length}
        onOpenDir={() => api.openGameSubdir(instanceId, "saves").catch(() => {})}
        onRefresh={load}
      />
      {loading ? (
        <Loading />
      ) : list.length === 0 ? (
        <Empty text="还没有存档。进入游戏创建世界后会出现在这里。" />
      ) : (
        <div className="grid grid-cols-2 gap-2.5">
          {list.map((s) => (
            <div
              key={s.dir_name}
              className="flex items-center gap-3 p-2.5 rounded-[3px] border border-ccl-border bg-white hover:border-ccl-brush5 transition-colors"
            >
              <div className="w-12 h-12 rounded-[2px] bg-ccl-gray8 flex items-center justify-center overflow-hidden shrink-0">
                {s.icon ? (
                  <img src={s.icon} alt="" className="w-full h-full object-cover [image-rendering:pixelated]" />
                ) : (
                  <ImageIcon size={18} className="text-ccl-gray4" />
                )}
              </div>
              <div className="min-w-0 flex-1">
                <div className="text-[13px] font-semibold text-ccl-brush1 truncate" title={s.level_name}>
                  {s.level_name}
                </div>
                <div className="text-[11.5px] text-ccl-gray3 mt-0.5">
                  {fmtSize(s.size)} · {fmtTime(s.mtime)}
                </div>
              </div>
              <div className="flex items-center gap-0.5 shrink-0">
                <button
                  className="btn btn-ghost !py-1 !px-1.5"
                  title="备份为 zip"
                  disabled={busy === s.dir_name}
                  onClick={() => backup(s.dir_name)}
                >
                  {busy === s.dir_name ? (
                    <Loader2 size={12} className="animate-spin" />
                  ) : (
                    <Archive size={12} />
                  )}
                </button>
                <button
                  className="btn btn-ghost !py-1 !px-1.5 icon-danger"
                  title="删除"
                  onClick={() => remove(s.dir_name, s.level_name)}
                >
                  <Trash2 size={12} />
                </button>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

/* ------------------------------------------------------- 资源包 / 光影 */

function PackRow({
  pack,
  onToggle,
  onDelete,
  toggleTitle,
}: {
  pack: PackInfo;
  onToggle: () => void;
  onDelete: () => void;
  toggleTitle: string;
}) {
  return (
    <div className="flex items-center gap-3 px-3 py-2 border-b border-ccl-gray7/70 last:border-0 hover:bg-ccl-gray8/60">
      <button
        className={`w-4 h-4 rounded-[2px] border shrink-0 flex items-center justify-center transition-colors ${
          pack.enabled ? "bg-ccl-accent border-ccl-accent text-white" : "border-ccl-gray5 bg-white"
        }`}
        title={toggleTitle}
        onClick={onToggle}
      >
        {pack.enabled && <span className="text-[10px] leading-none">✓</span>}
      </button>
      <div className="min-w-0 flex-1">
        <div className="text-[12.5px] text-ccl-brush1 truncate flex items-center gap-1.5">
          <span className="truncate" title={pack.display_name}>
            {pack.display_name}
          </span>
          {pack.is_dir && <span className="tag !text-[10px] !py-0 shrink-0">文件夹</span>}
          {pack.incompatible && (
            <span className="tag tag-orange !text-[10px] !py-0 shrink-0" title="pack_format 高于当前游戏版本">
              版本不符
            </span>
          )}
        </div>
        {pack.description && (
          <div className="text-[11px] text-ccl-gray3 truncate" title={pack.file_name}>
            {pack.file_name}
          </div>
        )}
      </div>
      <div className="text-[11px] text-ccl-gray4 shrink-0 w-16 text-right">{fmtSize(pack.size)}</div>
      <button className="btn btn-ghost !py-1 !px-1.5 icon-danger shrink-0" title="删除" onClick={onDelete}>
        <Trash2 size={12} />
      </button>
    </div>
  );
}

export function ResourcepacksPanel({ instanceId }: { instanceId: string }) {
  const { showToast } = useApp();
  const [list, setList] = useState<PackInfo[]>([]);
  const [loading, setLoading] = useState(true);

  const load = () => {
    setLoading(true);
    api
      .listResourcepacks(instanceId)
      .then(setList)
      .catch((e) => showToast(String(e), "bad"))
      .finally(() => setLoading(false));
  };
  useEffect(load, [instanceId]);

  const toggle = async (p: PackInfo) => {
    try {
      await api.toggleResourcepack(instanceId, p.file_name, !p.enabled);
      load();
    } catch (e) {
      showToast(String(e), "bad");
    }
  };

  const remove = async (p: PackInfo) => {
    if (!confirm(`确定删除资源包「${p.display_name}」？`)) return;
    try {
      await api.deleteResourcepack(instanceId, p.file_name);
      showToast("已删除");
      load();
    } catch (e) {
      showToast(String(e), "bad");
    }
  };

  const importFile = async () => {
    try {
      const picked = await open({
        multiple: false,
        filters: [{ name: "资源包", extensions: ["zip"] }],
      });
      if (!picked || Array.isArray(picked)) return;
      await api.importPackFile(instanceId, "resourcepack", picked as string);
      showToast("已导入资源包");
      load();
    } catch (e) {
      showToast(String(e), "bad");
    }
  };

  const enabledCount = list.filter((p) => p.enabled).length;

  return (
    <div>
      <PanelHead
        title="资源包"
        count={list.length}
        onOpenDir={() => api.openGameSubdir(instanceId, "resourcepacks").catch(() => {})}
        onRefresh={load}
        extra={
          <button className="btn btn-ghost !py-1 !px-2" onClick={importFile} title="从文件导入">
            <Upload size={12} /> 导入
          </button>
        }
      />
      <p className="text-[11.5px] text-ccl-gray3 mb-2.5">
        已启用 {enabledCount} 个。勾选状态直接写入 options.txt，与游戏内设置等价；启用顺序靠下者优先。
      </p>
      {loading ? (
        <Loading />
      ) : list.length === 0 ? (
        <Empty
          text="暂无资源包。可点右上「导入」添加本地 zip，或到下载页获取。"
          action={
            <button className="btn btn-ghost" onClick={importFile}>
              <Upload size={12} /> 导入资源包
            </button>
          }
        />
      ) : (
        <div className="rounded-[3px] border border-ccl-border overflow-hidden bg-white">
          {list.map((p) => (
            <PackRow
              key={p.file_name}
              pack={p}
              toggleTitle={p.enabled ? "禁用" : "启用"}
              onToggle={() => toggle(p)}
              onDelete={() => remove(p)}
            />
          ))}
        </div>
      )}
    </div>
  );
}

export function ShaderpacksPanel({ instanceId }: { instanceId: string }) {
  const { showToast } = useApp();
  const [list, setList] = useState<PackInfo[]>([]);
  const [loading, setLoading] = useState(true);

  const load = () => {
    setLoading(true);
    api
      .listShaderpacks(instanceId)
      .then(setList)
      .catch((e) => showToast(String(e), "bad"))
      .finally(() => setLoading(false));
  };
  useEffect(load, [instanceId]);

  const setOne = async (name: string) => {
    try {
      await api.setShaderpack(instanceId, name);
      load();
    } catch (e) {
      showToast(String(e), "bad");
    }
  };

  const remove = async (p: PackInfo) => {
    if (!confirm(`确定删除光影包「${p.display_name}」？`)) return;
    try {
      await api.deleteShaderpack(instanceId, p.file_name);
      showToast("已删除");
      load();
    } catch (e) {
      showToast(String(e), "bad");
    }
  };

  const importFile = async () => {
    try {
      const picked = await open({
        multiple: false,
        filters: [{ name: "光影包", extensions: ["zip"] }],
      });
      if (!picked || Array.isArray(picked)) return;
      await api.importPackFile(instanceId, "shaderpack", picked as string);
      showToast("已导入光影包");
      load();
    } catch (e) {
      showToast(String(e), "bad");
    }
  };

  const active = list.find((p) => p.enabled);

  return (
    <div>
      <PanelHead
        title="光影包"
        count={list.length}
        onOpenDir={() => api.openGameSubdir(instanceId, "shaderpacks").catch(() => {})}
        onRefresh={load}
        extra={
          <button className="btn btn-ghost !py-1 !px-2" onClick={importFile} title="从文件导入">
            <Upload size={12} /> 导入
          </button>
        }
      />
      <p className="text-[11.5px] text-ccl-gray3 mb-2.5">
        同一时间只能启用一个光影。需要安装 Iris（Fabric/Quilt）或 OptiFine（Forge）才能生效。
      </p>
      {loading ? (
        <Loading />
      ) : list.length === 0 ? (
        <Empty
          text="暂无光影包。可点右上「导入」添加本地 zip。"
          action={
            <button className="btn btn-ghost" onClick={importFile}>
              <Upload size={12} /> 导入光影包
            </button>
          }
        />
      ) : (
        <>
          <div className="flex items-center justify-between mb-2 px-1">
            <span className="text-[12px] text-ccl-gray2">
              当前：<b className="text-ccl-brush1">{active ? active.display_name : "未启用光影"}</b>
            </span>
            {active && (
              <button className="btn btn-ghost !py-1 !px-2" onClick={() => setOne("OFF")}>
                关闭光影
              </button>
            )}
          </div>
          <div className="rounded-[3px] border border-ccl-border overflow-hidden bg-white">
            {list.map((p) => (
              <PackRow
                key={p.file_name}
                pack={p}
                toggleTitle={p.enabled ? "取消启用" : "启用该光影"}
                onToggle={() => setOne(p.enabled ? "OFF" : p.file_name)}
                onDelete={() => remove(p)}
              />
            ))}
          </div>
        </>
      )}
    </div>
  );
}

/* ------------------------------------------------------------------ 日志 */

const CRASH_HINTS: { re: RegExp; tip: string }[] = [
  { re: /java\.lang\.OutOfMemoryError/i, tip: "内存不足：到「游戏设置」调大最大内存" },
  { re: /java\.lang\.ClassNotFoundException|NoClassDefFoundError/i, tip: "缺少类：可能模组缺失依赖或版本不匹配" },
  { re: /Missing or unsupported mandatory dependencies|requires .* which is missing/i, tip: "模组依赖缺失：补装提示的前置模组" },
  { re: /java\.lang\.NoSuchMethodError|NoSuchFieldError/i, tip: "API 不匹配：模组与游戏/加载器版本不符" },
  { re: /GLFW error|Failed to create window|Pixel format not accelerated/i, tip: "显卡驱动或 OpenGL 异常：更新显卡驱动" },
  { re: /Duplicate mods|duplicate/i, tip: "模组重复：mods 目录里存在同一模组的多个版本" },
  { re: /UnsupportedClassVersionError/i, tip: "Java 版本过低：换用更高版本的 Java" },
];

export function LogsPanel({ instanceId }: { instanceId: string }) {
  const { showToast } = useApp();
  const [files, setFiles] = useState<LogFile[]>([]);
  const [current, setCurrent] = useState<string | null>(null);
  const [text, setText] = useState("");
  const [loading, setLoading] = useState(false);

  const loadFiles = () => {
    api
      .listGameLogs(instanceId)
      .then((f) => {
        setFiles(f);
        setCurrent((c) => c ?? f[0]?.name ?? null);
      })
      .catch((e) => showToast(String(e), "bad"));
  };
  useEffect(loadFiles, [instanceId]);

  useEffect(() => {
    if (!current) return;
    setLoading(true);
    api
      .readGameLog(instanceId, current, 600)
      .then(setText)
      .catch((e) => setText(`读取失败：${e}`))
      .finally(() => setLoading(false));
  }, [current, instanceId]);

  const hints = useMemo(() => {
    const hit: string[] = [];
    for (const h of CRASH_HINTS) if (h.re.test(text) && !hit.includes(h.tip)) hit.push(h.tip);
    return hit;
  }, [text]);

  return (
    <div>
      <PanelHead
        title="日志"
        count={files.length}
        onOpenDir={() => api.openGameSubdir(instanceId, "logs").catch(() => {})}
        onRefresh={() => {
          loadFiles();
          if (current) api.readGameLog(instanceId, current, 600).then(setText).catch(() => {});
        }}
        extra={
          <button
            className="btn btn-ghost !py-1 !px-2"
            title="复制到剪贴板"
            onClick={() => {
              navigator.clipboard.writeText(text).then(
                () => showToast("已复制"),
                () => showToast("复制失败", "bad")
              );
            }}
          >
            <Copy size={12} /> 复制
          </button>
        }
      />
      {files.length === 0 ? (
        <Empty text="还没有日志。启动一次游戏后会生成 logs/latest.log。" />
      ) : (
        <div className="flex gap-3 h-[calc(100vh-260px)] min-h-[280px]">
          <div className="w-[190px] shrink-0 overflow-auto rounded-[3px] border border-ccl-border bg-white">
            {files.map((f) => (
              <button
                key={f.name}
                onClick={() => setCurrent(f.name)}
                className={`w-full text-left px-2.5 py-1.5 border-b border-ccl-gray7/70 last:border-0 ${
                  current === f.name ? "bg-ccl-brush8 text-ccl-accent" : "hover:bg-ccl-gray8"
                }`}
              >
                <div className="text-[12px] truncate flex items-center gap-1">
                  {f.name.startsWith("crash-reports/") && (
                    <AlertTriangle size={11} className="text-ccl-warn shrink-0" />
                  )}
                  <span className="truncate" title={f.name}>
                    {f.name.replace("crash-reports/", "")}
                  </span>
                </div>
                <div className="text-[10.5px] text-ccl-gray4">
                  {fmtSize(f.size)} · {fmtTime(f.mtime)}
                </div>
              </button>
            ))}
          </div>
          <div className="flex-1 min-w-0 flex flex-col gap-2">
            {hints.length > 0 && (
              <div className="rounded-[3px] border border-ccl-warn/40 bg-[#fffaf0] px-3 py-2 text-[12px] text-[#8a5d16]">
                <div className="font-semibold mb-0.5 flex items-center gap-1">
                  <AlertTriangle size={12} /> 检测到可能的崩溃原因
                </div>
                <ul className="list-disc pl-4 space-y-0.5">
                  {hints.map((h) => (
                    <li key={h}>{h}</li>
                  ))}
                </ul>
              </div>
            )}
            <div className="flex-1 min-h-0 rounded-[3px] border border-ccl-border bg-[#1e1f22] overflow-auto">
              {loading ? (
                <div className="p-4 text-[12px] text-ccl-gray4">
                  <Loader2 size={13} className="inline animate-spin mr-1.5" /> 读取中…
                </div>
              ) : (
                <pre className="p-3 text-[11.5px] leading-[1.55] text-[#d4d4d4] font-mono whitespace-pre-wrap break-all">
                  {text || "（空文件）"}
                </pre>
              )}
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

/* ------------------------------------------------------- 模组摘要卡片 */

export function ModsSummary({ instanceId }: { instanceId: string }) {
  const { showToast } = useApp();
  const [mods, setMods] = useState<{ enabled: boolean; size: number }[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    api
      .listMods(instanceId)
      .then((m) => setMods(m))
      .catch((e) => showToast(String(e), "bad"))
      .finally(() => setLoading(false));
  }, [instanceId]);

  const total = mods.length;
  const enabled = mods.filter((m) => m.enabled).length;
  const size = mods.reduce((a, b) => a + b.size, 0);

  return (
    <div className="grid grid-cols-3 gap-2.5">
      {[
        { label: "已安装模组", value: loading ? "—" : String(total) },
        { label: "已启用", value: loading ? "—" : String(enabled) },
        { label: "占用空间", value: loading ? "—" : fmtSize(size) },
      ].map((s) => (
        <div key={s.label} className="rounded-[3px] border border-ccl-border bg-white px-3 py-2.5">
          <div className="text-[11.5px] text-ccl-gray3">{s.label}</div>
          <div className="text-[16px] font-bold text-ccl-brush1 mt-0.5">{s.value}</div>
        </div>
      ))}
    </div>
  );
}
