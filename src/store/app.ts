import { create } from "zustand";
import { api, type AppSettings, type Instance, type Account, type UpdateInfo, type DlProgress } from "../lib/tauri";
import { isMobile } from "../lib/env";

interface AppState {
  settings: AppSettings;
  instances: Instance[];
  accounts: Account[];
  update: UpdateInfo | null;
  downloads: Record<string, DlProgress>;
  running: Record<string, boolean>;
  logs: Record<string, string[]>;
  toast: { text: string; kind: "ok" | "bad" } | null;
  /** PCL 首页左栏选中的实例 */
  currentInstanceId: string | null;

  loadSettings: () => Promise<void>;
  patchSettings: (patch: Partial<AppSettings>) => Promise<void>;
  loadInstances: () => Promise<void>;
  loadAccounts: () => Promise<void>;
  checkUpdate: () => Promise<void>;
  setDownload: (p: DlProgress) => void;
  setRunning: (id: string, running: boolean) => void;
  appendLog: (id: string, line: string) => void;
  clearLogs: (id: string) => void;
  showToast: (text: string, kind?: "ok" | "bad") => void;
  setCurrentInstance: (id: string | null) => void;
}

const defaultSettings: AppSettings = {
  download_source: "bmclapi",
  max_memory: 4096,
  jvm_args: "",
  auto_check_update: true,
  close_action: "minimize",
  curseforge_api_key: "",
  java_path: "",
  window_width: null,
  window_height: null,
  microsoft_client_id: "",
  game_folder: "",
  version_isolation: 4,
  custom_folders: [],
};

export const useApp = create<AppState>((set, get) => ({
  settings: defaultSettings,
  instances: [],
  accounts: [],
  update: null,
  downloads: {},
  running: {},
  logs: {},
  toast: null,
  currentInstanceId: localStorage.getItem("ccl-current-instance"),

  setCurrentInstance: (id) => {
    if (id) localStorage.setItem("ccl-current-instance", id);
    else localStorage.removeItem("ccl-current-instance");
    set({ currentInstanceId: id });
  },

  loadSettings: async () => {
    try {
      const s = await api.getSettings();
      set({ settings: { ...defaultSettings, ...s } });
    } catch {
      /* 首次运行忽略 */
    }
  },

  patchSettings: async (patch) => {
    const next = { ...get().settings, ...patch };
    set({ settings: next });
    await api.saveSettings(next);
  },

  loadInstances: async () => {
    set({ instances: await api.listInstances() });
  },

  loadAccounts: async () => {
    set({ accounts: await api.accountsList() });
  },

  checkUpdate: async () => {
    // 移动端（Android）不做云控更新检查：更新通道只有 Windows 安装包
    if (isMobile()) return;
    try {
      const u = await api.checkCloudUpdate();
      set({ update: u });
    } catch {
      /* 离线忽略 */
    }
  },

  setDownload: (p) =>
    set((s) => {
      const next = { ...s.downloads, [p.id]: p };
      if (p.finished || p.error) {
        // 完成后 3 秒移除
        setTimeout(() => {
          set((s2) => {
            const copy = { ...s2.downloads };
            delete copy[p.id];
            return { downloads: copy };
          });
        }, 3000);
      }
      return { downloads: next };
    }),

  setRunning: (id, running) =>
    set((s) => ({ running: { ...s.running, [id]: running } })),

  appendLog: (id, line) =>
    set((s) => ({
      logs: { ...s.logs, [id]: [...(s.logs[id] || []), line].slice(-2000) },
    })),

  clearLogs: (id) => set((s) => ({ logs: { ...s.logs, [id]: [] } })),

  showToast: (text, kind = "ok") => {
    set({ toast: { text, kind } });
    setTimeout(() => set({ toast: null }), 3500);
  },
}));
