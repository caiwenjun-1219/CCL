import { useState } from "react";
import { Package, Upload, Download, Loader2 } from "lucide-react";
import { open } from "@tauri-apps/plugin-dialog";
import { useApp } from "../store/app";
import { api, onModpackProgress } from "../lib/tauri";
import { PackRecommend } from "../components/Recommend";

export default function ModpacksPage() {
  const { instances, showToast, loadInstances } = useApp();
  const [busy, setBusy] = useState(false);
  const [progress, setProgress] = useState<{ phase: string; done: number; total: number } | null>(null);

  const importPack = async () => {
    const selected = await open({
      multiple: false,
      filters: [{ name: "整合包", extensions: ["mrpack", "zip"] }],
    });
    if (!selected) return;
    setBusy(true);
    setProgress({ phase: "解析整合包", done: 0, total: 0 });
    try {
      const inst = await api.importModpack(selected as string, null);
      showToast(`整合包导入完成：${inst.name}`);
      loadInstances();
    } catch (e) {
      showToast(String(e), "bad");
    } finally {
      setBusy(false);
      setProgress(null);
    }
  };

  const exportPack = async (id: string, name: string) => {
    const dest = await open({
      defaultPath: `${name}.mrpack`,
      filters: [{ name: "Modrinth 整合包", extensions: ["mrpack"] }],
    });
    if (!dest) return;
    setBusy(true);
    try {
      await api.exportModpack(id, dest as string);
      showToast("导出完成");
    } catch (e) {
      showToast(String(e), "bad");
    } finally {
      setBusy(false);
    }
  };

  return (
    <div>
      <h1 className="pcl-title mb-4">整合包</h1>

      <PackRecommend onInstalled={loadInstances} />

      <div className="flex items-center gap-2.5 mt-7 mb-3">
        <span className="text-[12px] font-bold text-ccl-muted">本地整合包</span>
        <span className="flex-1 h-px bg-ccl-gray6" />
      </div>

      <div className="card mb-5">
        <div className="flex items-start gap-4">
          <div className="w-12 h-12 rounded-xl bg-ccl-accent/10 border border-ccl-accent/25 flex items-center justify-center shrink-0">
            <Package size={22} className="text-ccl-accent" />
          </div>
          <div className="flex-1">
            <h2 className="text-[14px] font-bold mb-1">导入整合包</h2>
            <p className="text-[12px] text-ccl-muted mb-3 leading-relaxed">
              支持 Modrinth（.mrpack）与 CurseForge（manifest.zip）两种格式。
              导入时会自动创建实例、安装对应 MC 版本与加载器、下载全部模组并还原配置。
              <br />
              CurseForge 整合包需要在设置页配置 API Key 才能自动下载模组。
            </p>
            <button className="btn btn-primary" onClick={importPack} disabled={busy}>
              {busy ? <Loader2 size={13} className="animate-spin" /> : <Upload size={13} />}
              选择整合包文件
            </button>
            {progress && (
              <div className="mt-3">
                <div className="flex justify-between text-[11px] text-ccl-muted mb-1">
                  <span>{progress.phase}</span>
                  {progress.total > 0 && (
                    <span>
                      {progress.done} / {progress.total}
                    </span>
                  )}
                </div>
                <div className="h-1.5 bg-[#e6eaef] rounded overflow-hidden">
                  <div
                    className="h-full bg-ccl-accent transition-all"
                    style={{
                      width: progress.total > 0 ? `${(progress.done / progress.total) * 100}%` : "30%",
                    }}
                  />
                </div>
              </div>
            )}
          </div>
        </div>
      </div>

      <div className="card">
        <h2 className="text-[14px] font-bold mb-3">导出现有实例为整合包</h2>
        <p className="text-[12px] text-ccl-muted mb-4">
          导出为 Modrinth .mrpack 格式，包含全部模组、配置文件与选项文件，可在任意支持 .mrpack 的启动器导入。
        </p>
        {instances.length === 0 ? (
          <p className="text-[12px] text-ccl-muted">暂无实例</p>
        ) : (
          <div className="grid grid-cols-1 md:grid-cols-2 gap-2">
            {instances.map((inst) => (
              <div
                key={inst.id}
                className="flex items-center gap-3 bg-ccl-bg border border-ccl-border rounded-lg px-3.5 py-2.5"
              >
                <Package size={16} className="text-ccl-muted shrink-0" />
                <div className="min-w-0 flex-1">
                  <div className="text-[13px] font-semibold truncate">{inst.name}</div>
                  <div className="text-[11px] text-ccl-muted">
                    {inst.minecraft_version} · {inst.loader}
                  </div>
                </div>
                <button
                  className="btn btn-ghost !py-1 !px-2.5 shrink-0"
                  disabled={busy}
                  onClick={() => exportPack(inst.id, inst.name)}
                >
                  <Download size={12} /> 导出
                </button>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
