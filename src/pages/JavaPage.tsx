import { useEffect, useState } from "react";
import { Coffee, Download, RefreshCw, Loader2, Check, Cpu, Smartphone } from "lucide-react";
import { useApp } from "../store/app";
import { api, type JavaInfo, type JavaEnv } from "../lib/tauri";

/** 桌面端（Adoptium）可下载的 Java 主版本 */
const DESKTOP_VERSIONS = [8, 11, 17, 21, 25];
/** Android 端（EclipseJRE）提供的 Java 主版本 */
const ANDROID_VERSIONS = [8, 17, 21, 25];

const MC_JAVA_HINT: Record<number, string> = {
  8: "MC 1.16 及以下",
  11: "MC 1.17 可用",
  17: "MC 1.18 ~ 1.20.4",
  21: "MC 1.20.5+ / 1.21.x",
  25: "最新版 MC",
};

export default function JavaPage() {
  const { showToast } = useApp();
  const [javas, setJavas] = useState<JavaInfo[]>([]);
  const [env, setEnv] = useState<JavaEnv | null>(null);
  const [downloading, setDownloading] = useState<number | null>(null);

  const refresh = async () => {
    try {
      const [list, e] = await Promise.all([api.scanJavas(), api.javaEnv()]);
      setJavas(list);
      setEnv(e);
    } catch (e) {
      showToast(String(e), "bad");
    }
  };

  useEffect(() => {
    refresh();
  }, []);

  const download = async (major: number) => {
    setDownloading(major);
    try {
      await api.downloadJava(major);
      showToast(`Java ${major} 下载完成`);
      refresh();
    } catch (e) {
      showToast(String(e), "bad");
    } finally {
      setDownloading(null);
    }
  };

  const isAndroid = env?.is_android_jre ?? false;
  const versions = env?.supported_majors?.length
    ? env.supported_majors
    : isAndroid
      ? ANDROID_VERSIONS
      : DESKTOP_VERSIONS;

  return (
    <div>
      <div className="flex items-center justify-between mb-5">
        <h1 className="pcl-title">Java 管理</h1>
        <button className="btn btn-ghost" onClick={refresh}>
          <RefreshCw size={13} /> 重新扫描
        </button>
      </div>

      <div className="card mb-5">
        <div className="flex items-center gap-2 mb-3">
          <Cpu size={15} className="text-ccl-muted" />
          <h2 className="text-[13px] font-bold">
            {isAndroid ? "已安装的 Android JRE" : "本机已检测到"} {javas.length} 个 Java 运行时
          </h2>
        </div>
        {javas.length === 0 ? (
          <p className="text-[12px] text-ccl-muted">
            {isAndroid
              ? "未安装任何 Java 运行时。Android 端需要交叉编译的 JRE，请在下方下载。"
              : "未检测到任何 Java。启动游戏需要 Java，请在下方下载。"}
          </p>
        ) : (
          <div className="grid grid-cols-1 md:grid-cols-2 gap-2">
            {javas.map((j) => (
              <div
                key={j.path}
                className="bg-ccl-bg border border-ccl-border rounded-lg px-3.5 py-2.5"
              >
                <div className="flex items-center gap-2">
                  <span className="tag tag-green">Java {j.major}</span>
                  <span className="text-[12px] font-semibold truncate">{j.version || j.vendor}</span>
                  {j.managed && <span className="tag tag-gray">托管</span>}
                </div>
                <div className="text-[11px] text-ccl-muted mt-1 truncate" title={j.path}>
                  {j.vendor} · {j.path}
                </div>
              </div>
            ))}
          </div>
        )}
      </div>

      <div className="card">
        <div className="flex items-center gap-2 mb-3">
          {isAndroid ? (
            <Smartphone size={15} className="text-ccl-muted" />
          ) : (
            <Coffee size={15} className="text-ccl-muted" />
          )}
          <h2 className="text-[13px] font-bold">
            下载 Java{env?.source ? `（${env.source}）` : ""}
          </h2>
        </div>
        {isAndroid ? (
          <p className="text-[12px] text-ccl-muted mb-4">
            Android 端使用<strong>交叉编译到 Android（bionic）</strong>的 OpenJDK，
            产物为 Pojav / FCL 同款 <code>tar.xz</code> 格式（通用包 + 架构包叠加解压），
            每个文件都按官方清单校验 sha256。桌面版 Adoptium 的 Windows JDK 无法在 Android 上运行，故完全隔离。
          </p>
        ) : (
          <p className="text-[12px] text-ccl-muted mb-4">
            通过 TUNA 镜像（清华大学开源软件镜像站）自动下载 Adoptium 最新补丁版，
            安装到启动器数据目录；镜像不可用时自动回退官方源。
            启动游戏时会按 MC 版本自动选择合适的主版本。
          </p>
        )}
        <div className="grid grid-cols-2 md:grid-cols-5 gap-2">
          {versions.map((v) => {
            const has = javas.some((j) => j.major === v);
            return (
              <div
                key={v}
                className="bg-ccl-bg border border-ccl-border rounded-lg px-3 py-3 text-center"
              >
                <div className="text-[15px] font-bold mb-0.5">Java {v}</div>
                <div className="text-[10px] text-ccl-muted mb-2.5 h-6">{MC_JAVA_HINT[v]}</div>
                {has ? (
                  <span className="text-[11px] text-ccl-accent2 flex items-center justify-center gap-1">
                    <Check size={11} /> 已安装
                  </span>
                ) : (
                  <button
                    className="btn btn-ghost !py-1 !px-3 w-full"
                    disabled={downloading === v}
                    onClick={() => download(v)}
                  >
                    {downloading === v ? (
                      <Loader2 size={11} className="animate-spin" />
                    ) : (
                      <Download size={11} />
                    )}
                    下载
                  </button>
                )}
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}
