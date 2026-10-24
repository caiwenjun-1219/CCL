import { useEffect, useState } from "react";
import { UserPlus, Trash2, RefreshCw, ExternalLink, Loader2, LogIn, Wifi } from "lucide-react";
import { openPath } from "@tauri-apps/plugin-opener";
import { useApp } from "../store/app";
import { api, type Account } from "../lib/tauri";

type PollResult = { Pending?: never } | { Done: Account } | { Failed: string };

export default function AccountsPage() {
  const { accounts, loadAccounts, showToast } = useApp();
  const [msState, setMsState] = useState<{
    device_code: string;
    user_code: string;
    verification_uri: string;
    interval: number;
  } | null>(null);
  const [offlineName, setOfflineName] = useState("");
  const [busy, setBusy] = useState(false);

  const startMsLogin = async () => {
    setBusy(true);
    try {
      const info = await api.msLoginStart();
      setMsState({
        device_code: info.device_code,
        user_code: info.user_code,
        verification_uri: info.verification_uri,
        interval: info.interval,
      });
      openPath(info.verification_uri).catch(() => {});
    } catch (e) {
      showToast(String(e), "bad");
    } finally {
      setBusy(false);
    }
  };

  // 轮询登录状态
  useEffect(() => {
    if (!msState) return;
    let stopped = false;
    let timer: ReturnType<typeof setTimeout>;

    const poll = async () => {
      if (stopped) return;
      try {
        const res = (await api.msLoginPoll(msState.device_code)) as PollResult;
        if (stopped) return;
        if ("Done" in res && res.Done) {
          showToast(`登录成功：${res.Done.name}`);
          setMsState(null);
          loadAccounts();
          return;
        }
        if ("Failed" in res && res.Failed) {
          showToast(res.Failed, "bad");
          setMsState(null);
          return;
        }
      } catch (e) {
        showToast(String(e), "bad");
        setMsState(null);
        return;
      }
      timer = setTimeout(poll, Math.max(msState.interval, 3) * 1000);
    };
    timer = setTimeout(poll, Math.max(msState.interval, 3) * 1000);
    return () => {
      stopped = true;
      clearTimeout(timer);
    };
  }, [msState]);

  const offline = async () => {
    setBusy(true);
    try {
      const acc = await api.offlineLogin(offlineName.trim());
      showToast(`离线账户已添加：${acc.name}`);
      setOfflineName("");
      loadAccounts();
    } catch (e) {
      showToast(String(e), "bad");
    } finally {
      setBusy(false);
    }
  };

  const remove = async (uuid: string, name: string) => {
    if (!confirm(`移除账户 ${name}？`)) return;
    await api.removeAccount(uuid);
    loadAccounts();
  };

  const refresh = async (uuid: string) => {
    try {
      await api.refreshAccount(uuid);
      showToast("令牌已刷新");
      loadAccounts();
    } catch (e) {
      showToast(String(e), "bad");
    }
  };

  return (
    <div>
      <h1 className="pcl-title mb-4">游戏账户</h1>

      <div className="grid grid-cols-1 lg:grid-cols-2 gap-5">
        {/* 添加账户 */}
        <div className="card">
          <h2 className="text-[13px] font-bold mb-3">添加账户</h2>

          {msState ? (
            <div className="bg-ccl-bg border border-ccl-border rounded-lg p-4 text-center">
              <Loader2 size={22} className="animate-spin mx-auto text-ccl-accent2 mb-2.5" />
              <p className="text-[13px] mb-2">等待微软授权…</p>
              <p className="text-[12px] text-ccl-muted mb-2">
                请在浏览器中输入验证码：
              </p>
              <div className="text-2xl font-extrabold tracking-[0.3em] text-ccl-accent2 my-2">
                {msState.user_code}
              </div>
              <p className="text-[11px] text-ccl-muted mb-3">{msState.verification_uri}</p>
              <button
                className="btn btn-ghost !py-1 !px-3"
                onClick={() => openPath(msState.verification_uri).catch(() => {})}
              >
                <ExternalLink size={12} /> 重新打开页面
              </button>
              <button className="btn btn-ghost !py-1 !px-3 ml-2" onClick={() => setMsState(null)}>
                取消
              </button>
            </div>
          ) : (
            <>
              <button className="btn btn-primary w-full mb-4" onClick={startMsLogin} disabled={busy}>
                <LogIn size={13} /> 微软账户登录（正版）
              </button>
              <div className="border-t border-ccl-border pt-4">
                <label className="text-[12px] text-ccl-muted block mb-1.5">离线模式用户名</label>
                <div className="flex gap-2">
                  <input
                    className="field"
                    placeholder="1-16 位字母/数字/下划线"
                    value={offlineName}
                    onChange={(e) => setOfflineName(e.target.value)}
                  />
                  <button className="btn btn-ghost" onClick={offline} disabled={busy}>
                    <Wifi size={13} /> 添加
                  </button>
                </div>
                <p className="text-[11px] text-ccl-muted mt-2 leading-relaxed">
                  离线模式只能进入允许正版验证关闭的服务器；微软登录需要拥有 Minecraft Java 版。
                </p>
              </div>
            </>
          )}
        </div>

        {/* 账户列表 */}
        <div className="card">
          <h2 className="text-[13px] font-bold mb-3">已添加账户（{accounts.length}）</h2>
          {accounts.length === 0 ? (
            <p className="text-[12px] text-ccl-muted">尚未添加任何账户</p>
          ) : (
            <div className="space-y-2">
              {accounts.map((a) => (
                <div
                  key={a.uuid}
                  className="bg-ccl-bg border border-ccl-border rounded-lg px-3.5 py-2.5 flex items-center gap-3"
                >
                  <div className="w-9 h-9 rounded-lg bg-ccl-accent/10 border border-ccl-accent/25 flex items-center justify-center text-[12px] font-bold text-ccl-accent shrink-0">
                    {a.name.slice(0, 2).toUpperCase()}
                  </div>
                  <div className="min-w-0 flex-1">
                    <div className="text-[13px] font-bold truncate">{a.name}</div>
                    <div className="text-[10px] text-ccl-muted truncate">
                      {a.account_type === "msa" ? "微软正版" : "离线模式"} · {a.uuid.slice(0, 8)}
                    </div>
                  </div>
                  {a.account_type === "msa" && (
                    <button className="btn btn-ghost !py-1 !px-2" title="刷新令牌" onClick={() => refresh(a.uuid)}>
                      <RefreshCw size={11} />
                    </button>
                  )}
                  <button className="btn btn-danger !py-1 !px-2" onClick={() => remove(a.uuid, a.name)}>
                    <Trash2 size={11} />
                  </button>
                </div>
              ))}
            </div>
          )}
          <p className="text-[11px] text-ccl-muted mt-3 leading-relaxed">
            启动游戏时默认使用列表第一个账户。多个账户可在首页启动按钮下拉切换（后续版本支持）。
          </p>
        </div>
      </div>
    </div>
  );
}
