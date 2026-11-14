import { useMemo } from "react";
import { create } from "zustand";

import { api, errorMessage } from "@/lib/api";
import { useApp } from "@/lib/store";
import type { AgentsReport } from "@/lib/types";

export type AgentTab = "permissions" | "connections" | "plugins" | "skills" | "cleanup";

interface AgentsState {
  report: AgentsReport | null;
  loading: boolean;
  /** Key of the change being made, so only its button shows progress. */
  busy: string | null;
  error: string | null;
  selected: string | null;
  tab: AgentTab;
  select: (id: string) => void;
  setTab: (tab: AgentTab) => void;
  dismissError: () => void;
  load: () => Promise<void>;
  analyze: () => Promise<void>;
  /** Make a change; the backend answers with the report as it is now. */
  change: (key: string, run: () => Promise<AgentsReport>) => Promise<boolean>;
}

/** Cleanup buttons depend on the app-wide list of recommendations. */
async function refreshRecommendations() {
  try {
    useApp.setState({ recommendations: await api.getRecommendations() });
  } catch {
    // The page still works; the Cleanup page refreshes on its own.
  }
}

export const useAgents = create<AgentsState>((set, get) => ({
  report: null,
  loading: false,
  busy: null,
  error: null,
  selected: null,
  tab: "permissions",
  select: (selected) => set({ selected }),
  setTab: (tab) => set({ tab }),
  dismissError: () => set({ error: null }),

  load: async () => {
    if (!get().report) {
      const cached = await api.getAgentsReport().catch(() => null);
      if (cached && !get().report) set({ report: cached });
    }
    await get().analyze();
  },

  analyze: async () => {
    if (get().loading) return;
    set({ loading: true });
    try {
      const report = await api.analyzeAgents();
      set({ report, error: null });
      await refreshRecommendations();
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
      const report = await run();
      set({ report });
      await refreshRecommendations();
      return true;
    } catch (e) {
      set({ error: errorMessage(e) });
      return false;
    } finally {
      set({ busy: null });
    }
  },
}));

/** Ids of recommendations that can still be reviewed. */
export function useReviewable() {
  const recommendations = useApp((s) => s.recommendations);
  return useMemo(
    () => new Set(recommendations.filter((r) => r.methods.length).map((r) => r.id)),
    [recommendations],
  );
}
