import { create } from "zustand";

import { api, errorMessage } from "@/lib/api";
import { formatBytes } from "@/lib/format";
import { isWindows } from "@/lib/platform";
import type { SystemExtra } from "@/lib/types";

/** Areas smaller than this are left out: not worth a trip to Windows' tools. */
export const MIN_EXTRA_BYTES = 200_000_000;

interface ExtrasState {
  extras: SystemExtra[] | null;
  error: string | null;
  load: () => Promise<void>;
}

/** Read once per launch; sizes of these areas change slowly. */
export const useSystemExtras = create<ExtrasState>((set, get) => ({
  extras: null,
  error: null,
  load: async () => {
    if (!isWindows() || get().extras) return;
    try {
      set({ extras: await api.getSystemExtras(), error: null });
    } catch (e) {
      set({ error: errorMessage(e) });
    }
  },
}));

/** The areas worth showing, largest first. Unmeasured areas are kept: they may be large. */
export function visibleExtras(extras: SystemExtra[]): SystemExtra[] {
  return extras
    .filter((e) => e.measured === "unavailable" || e.bytes >= MIN_EXTRA_BYTES)
    .sort((a, b) => b.bytes - a.bytes);
}

export function sizeLabel(e: SystemExtra): string {
  if (e.measured === "unavailable") return "Size unknown";
  const size = formatBytes(e.bytes);
  return e.measured === "partial" ? `At least ${size}` : size;
}
