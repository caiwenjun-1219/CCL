// 资源包 / 光影包下载页：复用 Modrinth 推荐位，装到选定实例的对应目录

import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import { Sparkles, Wrench } from "lucide-react";
import { useApp } from "../store/app";
import { ResourceRecommend } from "../components/Recommend";

export default function ResourcePage({ kind }: { kind: "resourcepack" | "shader" }) {
  const navigate = useNavigate();
  const { instances } = useApp();
  const [instanceId, setInstanceId] = useState("");

  useEffect(() => {
    if (instances.length > 0 && !instanceId) setInstanceId(instances[0].id);
  }, [instances]);

  const label = kind === "resourcepack" ? "资源包" : "光影包";
  const tab = kind === "resourcepack" ? "resourcepacks" : "shaderpacks";

  return (
    <div>
      <div className="flex flex-wrap items-center gap-3 mb-4">
        <h1 className="pcl-title">{label}</h1>
        <select
          className="field !w-52"
          value={instanceId}
          onChange={(e) => setInstanceId(e.target.value)}
        >
          {instances.length === 0 && <option value="">（还没有实例）</option>}
          {instances.map((i) => (
            <option key={i.id} value={i.id}>
              {i.name}
            </option>
          ))}
        </select>
        <span className="text-[11.5px] text-ccl-muted inline-flex items-center gap-1">
          <Sparkles size={11} />
          安装后到实例详情页的「{label}」中启用
        </span>
        <div className="flex-1" />
        <button
          className="btn btn-ghost"
          disabled={!instanceId}
          onClick={() => navigate(`/instance/${instanceId}?tab=${tab}`)}
          title={`管理已安装的${label}`}
        >
          <Wrench size={13} /> {label}管理
        </button>
      </div>

      <ResourceRecommend instanceId={instanceId} kind={kind} />
    </div>
  );
}
