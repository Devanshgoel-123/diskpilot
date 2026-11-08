import { create } from "zustand";

import { api, errorMessage } from "@/lib/api";
import { isWindows } from "@/lib/platform";
import type { VirtualDisk, VirtualDiskKind, WslOverview } from "@/lib/types";

/**
 * WSL and Docker Desktop virtual disks, shared by the Overview card and the
 * Developer Tools section so WSL is asked once. Windows only.
 */
interface WslState {
  overview: WslOverview | null;
  loading: boolean;
  error: string | null;
  /** Read the disks again, for example after a compaction. */
  load: () => Promise<void>;
  /** Read the disks unless they were read already or are being read. */
  ensure: () => void;
}

export const useWsl = create<WslState>((set, get) => ({
  overview: null,
  loading: false,
  error: null,
  load: async () => {
    if (!isWindows()) return;
    set({ loading: true });
    try {
      set({ overview: await api.wslDisks(), error: null });
    } catch (e) {
      set({ error: errorMessage(e) });
    } finally {
      set({ loading: false });
    }
  },
  ensure: () => {
    const { overview, loading, error } = get();
    if (!overview && !loading && !error) void get().load();
  },
}));

export const KIND_LABEL: Record<VirtualDiskKind, string> = {
  distro: "WSL distribution",
  dockerData: "Docker data",
  dockerSystem: "Docker system",
};

/** Space worth showing on the Overview: at least 1 GB that compaction can return. */
export const WORTH_SHOWING_BYTES = 1_000_000_000;

/** "Sparse" disks give space back by themselves; only distributions can be made sparse. */
export function canMakeSparse(disk: VirtualDisk): boolean {
  return disk.kind === "distro" && !disk.sparse;
}
