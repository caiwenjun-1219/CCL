import { useEffect, useRef, useState } from "react";
import { useNavigate } from "react-router-dom";
import { Package, Loader2 } from "lucide-react";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { useApp } from "../store/app";
import { api, onModpackProgress } from "../lib/tauri";

/** 可识别的整合包扩展名（.mrpack 与 CurseForge .zip） */
const PACK_EXT = [".mrpack", ".zip"];

const isPackFile = (p: string) => PACK_EXT.some((e) => p.toLowerCase().endsWith(e));

const fileName = (p: string) => p.replace(/\\/g, "/").split("/").pop() || p;

/**
 * 全局拖放层：把 .mrpack / .zip 拖到启动器窗口上，松开即自动安装整合包。
 * 桌面端（Windows/macOS/Linux）有效；非 Tauri 环境（浏览器调试）自动静默降级。
 */
export default function ModpackDrop() {
  const navigate = useNavigate();
  const { showToast, loadInstances } = useApp();
  const [hover, setHover] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [progress, setProgress] = useState<{ phase: string; done: number; total: number } | null>(null);
  // 拖入的文件可能同时包含非整合包文件，用 ref 持有待安装目标，避免闭口过期
  const pending = useRef<string | null>(null);
  const unProg = useRef<(() => void) | undefined>(undefined);

  useEffect(() => {
    let un: (() => void) | undefined;
    let unProg: (() => void) | undefined;
    (async () => {
      try {
        const webview = getCurrentWebview();
        un = await webview.onDragDropEvent((event) => {
          const payload = event.payload as { type: string; paths: string[] };
          const pack = (payload.paths || []).find(isPackFile) || null;
          if (payload.type === "enter" || payload.type === "over") {
            if (pack) {
              pending.current = pack;
              setHover(pack);
            }
          } else if (payload.type === "leave") {
            pending.current = null;
            setHover(null);
          } else if (payload.type === "drop") {
            setHover(null);
            const target = pack || pending.current;
            pending.current = null;
            if (target && !busy) void install(target);
          }
        });
      } catch {
        /* 非 Tauri 环境（vite preview 纯前端调试）：无拖放支持，静默忽略 */
      }
    })();
    return () => un?.();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const install = async (path: string) => {
    setBusy(true);
    setProgress({ phase: "解析整合包", done: 0, total: 0 });
    unProg.current = await onModpackProgress((p) => setProgress(p));
    try {
      const inst = await api.importModpack(path, null);
      showToast(`整合包导入完成：${inst.name}`);
      loadInstances();
      navigate("/");
    } catch (e) {
      showToast(String(e), "bad");
    } finally {
      unProg.current?.();
      unProg.current = undefined;
      setBusy(false);
      setProgress(null);
    }
  };

  if (!hover && !busy) return null;

  return (
    <div className="fixed inset-0 z-[100] flex items-center justify-center bg-[#1370f3]/25 backdrop-blur-[2px]">
      {busy ? (
        <div className="w-[420px] bg-white rounded-[6px] border border-ccl-gray6 shadow-[0_18px_50px_rgba(0,0,0,.28)] overflow-hidden">
          <div className="h-[38px] px-3.5 flex items-center gap-2 border-b border-ccl-gray6 text-[13px] font-bold">
            <Loader2 size={14} className="animate-spin text-ccl-accent" />
            <span>正在安装整合包</span>
          </div>
          <div className="p-4">
            <div className="text-[12.5px] text-ccl-gray2 truncate mb-3" title={hover || ""}>
              {fileName(hover || "")}
            </div>
            <div className="h-[6px] bg-ccl-gray7 rounded-[3px] overflow-hidden">
              <div
                className="h-full rounded-[3px] transition-all"
                style={{
                  width:
                    progress && progress.total > 0
                      ? `${Math.floor((progress.done / progress.total) * 100)}%`
                      : "8%",
                  background: "linear-gradient(90deg,#4890f5,#1370f3)",
                }}
              />
            </div>
            <div className="flex justify-between mt-2 text-[11.5px] text-ccl-gray3">
              <span>{progress?.phase || "准备中"}</span>
              <span>
                {progress && progress.total > 0
                  ? `${progress.done} / ${progress.total}`
                  : "下载本体与模组文件…"}
              </span>
            </div>
          </div>
        </div>
      ) : (
        <div className="w-[380px] h-[240px] bg-white/95 rounded-[6px] border-2 border-dashed border-ccl-accent shadow-[0_18px_50px_rgba(0,0,0,.28)] flex flex-col items-center justify-center gap-3 pointer-events-none">
          <div className="w-[52px] h-[52px] rounded-full bg-ccl-accent/10 flex items-center justify-center">
            <Package size={26} className="text-ccl-accent" />
          </div>
          <div className="text-[14px] font-bold text-ccl-text">松开鼠标，自动安装整合包</div>
          <div className="text-[11.5px] text-ccl-gray3 px-6 text-center truncate max-w-full" title={hover || ""}>
            {fileName(hover || "")}
          </div>
          <div className="text-[10.5px] text-ccl-gray3">支持 Modrinth .mrpack 与 CurseForge .zip</div>
        </div>
      )}
    </div>
  );
}
