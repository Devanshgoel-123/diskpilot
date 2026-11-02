import { create } from "zustand";

import { api, errorMessage, inDesktopApp, onScanFinished, onScanProgress } from "./api";
import { applyTheme, loadTheme, saveTheme, type ThemePreference } from "./theme";
import type {
  AppEnvironment,
  AssistantStatus,
  ChatEntry,
  CleanupRecord,
  DevToolsReport,
  Recommendation,
  DiskInfo,
  ScanProgress,
  ScanRecord,
  ScanStatus,
  ScanSummary,
} from "./types";

export type PageId =
  | "assistant"
  | "overview"
  | "explorer"
  | "recommendations"
  | "applications"
  | "docker"
  | "developer"
  | "large-files"
  | "history"
  | "settings";

const ROOT_KEY = "diskpilot.scanRoot";
const ASSISTANT_ENABLED_KEY = "diskpilot.assistant.enabled";
const ASSISTANT_MODEL_KEY = "diskpilot.assistant.model";
export const DEFAULT_ASSISTANT_MODEL = "claude-opus-5-5";

function readPref(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

function writePref(key: string, value: string): void {
  try {
    localStorage.setItem(key, value);
  } catch {
    /* storage unavailable */
  }
}

function loadScanRoot(): string | null {
  try {
    return localStorage.getItem(ROOT_KEY);
  } catch {
    return null;
  }
}

interface AppState {
  ready: boolean;
  desktop: boolean;
  env: AppEnvironment | null;
  disk: DiskInfo | null;
  summary: ScanSummary | null;
  history: ScanRecord[];
  recommendations: Recommendation[];
  cleanups: CleanupRecord[];
  devtools: DevToolsReport | null;
  analyzingDevTools: boolean;
  /** Recommendation ids to preselect when the Recommendations page opens. */
  preselect: string[] | null;

  /** The assistant is off until the user enables it in Settings. */
  assistantEnabled: boolean;
  assistantModel: string;
  assistantStatus: AssistantStatus | null;
  chat: ChatEntry[];
  asking: boolean;

  scanning: boolean;
  progress: ScanProgress | null;
  lastStatus: { status: ScanStatus; error: string | null } | null;
  error: string | null;

  page: PageId;
  explorerPath: string | null;
  /** Folder to scan; `null` means the home folder. */
  scanRoot: string | null;
  theme: ThemePreference;

  init: () => Promise<void>;
  startScan: () => Promise<void>;
  cancelScan: () => Promise<void>;
  refreshDisk: () => Promise<void>;
  refreshHistory: () => Promise<void>;
  refreshRecommendations: () => Promise<void>;
  /** Delete the saved scan and clear the results on screen. */
  forgetScan: () => Promise<void>;
  analyzeDevTools: () => Promise<void>;
  reviewInRecommendations: (ids: string[]) => void;
  consumePreselect: () => string[] | null;
  setAssistantEnabled: (on: boolean) => void;
  setAssistantModel: (model: string) => void;
  refreshAssistantStatus: () => Promise<void>;
  ask: (question: string) => Promise<void>;
  resetChat: () => Promise<void>;
  navigate: (page: PageId) => void;
  openInExplorer: (path: string) => void;
  /** Navigate within the Storage Explorer; `null` is the scan root. */
  browse: (path: string | null) => void;
  setScanRoot: (root: string | null) => void;
  setTheme: (t: ThemePreference) => void;
  dismissError: () => void;
}

let subscribed = false;

export const useApp = create<AppState>((set, get) => ({
  ready: false,
  desktop: false,
  env: null,
  disk: null,
  summary: null,
  history: [],
  recommendations: [],
  cleanups: [],
  devtools: null,
  analyzingDevTools: false,
  preselect: null,
  assistantEnabled: readPref(ASSISTANT_ENABLED_KEY) === "true",
  assistantModel: readPref(ASSISTANT_MODEL_KEY) ?? DEFAULT_ASSISTANT_MODEL,
  assistantStatus: null,
  chat: [],
  asking: false,
  scanning: false,
  progress: null,
  lastStatus: null,
  error: null,
  page: "overview",
  explorerPath: null,
  scanRoot: loadScanRoot(),
  theme: loadTheme(),

  init: async () => {
    applyTheme(get().theme);
    if (!inDesktopApp()) {
      set({ ready: true, desktop: false });
      return;
    }
    set({ desktop: true });

    if (!subscribed) {
      subscribed = true;
      await onScanProgress((progress) => set({ progress }));
      await onScanFinished(async (done) => {
        set({ scanning: false, lastStatus: { status: done.status, error: done.error } });
        if (done.status === "completed") {
          const [summary, recommendations] = await Promise.all([
            api.getScanSummary(),
            api.getRecommendations(),
          ]);
          set({ summary, recommendations, explorerPath: null });
          // Version and per-tool checks feed Cleanup too, so refresh them now.
          void get().analyzeDevTools();
        }
        void get().refreshDisk();
        void get().refreshHistory();
      });
    }

    try {
      const [env, disk, summary, scanning, history, recommendations, cleanups] = await Promise.all([
        api.getEnvironment(),
        api.getDiskInfo(),
        api.getScanSummary(),
        api.isScanRunning(),
        api.getScanHistory(50),
        api.getRecommendations(),
        api.getCleanupHistory(50),
      ]);
      set({ env, disk, summary, scanning, history, recommendations, cleanups, ready: true });
      // Results saved from the last launch: re-run the tool checks, which
      // read current versions and installers rather than saved data.
      if (summary) void get().analyzeDevTools();
      else void api.getDevToolsReport().then((devtools) => set({ devtools }));
      void get().refreshAssistantStatus();
    } catch (e) {
      set({ error: errorMessage(e), ready: true });
    }
  },

  startScan: async () => {
    set({ error: null, lastStatus: null, progress: null });
    try {
      await api.startScan(get().scanRoot ?? undefined);
      set({ scanning: true });
    } catch (e) {
      set({ error: errorMessage(e) });
    }
  },

  cancelScan: async () => {
    try {
      await api.cancelScan();
    } catch (e) {
      set({ error: errorMessage(e) });
    }
  },

  refreshDisk: async () => {
    try {
      set({ disk: await api.getDiskInfo() });
    } catch (e) {
      set({ error: errorMessage(e) });
    }
  },

  refreshHistory: async () => {
    try {
      const [history, cleanups] = await Promise.all([
        api.getScanHistory(50),
        api.getCleanupHistory(50),
      ]);
      set({ history, cleanups });
    } catch (e) {
      set({ error: errorMessage(e) });
    }
  },

  analyzeDevTools: async () => {
    if (get().analyzingDevTools) return;
    set({ analyzingDevTools: true });
    try {
      const devtools = await api.analyzeDevTools();
      const recommendations = await api.getRecommendations();
      set({ devtools, recommendations });
    } catch (e) {
      set({ error: errorMessage(e) });
    } finally {
      set({ analyzingDevTools: false });
    }
  },

  reviewInRecommendations: (ids) => set({ preselect: ids, page: "recommendations" }),

  consumePreselect: () => {
    const ids = get().preselect;
    if (ids) set({ preselect: null });
    return ids;
  },

  setAssistantEnabled: (on) => {
    writePref(ASSISTANT_ENABLED_KEY, String(on));
    set({ assistantEnabled: on });
  },

  setAssistantModel: (model) => {
    writePref(ASSISTANT_MODEL_KEY, model);
    // The engine starts a new conversation when the model changes.
    set({ assistantModel: model, chat: [] });
  },

  refreshAssistantStatus: async () => {
    try {
      set({ assistantStatus: await api.assistantStatus() });
    } catch (e) {
      set({ error: errorMessage(e) });
    }
  },

  ask: async (question) => {
    const q = question.trim();
    if (!q || get().asking) return;
    set((s) => ({ asking: true, chat: [...s.chat, { role: "user", text: q }] }));
    try {
      const reply = await api.assistantAsk(q, get().assistantModel);
      set((s) => ({ chat: [...s.chat, { role: "assistant", reply }] }));
    } catch (e) {
      set((s) => ({ chat: [...s.chat, { role: "error", text: errorMessage(e) }] }));
    } finally {
      set({ asking: false });
    }
  },

  resetChat: async () => {
    try {
      await api.assistantReset();
    } finally {
      set({ chat: [] });
    }
  },

  refreshRecommendations: async () => {
    try {
      set({ recommendations: await api.getRecommendations() });
    } catch (e) {
      set({ error: errorMessage(e) });
    }
  },

  forgetScan: async () => {
    try {
      await api.clearSavedScan();
      set({ summary: null, explorerPath: null, recommendations: await api.getRecommendations() });
    } catch (e) {
      set({ error: errorMessage(e) });
    }
  },

  navigate: (page) => set({ page }),

  openInExplorer: (path) => set({ page: "explorer", explorerPath: path }),

  browse: (path) => set({ explorerPath: path }),

  setScanRoot: (root) => {
    try {
      if (root) localStorage.setItem(ROOT_KEY, root);
      else localStorage.removeItem(ROOT_KEY);
    } catch {
      /* storage unavailable */
    }
    set({ scanRoot: root });
  },

  setTheme: (theme) => {
    saveTheme(theme);
    applyTheme(theme);
    set({ theme });
  },

  dismissError: () => set({ error: null }),
}));
