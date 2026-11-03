import { create } from "zustand";

import { api } from "@/lib/api";
import type { TrashSummary } from "@/lib/types";

/** How many of the largest entries are drawn as cards on the stage. */
export const STAGE_CHIPS = 5;

/**
 * Whether the Empty Trash window is open, and a cheap summary of the Trash
 * for the button. The window itself lists the Trash again when it opens, so
 * the summary only ever labels the button.
 */
interface TrashUi {
  open: boolean;
  /** Increments each time the window opens, so every opening starts fresh. */
  session: number;
  summary: TrashSummary | null;
  openDialog: () => void;
  setOpen: (open: boolean) => void;
  refresh: () => Promise<void>;
}

export const useTrash = create<TrashUi>((set) => ({
  open: false,
  session: 0,
  summary: null,
  openDialog: () => set((s) => ({ open: true, session: s.session + 1 })),
  setOpen: (open) => set({ open }),
  refresh: async () => {
    try {
      set({ summary: await api.getTrashSummary() });
    } catch {
      set({ summary: null });
    }
  },
}));
