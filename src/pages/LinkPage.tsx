import { Users, Wifi, ShieldCheck } from "lucide-react";

/** 联机页（PCL 的「联机」页签）——当前版本尚未接入房间服务，展示空态 */
export default function LinkPage() {
  return (
    <div className="h-full flex flex-col items-center justify-center text-center select-none">
      <div className="w-[72px] h-[72px] rounded-full flex items-center justify-center bg-ccl-brush8 text-ccl-accent mb-5">
        <Users size={34} strokeWidth={1.6} />
      </div>
      <h2 className="text-[16px] font-bold mb-1.5">联机大厅</h2>
      <p className="text-[12.5px] text-ccl-gray3 leading-relaxed max-w-[380px]">
        联机房间功能尚未开放。你可以先在「版本管理」中创建一个服务器专用实例，
        <br />
        或使用游戏内的多人游戏功能连接局域网 / 服务器。
      </p>

      <div className="grid grid-cols-2 gap-3 mt-8 w-[520px] max-w-full text-left">
        <div className="border border-ccl-gray6 rounded-[3px] p-3.5">
          <div className="flex items-center gap-2 text-[13px] font-bold mb-1">
            <Wifi size={14} className="text-ccl-accent" /> 局域网联机
          </div>
          <p className="text-[11.5px] text-ccl-gray3 leading-relaxed">
            同一网络下，在游戏内「对局域网开放」即可被自动发现，无需额外配置。
          </p>
        </div>
        <div className="border border-ccl-gray6 rounded-[3px] p-3.5">
          <div className="flex items-center gap-2 text-[13px] font-bold mb-1">
            <ShieldCheck size={14} className="text-ccl-accent" /> 正版验证服
          </div>
          <p className="text-[11.5px] text-ccl-gray3 leading-relaxed">
            使用微软正版账户登录后，可直接进入需要正版验证的服务器。
          </p>
        </div>
      </div>
    </div>
  );
}
