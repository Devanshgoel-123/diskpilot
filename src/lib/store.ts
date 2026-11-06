import { create } from "zustand";

import {
  api,
  errorMessage,
  inDesktopApp,
  onScanFinished,
  onScanPartial,
  onScanProgress,
} from "./api";
import { applyTheme, loadTheme, saveTheme, type ThemePreference } from "./theme";
import type {
  AppEnvironment,
  AssistantProvider,
  AssistantStatus,
  ChatEntry,
  CleanupRecord,
  DevToolsReport,
  Recommendation,
  DiskInfo,
  ModelInfo,
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
  | "duplicates"
  | "history"
  | "settings";

export type StorageView = "browse" | "largest";

const ROOT_KEY = "diskpilot.scanRoot";
const ASSISTANT_ENABLED_KEY = "diskpilot.assistant.enabled";
const ASSISTANT_PROVIDER_KEY = "diskpilot.assistant.provider";
/** Anthropic keeps the key used before other providers existed. */
const ASSISTANT_MODEL_KEYS: Record<AssistantProvider, string> = {
  anthropic: "diskpilot.assistant.model",
  openai: "diskpilot.assistant.model.openai",
  openrouter: "diskpilot.assistant.model.openrouter",
};
export const DEFAULT_ASSISTANT_MODEL = "claude-opus-5-5";
export const ASSISTANT_PROVIDERS: AssistantProvider[] = ["anthropic", "openai", "openrouter"];

function readProvider(): AssistantProvider {
  const p = readPref(ASSISTANT_PROVIDER_KEY);
  return ASSISTANT_PROVIDERS.includes(p as AssistantProvider)
    ? (p as AssistantProvider)
    : "anthropic";
}

function readModels(): Record<AssistantProvider, string> {
  return {
    anthropic: readPref(ASSISTANT_MODEL_KEYS.anthropic) ?? DEFAULT_ASSISTANT_MODEL,
    openai: readPref(ASSISTANT_MODEL_KEYS.openai) ?? "",
    openrouter: readPref(ASSISTANT_MODEL_KEYS.openrouter) ?? "",
  };
}

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
  assistantProvider: AssistantProvider;
  /** The chosen model for each provider. */
  assistantModels: Record<AssistantProvider, string>;
  /** The chosen model for the current provider. */
  assistantModel: string;
  /** Models each provider listed when its connection was last checked. */
  providerModels: Partial<Record<AssistantProvider, ModelInfo[]>>;
  assistantStatus: AssistantStatus | null;
  chat: ChatEntry[];
  asking: boolean;

  scanning: boolean;
  /** Results on screen are from the running scan, without app containers. */
  partial: boolean;
  progress: ScanProgress | null;
  lastStatus: { status: ScanStatus; error: string | null } | null;
  error: string | null;

  page: PageId;
  explorerPath: string | null;
  storageView: StorageView;
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
  setAssistantProvider: (provider: AssistantProvider) => void;
  setAssistantModel: (model: string) => void;
  /** Check the provider's key and list its models. Throws on failure. */
  loadProviderModels: (provider: AssistantProvider) => Promise<ModelInfo[]>;
  refreshAssistantStatus: () => Promise<void>;
  ask: (question: string) => Promise<void>;
  resetChat: () => Promise<void>;
  navigate: (page: PageId) => void;
  openInExplorer: (path: string) => void;
  /** Navigate within the Storage Explorer; `null` is the scan root. */
  browse: (path: string | null) => void;
  setStorageView: (view: StorageView) => void;
  setScanRoot: (root: string | null) => void;
  setTheme: (t: ThemePreference) => void;
  dismissError: () => void;
}

let subscribed = false;

/**
 * Providers that list their own models load the list in the background once
 * a key is available, so the model menu and labels are ready. Failures are
 * shown when the person checks the connection in Settings.
 */
function listModelsIfNeeded(): void {
  const {
    assistantProvider: p,
    assistantStatus,
    providerModels,
    loadProviderModels,
  } = useApp.getState();
  const status = assistantStatus?.providers.find((s) => s.id === p);
  if (status?.configured && !status.models.length && !providerModels[p]) {
    loadProviderModels(p).catch(() => undefined);
  }
}

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
  assistantProvider: readProvider(),
  assistantModels: readModels(),
  assistantModel: readModels()[readProvider()],
  providerModels: {},
  assistantStatus: null,
  chat: [],
  asking: false,
  scanning: false,
  partial: false,
  progress: null,
  lastStatus: null,
  error: null,
  page: "overview",
  explorerPath: null,
  storageView: "browse",
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
      await onScanPartial(async () => {
        const [summary, recommendations] = await Promise.all([
          api.getScanSummary(),
          api.getRecommendations(),
        ]);
        // The final results may already have arrived on a fast scan.
        if (!get().scanning) return;
        set({ summary, recommendations, partial: true, explorerPath: null });
      });
      await onScanFinished(async (done) => {
        const hadPartial = get().partial;
        set({
          scanning: false,
          partial: false,
          lastStatus: { status: done.status, error: done.error },
        });
        // A cancelled or failed scan that showed partial results puts the
        // previous results back; fetch them as well.
        if (done.status === "completed" || hadPartial) {
          const [summary, recommendations] = await Promise.all([
            api.getScanSummary(),
            api.getRecommendations(),
          ]);
          // Keep the folder being browsed when the containers fill in.
          const keepPlace = hadPartial && done.status === "completed";
          set(
            keepPlace
              ? { summary, recommendations }
              : { summary, recommendations, explorerPath: null },
          );
        }
        if (done.status === "completed") {
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
    set({ error: null, lastStatus: null, progress: null, partial: false });
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

  setAssistantProvider: (provider) => {
    writePref(ASSISTANT_PROVIDER_KEY, provider);
    // The engine starts a new conversation when the provider changes.
    set((s) => ({
      assistantProvider: provider,
      assistantModel: s.assistantModels[provider],
      chat: [],
    }));
    listModelsIfNeeded();
  },

  setAssistantModel: (model) => {
    const provider = get().assistantProvider;
    writePref(ASSISTANT_MODEL_KEYS[provider], model);
    // The engine starts a new conversation when the model changes.
    set((s) => ({
      assistantModels: { ...s.assistantModels, [provider]: model },
      assistantModel: model,
      chat: [],
    }));
  },

  loadProviderModels: async (provider) => {
    const models = await api.assistantModels(provider);
    set((s) => ({ providerModels: { ...s.providerModels, [provider]: models } }));
    // Pick the first listed model when none is chosen or the chosen one is gone.
    const current = get().assistantModels[provider];
    const first = models[0];
    if (first && !models.some((m) => m.id === current)) {
      writePref(ASSISTANT_MODEL_KEYS[provider], first.id);
      set((s) => ({
        assistantModels: { ...s.assistantModels, [provider]: first.id },
        assistantModel: s.assistantProvider === provider ? first.id : s.assistantModel,
      }));
    }
    return models;
  },

  refreshAssistantStatus: async () => {
    try {
      set({ assistantStatus: await api.assistantStatus() });
      listModelsIfNeeded();
    } catch (e) {
      set({ error: errorMessage(e) });
    }
  },

  ask: async (question) => {
    const q = question.trim();
    if (!q || get().asking) return;
    set((s) => ({ asking: true, chat: [...s.chat, { role: "user", text: q }] }));
    try {
      const { assistantProvider, assistantModel } = get();
      const reply = await api.assistantAsk(q, assistantProvider, assistantModel);
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

  openInExplorer: (path) => set({ page: "explorer", storageView: "browse", explorerPath: path }),

  browse: (path) => set({ explorerPath: path }),

  setStorageView: (storageView) => set({ storageView }),

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
