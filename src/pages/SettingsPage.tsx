import { useEffect, useState } from "react";
import { Save, RefreshCw, ArrowUpCircle, Cloud, Loader2, KeyRound } from "lucide-react";
import { open } from "@tauri-apps/plugin-dialog";
import { useApp } from "../store/app";
import { api } from "../lib/tauri";

export default function SettingsPage() {
  const { settings, patchSettings, update, checkUpdate, showToast } = useApp();
  const [draft, setDraft] = useState(settings);
  const [saving, setSaving] = useState(false);
  const [checking, setChecking] = useState(false);
  const [installing, setInstalling] = useState(false);
  const [dlPct, setDlPct] = useState(0);

  useEffect(() => setDraft(settings), [settings]);

  const save = async () => {
    setSaving(true);
    try {
      await patchSettings(draft);
      showToast("设置已保存");
    } catch (e) {
      showToast(String(e), "bad");
    } finally {
      setSaving(false);
    }
  };

  const check = async () => {
    setChecking(true);
    try {
      await checkUpdate();
      const u = useApp.getState().update;
      if (u?.has_update) showToast(`发现新版本 v${u.latest_version}`);
      else if (u) showToast(`当前已是最新版本 v${u.current_version}`);
    } finally {
      setChecking(false);
    }
  };

  const install = async () => {
    if (!update) return;
    setInstalling(true);
    setDlPct(0);
    try {
      const { listen } = await import("@tauri-apps/api/event");
      const un = await listen<{ loaded: number; total: number }>("update-download", (e) => {
        if (e.payload.total > 0) setDlPct(Math.floor((e.payload.loaded / e.payload.total) * 100));
      });
      await api.installCloudUpdate(update.download_url, update.sha256);
      un();
    } catch (e) {
      showToast(String(e), "bad");
      setInstalling(false);
    }
  };

  const pickJava = async () => {
    const selected = await open({
      multiple: false,
      filters: [{ name: "Java 可执行文件", extensions: ["exe"] }],
    });
    if (selected) setDraft({ ...draft, java_path: selected as string });
  };

  return (
    <div className="max-w-2xl">
      <h1 className="hm-page-title">设置</h1>

      {/* 云控更新 */}
      <div className="card mb-4">
        <div className="flex items-center gap-2 mb-3">
          <Cloud size={15} className="text-ccl-muted" />
          <h2 className="text-[13px] font-bold">云控与更新</h2>
        </div>
        <div className="text-[12px] space-y-1.5 mb-3">
          <div className="flex justify-between">
            <span className="text-ccl-muted">当前版本</span>
            <b>v{update?.current_version || "…"}</b>
          </div>
          <div className="flex justify-between">
            <span className="text-ccl-muted">最新版本</span>
            <b className={update?.has_update ? "text-ccl-accent2" : ""}>
              v{update?.latest_version || "…"}
            </b>
          </div>
          <div className="flex justify-between">
            <span className="text-ccl-muted">云控服务</span>
            <span className={update?.cloud_online ? "text-ccl-accent2" : "text-ccl-muted"}>
              {update?.cloud_online ? "在线（cclconfig.caiwenjun.top）" : "离线（不影响使用）"}
            </span>
          </div>
        </div>
        <div className="flex flex-wrap gap-2 items-center">
          <button className="btn btn-ghost" onClick={check} disabled={checking}>
            {checking ? <Loader2 size={12} className="animate-spin" /> : <RefreshCw size={12} />} 检查更新
          </button>
          {update?.has_update && (
            <button className="btn btn-primary" onClick={install} disabled={installing}>
              {installing ? (
                <>
                  <Loader2 size={12} className="animate-spin" /> {dlPct}%
                </>
              ) : (
                <>
                  <ArrowUpCircle size={12} /> 更新到 v{update.latest_version}
                </>
              )}
            </button>
          )}
          <label className="flex items-center gap-1.5 text-[12px] text-ccl-muted ml-auto">
            <input
              type="checkbox"
              checked={draft.auto_check_update}
              onChange={(e) => setDraft({ ...draft, auto_check_update: e.target.checked })}
            />
            启动时自动检查
          </label>
        </div>
      </div>

      {/* 下载 */}
      <div className="card mb-4">
        <h2 className="text-[13px] font-bold mb-3">下载</h2>
        <label className="text-[12px] text-ccl-muted block mb-1.5">资源下载源</label>
        <select
          className="field mb-3"
          value={draft.download_source}
          onChange={(e) => setDraft({ ...draft, download_source: e.target.value })}
        >
          <option value="bmclapi">BMCLAPI 国内镜像（推荐，自动回退官方源）</option>
          <option value="mojang">Mojang 官方源（自动回退国内镜像）</option>
        </select>
        <p className="text-[11px] text-ccl-muted leading-relaxed">
          无论选哪个，单个文件下载失败时都会自动按「首选 → 备用镜像 → 官方源」的顺序一级一级回退。
        </p>
      </div>

      {/* 游戏 */}
      <div className="card mb-4">
        <h2 className="text-[13px] font-bold mb-3">游戏</h2>
        <label className="text-[12px] text-ccl-muted block mb-1.5">默认最大内存（MB）</label>
        <input
          className="field mb-3"
          type="number"
          min={1024}
          step={512}
          value={draft.max_memory}
          onChange={(e) => setDraft({ ...draft, max_memory: Number(e.target.value) })}
        />
        <label className="text-[12px] text-ccl-muted block mb-1.5">额外 JVM 参数</label>
        <input
          className="field mb-3"
          placeholder="-XX:+UseG1GC -Dfml.queryResult=confirm"
          value={draft.jvm_args}
          onChange={(e) => setDraft({ ...draft, jvm_args: e.target.value })}
        />
        <label className="text-[12px] text-ccl-muted block mb-1.5">Java 路径（留空 = 自动选择）</label>
        <div className="flex gap-2">
          <input
            className="field"
            placeholder="自动"
            value={draft.java_path}
            onChange={(e) => setDraft({ ...draft, java_path: e.target.value })}
          />
          <button className="btn btn-ghost" onClick={pickJava}>
            浏览
          </button>
        </div>
      </div>

      {/* CurseForge */}
      <div className="card mb-4">
        <div className="flex items-center gap-2 mb-3">
          <KeyRound size={15} className="text-ccl-muted" />
          <h2 className="text-[13px] font-bold">CurseForge API Key</h2>
        </div>
        <input
          className="field"
          placeholder="在 console.curseforge.com 申请"
          value={draft.curseforge_api_key}
          onChange={(e) => setDraft({ ...draft, curseforge_api_key: e.target.value })}
        />
        <p className="text-[11px] text-ccl-muted mt-2 leading-relaxed">
          用于 CurseForge 模组搜索与整合包导入。Modrinth 无需密钥。
        </p>
      </div>

      {/* 行为 */}
      <div className="card mb-4">
        <h2 className="text-[13px] font-bold mb-3">窗口行为</h2>
        <select
          className="field"
          value={draft.close_action}
          onChange={(e) => setDraft({ ...draft, close_action: e.target.value })}
        >
          <option value="minimize">关闭窗口时最小化到托盘</option>
          <option value="exit">关闭窗口时退出程序</option>
        </select>
      </div>

      <div className="flex justify-end">
        <button className="btn btn-primary" onClick={save} disabled={saving}>
          {saving ? <Loader2 size={13} className="animate-spin" /> : <Save size={13} />} 保存设置
        </button>
      </div>
    </div>
  );
}
