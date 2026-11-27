import { useEffect, useState } from "react";
import { Github, Globe, Cpu, Boxes, Cloud, ExternalLink } from "lucide-react";
import { useApp } from "../store/app";
import { api } from "../lib/tauri";
import { CclLogo } from "../components/PclIcon";

const STACK = [
  "Tauri 2",
  "React 19",
  "TypeScript",
  "Rust",
  "Tailwind CSS 4",
  "Cloudflare Workers",
];

export default function AboutPage() {
  const { update } = useApp();
  const [version, setVersion] = useState("");
  const [openUrl, setOpenUrl] = useState<((u: string) => void) | null>(null);

  useEffect(() => {
    api.appVersion().then(setVersion).catch(() => setVersion("1.0.0"));
    import("@tauri-apps/plugin-opener")
      .then((m) => setOpenUrl(() => m.openUrl))
      .catch(() => {});
  }, []);

  const link = (url: string) => () => {
    if (openUrl) openUrl(url);
  };

  return (
    <div className="max-w-[820px]">
      <h1 className="pcl-title mb-4">关于</h1>

      <div className="flex gap-6 items-start">
        {/* 左侧：Logo 卡 */}
        <div
          className="w-[180px] h-[180px] rounded-[4px] shrink-0 flex flex-col items-center justify-center gap-3 select-none"
          style={{
            background: "linear-gradient(140deg,#4b93f6 0%,#2b7ef4 50%,#1370f3 100%)",
            boxShadow: "0 10px 28px rgba(19,112,243,.28)",
          }}
        >
          <CclLogo width={86} />
          <div className="text-white/90 text-[11.5px] tracking-wide">Cwj Craft Launcher</div>
        </div>

        {/* 右侧：信息 */}
        <div className="flex-1 min-w-0">
          <div className="text-[15px] font-bold">Cwj Craft Launcher</div>
          <div className="text-[12px] text-ccl-gray3 mt-0.5">
            基于 Tauri 的 Minecraft 启动器 · Windows 优先
          </div>

          <div className="mt-3 text-[12.5px] space-y-1.5">
            <div className="flex gap-2">
              <span className="text-ccl-gray3 w-[76px] shrink-0">当前版本</span>
              <span className="font-semibold">v{version || update?.current_version || "…"}</span>
            </div>
            <div className="flex gap-2">
              <span className="text-ccl-gray3 w-[76px] shrink-0">最新版本</span>
              <span className={update?.has_update ? "text-ccl-accent font-semibold" : ""}>
                v{update?.latest_version || version || "…"}
              </span>
            </div>
            <div className="flex gap-2">
              <span className="text-ccl-gray3 w-[76px] shrink-0">云控服务</span>
              <span className={update?.cloud_online ? "text-[#2e7d46]" : "text-ccl-gray3"}>
                {update?.cloud_online ? "在线" : "离线"}
                <span className="text-ccl-gray4 ml-1">cclconfig.caiwenjun.top</span>
              </span>
            </div>
          </div>

          <p className="text-[12.5px] text-ccl-gray2 leading-relaxed mt-4">
            支持原版 / Fabric / Quilt / Forge / NeoForge 全加载器，内置 BMCLAPI 国内镜像下载、
            微软正版登录、Modrinth 与 CurseForge 模组市场、整合包一键导入导出，以及云控静默更新。
          </p>

          <div className="flex flex-wrap gap-1.5 mt-3">
            {STACK.map((s) => (
              <span key={s} className="tag tag-gray">
                {s}
              </span>
            ))}
          </div>

          <div className="flex gap-2 mt-5">
            <button className="btn !h-[30px]" onClick={link("https://caiwenjun.top")}>
              <Globe size={13} /> 个人主页
            </button>
            <button className="btn !h-[30px]" onClick={link("https://github.com/caiwenjun-1219/CCL")}>
              <Github size={13} /> 源码仓库
            </button>
          </div>
        </div>
      </div>

      {/* 功能概览 */}
      <div className="mt-7 grid grid-cols-3 gap-3">
        {[
          { icon: Boxes, title: "版本与加载器", desc: "四类加载器一键安装，classpath 自动组装" },
          { icon: Cpu, title: "Java 托管", desc: "扫描本机 JDK，或按 MC 版本自动下载" },
          { icon: Cloud, title: "云控更新", desc: "公告、强制更新、SHA256 校验与静默安装" },
        ].map((f) => (
          <div key={f.title} className="border border-ccl-gray6 rounded-[3px] p-3">
            <div className="flex items-center gap-1.5 text-[12.5px] font-bold mb-1">
              <f.icon size={13} className="text-ccl-accent" /> {f.title}
            </div>
            <div className="text-[11.5px] text-ccl-gray3 leading-relaxed">{f.desc}</div>
          </div>
        ))}
      </div>

      <div className="mt-6 text-[11.5px] text-ccl-gray4 flex items-center gap-1.5">
        <ExternalLink size={12} />
        界面视觉参考 Plain Craft Launcher 2（Meloong-Git/PCL），仅作致敬，非官方关联。
      </div>
    </div>
  );
}
