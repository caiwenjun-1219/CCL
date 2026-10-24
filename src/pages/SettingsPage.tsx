import { useEffect, useState } from "react";
import { useSearchParams, useNavigate } from "react-router-dom";
import { Save, RefreshCw, ArrowUpCircle, Loader2, KeyRound, FolderOpen } from "lucide-react";
import { open } from "@tauri-apps/plugin-dialog";
import { useApp } from "../store/app";
import { api } from "../lib/tauri";

/** PCL 设置项行：左标题+说明，右控件，底部极淡分隔线 */
function Row({
  label,
  desc,
  children,
}: {
  label: string;
  desc?: string;
  children: React.ReactNode;
}) {
  return (
    <div className="flex items-center gap-5 py-2.5 border-b border-ccl-gray6 last:border-0">
      <div className="flex-1 min-w-0">
        <div className="text-[13px] text-ccl-gray1">{label}</div>
        {desc && <div className="text-[11.5px] text-ccl-gray3 mt-0.5 leading-relaxed">{desc}</div>}
      </div>
      <div className="shrink-0 flex items-center gap-2">{children}</div>
    </div>
  );
}

function Switch({ on, onChange }: { on: boolean; onChange: (v: boolean) => void }) {
  return <div className="pcl-switch" data-on={on} onClick={() => onChange(!on)} role="switch" aria-checked={on} />;
}

export default function SettingsPage() {
  const [params] = useSearchParams();
  const tab = params.get("tab") || "launch";
  const navigate = useNavigate();
  const { settings, patchSettings, update, checkUpdate, showToast } = useApp();
  const [draft, setDraft] = useState(settings);
  const [checking, setChecking] = useState(false);
  const [installing, setInstalling] = useState(false);
  const [dlPct, setDlPct] = useState(0);

  useEffect(() => setDraft(settings), [settings]);

  const commit = (patch: Partial<typeof settings>) => patchSettings(patch);

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
    if (selected) {
      setDraft({ ...draft, java_path: selected as string });
      commit({ java_path: selected as string });
    }
  };

  const HEAD: Record<string, { title: string; sub: string }> = {
    launch: { title: "启动设置", sub: "内存、JVM 参数与 Java 运行环境" },
    ui: { title: "个性化", sub: "界面与窗口行为" },
    system: { title: "系统", sub: "下载源与第三方服务凭据" },
    other: { title: "其他", sub: "更新与关于信息" },
  };

  return (
    <div className="max-w-[760px]">
      <h1 className="pcl-title">{HEAD[tab]?.title || "设置"}</h1>
      <p className="pcl-subtitle">{HEAD[tab]?.sub}</p>

      {/* -------------------------------------------------- 启动 */}
      {tab === "launch" && (
        <div className="mt-5">
          <Row label="默认最大内存" desc="新建实例时使用，单位 MB。建议 2048 ~ 8192。">
            <input
              className="field !w-[120px] text-right"
              type="number"
              min={1024}
              step={512}
              value={draft.max_memory}
              onChange={(e) => setDraft({ ...draft, max_memory: Number(e.target.value) })}
              onBlur={() => commit({ max_memory: draft.max_memory })}
            />
            <span className="text-[12px] text-ccl-gray3">MB</span>
          </Row>
          <Row label="额外 JVM 参数" desc="追加到启动命令中，留空则使用默认 GC 策略。">
            <input
              className="field !w-[320px]"
              placeholder="-XX:+UseG1GC"
              value={draft.jvm_args}
              onChange={(e) => setDraft({ ...draft, jvm_args: e.target.value })}
              onBlur={() => commit({ jvm_args: draft.jvm_args })}
            />
          </Row>
          <Row label="Java 路径" desc="留空则按 MC 版本自动匹配本机 JDK。">
            <input
              className="field !w-[300px]"
              placeholder="自动选择"
              value={draft.java_path}
              onChange={(e) => setDraft({ ...draft, java_path: e.target.value })}
              onBlur={() => commit({ java_path: draft.java_path })}
            />
            <button className="btn !h-[30px]" onClick={pickJava}>
              <FolderOpen size={12} /> 浏览
            </button>
          </Row>
          <div className="mt-4 text-[11.5px] text-ccl-gray4 flex items-center gap-1.5">
            <Save size={12} />
            设置修改后即时生效并自动保存。
          </div>
        </div>
      )}

      {/* -------------------------------------------------- 个性化 */}
      {tab === "ui" && (
        <div className="mt-5">
          <Row label="界面主题" desc="当前跟随 PCL 经典蓝（#1370F3）。自定义主题色将在后续版本开放。">
            <div className="flex items-center gap-2">
              <span className="w-[18px] h-[18px] rounded-[3px]" style={{ background: "#1370f3" }} />
              <span className="text-[12.5px] text-ccl-gray2">PCL 蓝</span>
            </div>
          </Row>
          <Row label="关闭窗口时的行为" desc="最小化到托盘可让下载任务在后台继续。">
            <select
              className="field !w-[220px]"
              value={draft.close_action}
              onChange={(e) => commit({ close_action: e.target.value })}
            >
              <option value="minimize">最小化到托盘</option>
              <option value="exit">退出程序</option>
            </select>
          </Row>
          <Row label="启动时自动检查更新" desc="每 30 分钟也会静默检查一次云控配置。">
            <Switch on={draft.auto_check_update} onChange={(v) => commit({ auto_check_update: v })} />
          </Row>
        </div>
      )}

      {/* -------------------------------------------------- 系统 */}
      {tab === "system" && (
        <div className="mt-5">
          <Row label="资源下载源" desc="任何源失败都会自动逐级回退：首选 → 备用镜像 → 官方源。">
            <select
              className="field !w-[300px]"
              value={draft.download_source}
              onChange={(e) => commit({ download_source: e.target.value })}
            >
              <option value="bmclapi">BMCLAPI 国内镜像（推荐）</option>
              <option value="mojang">Mojang 官方源</option>
            </select>
          </Row>
          <Row label="CurseForge API Key" desc="用于 CurseForge 搜索与整合包导入；Modrinth 无需密钥。">
            <div className="flex items-center gap-2">
              <KeyRound size={13} className="text-ccl-gray4" />
              <input
                className="field !w-[300px]"
                placeholder="在 console.curseforge.com 申请"
                value={draft.curseforge_api_key}
                onChange={(e) => setDraft({ ...draft, curseforge_api_key: e.target.value })}
                onBlur={() => commit({ curseforge_api_key: draft.curseforge_api_key })}
              />
            </div>
          </Row>
        </div>
      )}

      {/* -------------------------------------------------- 其他 */}
      {tab === "other" && (
        <div className="mt-5">
          <div className="pcl-section">云控与更新</div>
          <Row label="当前版本">
            <span className="text-[13px] font-semibold">v{update?.current_version || "…"}</span>
          </Row>
          <Row label="最新版本">
            <span className={`text-[13px] font-semibold ${update?.has_update ? "text-ccl-accent" : ""}`}>
              v{update?.latest_version || "…"}
            </span>
          </Row>
          <Row label="云控服务" desc="cclconfig.caiwenjun.top">
            <span className={`text-[12.5px] ${update?.cloud_online ? "text-[#2e7d46]" : "text-ccl-gray3"}`}>
              {update?.cloud_online ? "在线" : "离线（不影响使用）"}
            </span>
          </Row>
          <div className="flex gap-2 mt-5">
            <button className="btn" onClick={check} disabled={checking}>
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
            <button className="btn ml-auto" onClick={() => navigate("/more/about")}>
              关于 CCL
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
