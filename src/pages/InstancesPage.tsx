import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import { Plus, Trash2, FolderOpen, Puzzle, RefreshCw, Loader2 } from "lucide-react";
import { useApp } from "../store/app";
import { api } from "../lib/tauri";

const LOADERS = [
  { id: "vanilla", label: "原版", desc: "纯净游戏，无任何模组加载器" },
  { id: "fabric", label: "Fabric", desc: "轻量快速，模组生态最大" },
  { id: "quilt", label: "Quilt", desc: "Fabric 分支，兼容 Fabric 模组" },
  { id: "neoforge", label: "NeoForge", desc: "Forge 继任者，大型模组包首选" },
  { id: "forge", label: "Forge", desc: "老牌加载器，1.20.2 及以下常用" },
];

export default function InstancesPage() {
  const navigate = useNavigate();
  const { instances, loadInstances, showToast } = useApp();
  const [creating, setCreating] = useState(false);

  useEffect(() => {
    loadInstances();
  }, []);

  const remove = async (id: string, name: string) => {
    if (!confirm(`确定删除实例「${name}」？游戏文件将一并删除。`)) return;
    try {
      await api.deleteInstance(id);
      showToast("已删除");
      loadInstances();
    } catch (e) {
      showToast(String(e), "bad");
    }
  };

  return (
    <div>
      <div className="flex items-center justify-between mb-5">
        <h1 className="text-lg font-bold">实例管理</h1>
        <button className="btn btn-primary" onClick={() => setCreating(true)}>
          <Plus size={13} /> 新建实例
        </button>
      </div>

      {instances.length === 0 ? (
        <div className="card text-center py-10 text-ccl-muted text-[13px]">
          暂无实例，点击右上角「新建实例」开始
        </div>
      ) : (
        <div className="card !p-0 overflow-hidden">
          <table className="w-full text-[13px]">
            <thead>
              <tr className="text-ccl-muted text-[12px] border-b border-ccl-border">
                <th className="text-left px-4 py-2.5 font-medium">名称</th>
                <th className="text-left px-4 py-2.5 font-medium">MC 版本</th>
                <th className="text-left px-4 py-2.5 font-medium">加载器</th>
                <th className="text-left px-4 py-2.5 font-medium">内存</th>
                <th className="text-left px-4 py-2.5 font-medium">状态</th>
                <th className="text-right px-4 py-2.5 font-medium">操作</th>
              </tr>
            </thead>
            <tbody>
              {instances.map((inst) => (
                <tr key={inst.id} className="border-b border-ccl-border/50 hover:bg-white/3">
                  <td className="px-4 py-2.5 font-semibold">{inst.name}</td>
                  <td className="px-4 py-2.5 text-ccl-muted">{inst.minecraft_version}</td>
                  <td className="px-4 py-2.5">
                    <span className="tag">{LOADERS.find((l) => l.id === inst.loader)?.label || inst.loader}</span>
                    {inst.loader_version && (
                      <span className="ml-1.5 text-[11px] text-ccl-muted">{inst.loader_version}</span>
                    )}
                  </td>
                  <td className="px-4 py-2.5 text-ccl-muted">{inst.max_memory || 4096} MB</td>
                  <td className="px-4 py-2.5">
                    {inst.loader === "vanilla" || inst.installed_version_id ? (
                      <span className="tag tag-green">就绪</span>
                    ) : (
                      <span className="tag tag-orange">待安装版本</span>
                    )}
                  </td>
                  <td className="px-4 py-2.5 text-right whitespace-nowrap">
                    <button
                      className="btn btn-ghost !py-1 !px-2.5"
                      title="模组管理"
                      onClick={() => navigate(`/mods?instance=${inst.id}`)}
                    >
                      <Puzzle size={13} />
                    </button>
                    <button
                      className="btn btn-ghost !py-1 !px-2.5 ml-1"
                      title="打开目录"
                      onClick={() => api.openInstanceDir(inst.id).catch((e) => showToast(String(e), "bad"))}
                    >
                      <FolderOpen size={13} />
                    </button>
                    <button
                      className="btn btn-danger !py-1 !px-2.5 ml-1"
                      title="删除"
                      onClick={() => remove(inst.id, inst.name)}
                    >
                      <Trash2 size={13} />
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      {creating && <CreateWizard onClose={() => { setCreating(false); loadInstances(); }} />}
    </div>
  );
}

function CreateWizard({ onClose }: { onClose: () => void }) {
  const { showToast } = useApp();
  const [step, setStep] = useState(0);
  const [name, setName] = useState("新实例");
  const [mcVersion, setMcVersion] = useState("1.21.11");
  const [loader, setLoader] = useState("vanilla");
  const [loaderVersion, setLoaderVersion] = useState("");
  const [loaderVersions, setLoaderVersions] = useState<string[]>([]);
  const [memory, setMemory] = useState(4096);
  const [manifest, setManifest] = useState<{ id: string; vtype: string }[]>([]);
  const [loadingVersions, setLoadingVersions] = useState(false);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    setLoadingVersions(true);
    api
      .versionManifest()
      .then((list) => {
        setManifest(list);
        if (!list.find((v) => v.id === mcVersion)) {
          const firstRelease = list.find((v) => v.vtype === "release");
          if (firstRelease) setMcVersion(firstRelease.id);
        }
      })
      .catch((e) => showToast(String(e), "bad"))
      .finally(() => setLoadingVersions(false));
  }, []);

  useEffect(() => {
    if (loader === "vanilla") return;
    setLoadingVersions(true);
    api
      .loaderVersions(loader, mcVersion)
      .then((list) => {
        setLoaderVersions(list);
        setLoaderVersion(list[0] || "");
      })
      .catch((e) => showToast(`${loader} 版本列表获取失败: ${e}`, "bad"))
      .finally(() => setLoadingVersions(false));
  }, [loader, mcVersion]);

  const releases = manifest.filter((v) => v.vtype === "release");
  const snapshots = manifest.filter((v) => v.vtype !== "release");

  const submit = async () => {
    setSaving(true);
    try {
      await api.createInstance(
        name,
        mcVersion,
        loader,
        loader === "vanilla" ? null : loaderVersion,
        memory
      );
      showToast("实例已创建，请到版本页安装游戏");
      onClose();
    } catch (e) {
      showToast(String(e), "bad");
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="fixed inset-0 bg-black/60 flex items-center justify-center z-50" onClick={onClose}>
      <div className="card w-[520px] max-h-[85vh] overflow-auto" onClick={(e) => e.stopPropagation()}>
        <h2 className="text-base font-bold mb-4">新建实例（{step + 1}/3）</h2>

        {step === 0 && (
          <div>
            <label className="text-[12px] text-ccl-muted block mb-1.5">实例名称</label>
            <input className="field" value={name} onChange={(e) => setName(e.target.value)} />
            <label className="text-[12px] text-ccl-muted block mb-1.5 mt-4">最大内存（MB）</label>
            <input
              className="field"
              type="number"
              value={memory}
              min={1024}
              step={512}
              onChange={(e) => setMemory(Number(e.target.value))}
            />
            <p className="text-[11px] text-ccl-muted mt-1.5">
              建议：4GB 物理内存设 2048-3072，8GB 设 4096-6144，16GB 以上设 8192
            </p>
          </div>
        )}

        {step === 1 && (
          <div>
            <label className="text-[12px] text-ccl-muted block mb-1.5">
              Minecraft 版本 {loadingVersions && <Loader2 size={11} className="inline animate-spin" />}
            </label>
            <select className="field mb-3" value={mcVersion} onChange={(e) => setMcVersion(e.target.value)}>
              <optgroup label="正式版">
                {releases.map((v) => (
                  <option key={v.id} value={v.id}>
                    {v.id}
                  </option>
                ))}
              </optgroup>
              <optgroup label="快照版">
                {snapshots.map((v) => (
                  <option key={v.id} value={v.id}>
                    {v.id}
                  </option>
                ))}
              </optgroup>
            </select>
            <label className="text-[12px] text-ccl-muted block mb-1.5">模组加载器</label>
            <div className="grid grid-cols-1 gap-2">
              {LOADERS.map((l) => (
                <button
                  key={l.id}
                  className={`text-left px-3.5 py-2.5 rounded-lg border transition-colors ${
                    loader === l.id
                      ? "border-ccl-accent bg-[#e4f3e4]"
                      : "border-ccl-border hover:border-ccl-accent/50"
                  }`}
                  onClick={() => setLoader(l.id)}
                >
                  <div className="text-[13px] font-semibold">{l.label}</div>
                  <div className="text-[11px] text-ccl-muted">{l.desc}</div>
                </button>
              ))}
            </div>
          </div>
        )}

        {step === 2 && (
          <div>
            {loader === "vanilla" ? (
              <p className="text-[13px] text-ccl-muted">原版实例无需选择加载器版本。</p>
            ) : (
              <>
                <label className="text-[12px] text-ccl-muted block mb-1.5">
                  {LOADERS.find((l) => l.id === loader)?.label} 版本
                </label>
                <select
                  className="field"
                  value={loaderVersion}
                  onChange={(e) => setLoaderVersion(e.target.value)}
                >
                  {loaderVersions.map((v) => (
                    <option key={v} value={v}>
                      {v}
                    </option>
                  ))}
                </select>
                {loaderVersions.length === 0 && !loadingVersions && (
                  <p className="text-[12px] text-ccl-warn mt-2">
                    该 MC 版本暂无 {loader} 版本，请返回上一步换版本或换加载器
                  </p>
                )}
              </>
            )}
            <div className="mt-4 p-3 bg-ccl-bg rounded-lg text-[12px] text-ccl-muted leading-relaxed">
              <div>名称：<b className="text-ccl-text">{name}</b></div>
              <div>版本：<b className="text-ccl-text">{mcVersion}</b></div>
              <div>加载器：<b className="text-ccl-text">{LOADERS.find((l) => l.id === loader)?.label}</b>{loader !== "vanilla" && loaderVersion && ` ${loaderVersion}`}</div>
              <div>内存：<b className="text-ccl-text">{memory} MB</b></div>
            </div>
          </div>
        )}

        <div className="flex justify-between mt-6">
          <button className="btn btn-ghost" disabled={step === 0} onClick={() => setStep(step - 1)}>
            上一步
          </button>
          {step < 2 ? (
            <button className="btn btn-primary" onClick={() => setStep(step + 1)}>
              下一步
            </button>
          ) : (
            <button className="btn btn-primary" disabled={saving} onClick={submit}>
              {saving ? <Loader2 size={13} className="animate-spin" /> : <Plus size={13} />} 创建
            </button>
          )}
        </div>
      </div>
    </div>
  );
}
