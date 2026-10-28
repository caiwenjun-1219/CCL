import { useEffect, useRef, useState } from "react";
import {
  Download,
  ShieldCheck,
  Check,
  Loader2,
  AlertTriangle,
  ChevronRight,
  Rocket,
} from "lucide-react";
import { api } from "../lib/tauri";
import { isMobile } from "../lib/env";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

/**
 * 安卓首次启动向导（参考 FCL 首启流程）。
 *
 * 步骤：欢迎 → 申请存储权限（MANAGE_EXTERNAL_STORAGE）→ 下载 Java 运行时 → 完成。
 * 桌面端不会挂载本组件（由 App.tsx 按 isMobile() 过滤）。
 */
const JAVA_MAJORS = [8, 17, 21, 25];

export default function FirstLaunchSetup({ onDone }: { onDone: () => void }) {
  const [step, setStep] = useState(0);
  const [permGranted, setPermGranted] = useState(false);
  const [permBusy, setPermBusy] = useState(false);
  const [permError, setPermError] = useState<string | null>(null);

  const [javaMajor, setJavaMajor] = useState(17);
  const [javaBusy, setJavaBusy] = useState(false);
  const [javaDone, setJavaDone] = useState(false);
  const [javaProgress, setJavaProgress] = useState(0); // 0~1
  const [javaError, setJavaError] = useState<string | null>(null);

  const unlisteners = useRef<UnlistenFn[]>([]);

  // 桌面端直接跳过；移动端首启检测权限
  useEffect(() => {
    if (!isMobile()) {
      onDone();
      return;
    }
    let alive = true;
    api
      .checkStoragePermission()
      .then((r) => alive && setPermGranted(r.granted))
      .catch(() => alive && setPermGranted(false));
    return () => {
      alive = false;
    };
  }, [onDone]);

  // 下载进度监听（聚合 java-<major>-* 任务）
  useEffect(() => {
    let cancelled = false;
    const tasks = new Map<string, { loaded: number; total: number; finished: boolean }>();

    const setup = async () => {
      const off1 = await listen<{
        id: string;
        loaded: number;
        total: number;
        finished: boolean;
      }>("download-progress", (e) => {
        const id = e.payload.id;
        if (!id.startsWith(`java-${javaMajor}-`)) return;
        tasks.set(id, {
          loaded: e.payload.loaded,
          total: e.payload.total,
          finished: e.payload.finished,
        });
        let loaded = 0;
        let total = 0;
        for (const t of tasks.values()) {
          loaded += t.loaded;
          total += t.total;
        }
        if (total > 0) setJavaProgress(Math.min(1, loaded / total));
      });

      const off2 = await listen("java-installed", () => {
        setJavaProgress(1);
        setJavaDone(true);
        setJavaBusy(false);
      });

      unlisteners.current.push(off1, off2);
    };
    setup();
    return () => {
      cancelled = true;
      unlisteners.current.forEach((u) => u());
      unlisteners.current = [];
    };
  }, [javaMajor]);

  const requestPerm = async () => {
    setPermBusy(true);
    setPermError(null);
    try {
      const r = await api.requestStoragePermission();
      setPermGranted(r.granted);
      if (!r.granted) setPermError("权限未授予：下载的 Java 与游戏将只能存放在应用私有目录，无法在文件管理器直接访问。");
    } catch (e) {
      setPermError(String(e));
    } finally {
      setPermBusy(false);
    }
  };

  const downloadJava = async () => {
    setJavaBusy(true);
    setJavaError(null);
    setJavaProgress(0);
    try {
      await api.downloadJava(javaMajor);
      // 成功兜底（java-installed 事件也会触发）
      setJavaDone(true);
      setJavaProgress(1);
    } catch (e) {
      setJavaError(String(e));
    } finally {
      setJavaBusy(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 bg-ccl-bg flex items-center justify-center p-5">
      <div className="w-full max-w-[440px]">
        <div className="card !p-7">
          {/* 顶部进度指示 */}
          <div className="flex items-center gap-2 mb-6">
            {["欢迎", "存储权限", "下载 Java", "完成"].map((label, i) => (
              <div key={label} className="flex-1 flex flex-col items-center gap-1">
                <div
                  className={`h-1.5 rounded-full w-full ${
                    i <= step ? "bg-ccl-primary" : "bg-ccl-border"
                  }`}
                />
                <span
                  className={`text-[10px] ${
                    i === step ? "text-ccl-primary" : "text-ccl-muted"
                  }`}
                >
                  {label}
                </span>
              </div>
            ))}
          </div>

          {step === 0 && (
            <div>
              <Rocket size={28} className="text-ccl-primary mb-3" />
              <h1 className="text-[18px] font-bold mb-2">欢迎使用 CCL 安卓版</h1>
              <p className="text-[13px] text-ccl-muted leading-relaxed mb-5">
                首次启动需要完成初始化：授予存储权限，并下载运行游戏所需的 Java
                运行时。整个过程参考 FCL，只需一次。
              </p>
              <button
                className="btn btn-primary w-full"
                onClick={() => setStep(1)}
              >
                开始设置 <ChevronRight size={15} />
              </button>
            </div>
          )}

          {step === 1 && (
            <div>
              <ShieldCheck size={28} className="text-ccl-primary mb-3" />
              <h1 className="text-[18px] font-bold mb-2">存储权限</h1>
              <p className="text-[13px] text-ccl-muted leading-relaxed mb-4">
                为了让 Java 运行时与游戏文件能被你在文件管理器里直接管理，CCL
                需要「所有文件访问」权限。我们会跳转到系统设置，请点击「允许」。
              </p>
              {permGranted ? (
                <div className="flex items-center gap-2 text-ccl-success text-[13px] mb-4">
                  <Check size={16} /> 已授予存储权限
                </div>
              ) : (
                <button
                  className="btn btn-primary w-full mb-3"
                  disabled={permBusy}
                  onClick={requestPerm}
                >
                  {permBusy ? (
                    <Loader2 size={14} className="animate-spin" />
                  ) : (
                    <ShieldCheck size={15} />
                  )}
                  授予存储权限
                </button>
              )}
              {permError && (
                <div className="flex items-start gap-2 text-ccl-warn text-[12px] mb-3">
                  <AlertTriangle size={14} className="mt-0.5 shrink-0" />
                  <span>{permError}</span>
                </div>
              )}
              <div className="flex gap-2">
                <button
                  className="btn btn-ghost flex-1"
                  onClick={() => setStep(2)}
                >
                  跳过
                </button>
                <button
                  className="btn btn-primary flex-1"
                  disabled={!permGranted}
                  onClick={() => setStep(2)}
                >
                  下一步
                </button>
              </div>
            </div>
          )}

          {step === 2 && (
            <div>
              <Download size={28} className="text-ccl-primary mb-3" />
              <h1 className="text-[18px] font-bold mb-2">下载 Java 运行时</h1>
              <p className="text-[13px] text-ccl-muted leading-relaxed mb-4">
                选择要下载的 Java 版本（现代版 Minecraft 推荐 Java 17，旧版 Mod
                可能需要 Java 8）。下载完成后即可安装游戏。
              </p>
              <div className="flex flex-wrap gap-2 mb-4">
                {JAVA_MAJORS.map((m) => (
                  <button
                    key={m}
                    className={`btn !py-1.5 !px-4 ${
                      javaMajor === m ? "btn-primary" : "btn-ghost"
                    }`}
                    disabled={javaBusy}
                    onClick={() => setJavaMajor(m)}
                  >
                    Java {m}
                  </button>
                ))}
              </div>

              {javaBusy && (
                <div className="mb-4">
                  <div className="h-2 rounded-full bg-ccl-border overflow-hidden">
                    <div
                      className="h-full bg-ccl-primary transition-all"
                      style={{ width: `${Math.round(javaProgress * 100)}%` }}
                    />
                  </div>
                  <div className="text-[11px] text-ccl-muted mt-1 text-right">
                    {Math.round(javaProgress * 100)}%
                  </div>
                </div>
              )}

              {javaError && (
                <div className="flex items-start gap-2 text-ccl-warn text-[12px] mb-3">
                  <AlertTriangle size={14} className="mt-0.5 shrink-0" />
                  <span>{javaError}</span>
                </div>
              )}

              {javaDone ? (
                <div className="flex items-center gap-2 text-ccl-success text-[13px] mb-4">
                  <Check size={16} /> Java {javaMajor} 已就绪
                </div>
              ) : (
                <button
                  className="btn btn-primary w-full mb-3"
                  disabled={javaBusy}
                  onClick={downloadJava}
                >
                  {javaBusy ? (
                    <Loader2 size={14} className="animate-spin" />
                  ) : (
                    <Download size={15} />
                  )}
                  下载 Java {javaMajor}
                </button>
              )}

              <button
                className="btn btn-ghost w-full"
                disabled={javaBusy || !javaDone}
                onClick={() => setStep(3)}
              >
                下一步
              </button>
            </div>
          )}

          {step === 3 && (
            <div>
              <Check size={28} className="text-ccl-success mb-3" />
              <h1 className="text-[18px] font-bold mb-2">准备就绪</h1>
              <p className="text-[13px] text-ccl-muted leading-relaxed mb-5">
                初始化完成！现在你可以创建游戏实例、安装模组加载器，畅玩 Minecraft
                了。
              </p>
              <button
                className="btn btn-primary w-full"
                onClick={onDone}
              >
                进入启动器 <ChevronRight size={15} />
              </button>
            </div>
          )}
        </div>
        <p className="text-center text-[11px] text-ccl-muted mt-3">
          CCL · 安卓版 · 首次启动设置
        </p>
      </div>
    </div>
  );
}
