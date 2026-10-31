import { useEffect, useState } from "react";
import {
  Coffee,
  Download,
  RefreshCw,
  Loader2,
  Check,
  Cpu,
  Smartphone,
  ShieldCheck,
  AlertTriangle,
  Stethoscope,
  X,
  FlaskConical,
  Boxes,
} from "lucide-react";
import { useApp } from "../store/app";
import {
  api,
  type JavaInfo,
  type JavaEnv,
  type SmokeReport,
  type EmbeddedReport,
  type LwjglReport,
} from "../lib/tauri";
import { isMobile } from "../lib/env";

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
  const [permGranted, setPermGranted] = useState(true);
  const [permBusy, setPermBusy] = useState(false);
  const [smoke, setSmoke] = useState<SmokeReport | null>(null);
  const [smokeBusy, setSmokeBusy] = useState(false);
  const [smokeErr, setSmokeErr] = useState<string | null>(null);
  const [emb, setEmb] = useState<EmbeddedReport | null>(null);
  const [embBusy, setEmbBusy] = useState(false);
  const [embErr, setEmbErr] = useState<string | null>(null);
  const [lw, setLw] = useState<LwjglReport | null>(null);
  const [lwBusy, setLwBusy] = useState(false);
  const [lwErr, setLwErr] = useState<string | null>(null);

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
    // 安卓端检测存储权限（MANAGE_EXTERNAL_STORAGE）
    if (isMobile()) {
      api
        .checkStoragePermission()
        .then((r) => setPermGranted(r.granted))
        .catch(() => setPermGranted(true));
    }
  }, []);

  const grantPerm = async () => {
    setPermBusy(true);
    try {
      const r = await api.requestStoragePermission();
      setPermGranted(r.granted);
      if (!r.granted) showToast("未授予存储权限：Java 与游戏将只能存放在应用私有目录", "bad");
    } catch (e) {
      showToast(String(e), "bad");
    } finally {
      setPermBusy(false);
    }
  };

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

  const runSmoke = async () => {
    setSmokeBusy(true);
    setSmokeErr(null);
    try {
      setSmoke(await api.javaSmokeTest());
    } catch (e) {
      setSmoke(null);
      setSmokeErr(String(e));
    } finally {
      setSmokeBusy(false);
    }
  };

  const runEmbedded = async () => {
    setEmbBusy(true);
    setEmbErr(null);
    try {
      setEmb(await api.javaEmbeddedTest());
    } catch (e) {
      setEmb(null);
      setEmbErr(String(e));
    } finally {
      setEmbBusy(false);
    }
  };

  const runLwjgl = async () => {
    setLwBusy(true);
    setLwErr(null);
    try {
      setLw(await api.javaLwjglProbe());
    } catch (e) {
      setLw(null);
      setLwErr(String(e));
    } finally {
      setLwBusy(false);
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

      {isMobile() && !permGranted && (
        <div className="card mb-5 border-ccl-warn/40 bg-ccl-warn/5">
          <div className="flex items-start gap-3">
            <AlertTriangle size={18} className="text-ccl-warn mt-0.5 shrink-0" />
            <div className="flex-1">
              <div className="text-[13px] font-bold mb-1">需要存储权限</div>
              <p className="text-[12px] text-ccl-muted leading-relaxed mb-3">
                Android 11+ 需要「所有文件访问」权限，才能把 Java 与游戏下载到共享存储，
                方便你在文件管理器里直接管理。未授予时也能使用，但文件仅存于应用私有目录。
              </p>
              <button
                className="btn btn-primary !py-1.5 !px-4"
                disabled={permBusy}
                onClick={grantPerm}
              >
                {permBusy ? (
                  <Loader2 size={13} className="animate-spin" />
                ) : (
                  <ShieldCheck size={14} />
                )}
                授予存储权限
              </button>
            </div>
          </div>
        </div>
      )}

      <div className="card mb-5">
        <div className="flex items-center justify-between mb-3">
          <div className="flex items-center gap-2">
            <Stethoscope size={15} className="text-ccl-muted" />
            <h2 className="text-[13px] font-bold">运行自检</h2>
          </div>
          <button className="btn !py-1 !px-3 !text-[12px]" disabled={smokeBusy} onClick={runSmoke}>
            {smokeBusy ? <Loader2 size={12} className="animate-spin" /> : <Stethoscope size={12} />}
            {smoke ? "重新检测" : "开始检测"}
          </button>
        </div>
        <p className="text-[12px] text-ccl-muted leading-relaxed mb-3">
          实际执行一次 <code>java -version</code>，并尝试直接加载 <code>libjvm.so</code>。
          安卓上这两条路能走通哪条，决定了启动游戏该用子进程还是内嵌 JVM。
        </p>

        {smokeErr && (
          <div className="flex items-start gap-2 text-[12px] text-ccl-danger">
            <X size={13} className="mt-0.5 shrink-0" /> {smokeErr}
          </div>
        )}

        {smoke && (
          <div className="space-y-2">
            <div
              className={`text-[12px] leading-relaxed rounded-md px-3 py-2 border ${
                smoke.exec_ok
                  ? "border-ccl-accent2/40 bg-ccl-accent2/5 text-ccl-accent2"
                  : "border-ccl-warn/40 bg-ccl-warn/5 text-ccl-warn"
              }`}
            >
              {smoke.verdict}
            </div>
            <div className="text-[11.5px] bg-ccl-bg border border-ccl-border rounded-md px-3 py-2 space-y-1">
              <Line label="设备 ABI" value={smoke.device_arch} />
              <Line label="JRE 目录" value={smoke.java_dir} mono />
              <Line
                label="bin/java"
                value={`${smoke.bin_exists ? "存在" : "缺失"} · 可执行位 ${smoke.bin_exec_bit ? "有" : "无"}`}
              />
              <Line
                label="exec 结果"
                value={
                  smoke.exec_ok
                    ? `成功，退出码 ${smoke.exec_code}`
                    : `失败 · errno ${smoke.exec_errno ?? "?"} · ${smoke.exec_error ?? ""}`
                }
              />
              <Line
                label="libjvm.so"
                value={
                  smoke.libjvm_exists
                    ? smoke.dlopen_ok
                      ? "dlopen 成功"
                      : `dlopen 失败：${smoke.dlopen_error}`
                    : "不存在"
                }
                mono
              />
            </div>
            {smoke.exec_output && (
              <pre className="text-[11px] leading-relaxed bg-ccl-bg border border-ccl-border rounded-md px-3 py-2 overflow-x-auto whitespace-pre-wrap text-ccl-muted">
                {smoke.exec_output.trim()}
              </pre>
            )}
          </div>
        )}
      </div>

      {isAndroid && (
        <div className="card mb-5">
          <div className="flex items-center justify-between mb-3">
            <div className="flex items-center gap-2">
              <FlaskConical size={15} className="text-ccl-muted" />
              <h2 className="text-[13px] font-bold">内嵌 JVM 实验</h2>
            </div>
            <button className="btn !py-1 !px-3 !text-[12px]" disabled={embBusy} onClick={runEmbedded}>
              {embBusy ? <Loader2 size={12} className="animate-spin" /> : <FlaskConical size={12} />}
              {emb ? "再测一次" : "开始实验"}
            </button>
          </div>
          <p className="text-[12px] text-ccl-muted leading-relaxed mb-3">
            把 <code>libjvm.so</code> 直接映射进 App 进程并调 <code>JNI_CreateJavaVM</code>，然后从 VM
            里读出 <code>java.version</code>。安卓要出画面只能走这条路——独立进程拿不到
            SurfaceView 的窗口句柄，图形层无从建立。
          </p>

          {embErr && (
            <div className="flex items-start gap-2 text-[12px] text-ccl-danger">
              <X size={13} className="mt-0.5 shrink-0" /> {embErr}
            </div>
          )}

          {emb && (
            <div className="space-y-2">
              <div
                className={`text-[12px] leading-relaxed rounded-md px-3 py-2 border ${
                  emb.vm_ok
                    ? "border-ccl-accent2/40 bg-ccl-accent2/5 text-ccl-accent2"
                    : "border-ccl-warn/40 bg-ccl-warn/5 text-ccl-warn"
                }`}
              >
                {emb.verdict}
              </div>
              <div className="text-[11.5px] bg-ccl-bg border border-ccl-border rounded-md px-3 py-2 space-y-1">
                <Line label="JRE 目录" value={emb.java_dir} mono />
                <Line
                  label="libjvm.so"
                  value={
                    emb.libjvm_exists
                      ? emb.dlopen_ok
                        ? "dlopen 成功"
                        : `dlopen 失败：${emb.dlopen_error}`
                      : "不存在"
                  }
                  mono
                />
                <Line
                  label="CreateJavaVM"
                  value={`${emb.create_rc_name}（${emb.create_rc}）· ${emb.elapsed_ms} ms`}
                />
                {emb.vm_ok && (
                  <>
                    <Line label="java.version" value={emb.java_version} mono />
                    <Line label="java.vm" value={`${emb.java_vm_name} ${emb.java_vm_version}`} />
                    <Line label="os.arch" value={emb.os_arch} />
                    <Line label="java.home" value={emb.java_home} mono />
                  </>
                )}
                {emb.cached && <Line label="说明" value="VM 每进程只能建一次，这是上次的结果" />}
              </div>
            </div>
          )}
        </div>
      )}

      {isAndroid && (
        <div className="card mb-5">
          <div className="flex items-center justify-between mb-3">
            <div className="flex items-center gap-2">
              <Boxes size={15} className="text-ccl-muted" />
              <h2 className="text-[13px] font-bold">LWJGL 移植层（出画面的前提）</h2>
            </div>
            <button className="btn !py-1 !px-3 !text-[12px]" disabled={lwBusy} onClick={runLwjgl}>
              {lwBusy ? <Loader2 size={12} className="animate-spin" /> : <Boxes size={12} />}
              {lw ? "再测一次" : "开始检测"}
            </button>
          </div>
          <p className="text-[12px] text-ccl-muted leading-relaxed mb-3">
            MC 的渲染全靠 LWJGL，而官方 LWJGL 的 <code>natives-linux</code> 依赖{" "}
            <code>libGL.so</code>／<code>libX11</code>，Android 上加载不了。这里解包并加载
            Pojav 那套移植版（<code>libpojavexec</code> + fork 版 GLFW 纯 Java 实现），
            依次验证：natives 能加载 → lwjgl 核心能调 → GLFW 能初始化。
          </p>

          {lwErr && (
            <div className="flex items-start gap-2 text-[12px] text-ccl-danger">
              <X size={13} className="mt-0.5 shrink-0" /> {lwErr}
            </div>
          )}

          {lw && (
            <div className="space-y-2">
              <div
                className={`text-[12px] leading-relaxed rounded-md px-3 py-2 border ${
                  lw.steps_fail === 0
                    ? "border-ccl-accent2/40 bg-ccl-accent2/5 text-ccl-accent2"
                    : lw.vm_ok
                      ? "border-ccl-warn/40 bg-ccl-warn/5 text-ccl-warn"
                      : "border-ccl-danger/40 bg-ccl-danger/5 text-ccl-danger"
                }`}
              >
                {lw.verdict}
              </div>
              <div className="text-[11.5px] bg-ccl-bg border border-ccl-border rounded-md px-3 py-2 space-y-1">
                <Line label="ABI" value={lw.abi} mono />
                <Line
                  label="解包目录"
                  value={`${lw.dir}（${lw.native_count} 个 .so，${
                    lw.prepared_cached ? "复用" : `${lw.prepare_ms} ms`
                  }）`}
                  mono
                />
                <Line label="检查项" value={`${lw.steps_ok} 通过 / ${lw.steps_fail} 失败`} />
                {!lw.vm_ok && <Line label="JVM" value={lw.vm_error} />}
              </div>
              {lw.probe && (
                <pre className="text-[11px] leading-relaxed bg-ccl-bg border border-ccl-border rounded-md px-3 py-2 overflow-x-auto whitespace-pre-wrap text-ccl-muted">
                  {lw.probe.trim()}
                </pre>
              )}
            </div>
          )}
        </div>
      )}

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

/** 自检结果的一行 key-value */
function Line({ label, value, mono }: { label: string; value: string; mono?: boolean }) {
  return (
    <div className="flex gap-2">
      <span className="text-ccl-muted shrink-0 w-[74px]">{label}</span>
      <span className={`flex-1 min-w-0 break-all ${mono ? "font-mono text-[11px]" : ""}`}>{value}</span>
    </div>
  );
}
