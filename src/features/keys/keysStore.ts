import { create } from "zustand";

import { api, errorMessage } from "@/lib/api";
import type { KeysReport } from "@/lib/types";

interface KeysState {
  report: KeysReport | null;
  providers: string[];
  loading: boolean;
  busy: string | null;
  error: string | null;
  load: () => Promise<void>;
  change: (key: string, run: () => Promise<KeysReport>) => Promise<boolean>;
  dismissError: () => void;
}

export const useKeys = create<KeysState>((set, get) => ({
  report: null,
  providers: [],
  loading: false,
  busy: null,
  error: null,
  dismissError: () => set({ error: null }),
  load: async () => {
    if (get().loading) return;
    set({ loading: true });
    try {
      const [report, providers] = await Promise.all([api.keysReport(), api.keysProviders()]);
      set({ report, providers, error: null });
    } catch (e) {
      set({ error: errorMessage(e) });
    } finally {
      set({ loading: false });
    }
  },
  change: async (key, run) => {
    if (get().busy) return false;
    set({ busy: key, error: null });
    try {
      set({ report: await run() });
      return true;
    } catch (e) {
      set({ error: errorMessage(e) });
      return false;
    } finally {
      set({ busy: null });
    }
  },
}));
