import { relaunch } from "@tauri-apps/plugin-process";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { create } from "zustand";

import { errorMessage } from "@/lib/api";

const AUTO_KEY = "diskpilot.updates.auto";
const DAY_MS = 24 * 60 * 60 * 1000;

function readAuto(): boolean {
  try {
    return localStorage.getItem(AUTO_KEY) !== "off";
  } catch {
    return true;
  }
}

interface UpdatesState {
  /** Check GitHub for a newer release once a day. On unless turned off. */
  autoCheck: boolean;
  checking: boolean;
  /** A newer version, ready to install; null when up to date or not checked. */
  update: Update | null;
  /** Set after a manual check finds nothing newer. */
  upToDate: boolean;
  lastCheckedMs: number | null;
  error: string | null;
  /** Bytes downloaded and total while installing. */
  progress: { done: number; total: number | null } | null;
  installing: boolean;
  setAutoCheck: (on: boolean) => void;
  checkNow: (manual?: boolean) => Promise<void>;
  install: () => Promise<void>;
}

export const useUpdates = create<UpdatesState>((set, get) => ({
  autoCheck: readAuto(),
  checking: false,
  update: null,
  upToDate: false,
  lastCheckedMs: null,
  error: null,
  progress: null,
  installing: false,

  setAutoCheck: (on) => {
    try {
      localStorage.setItem(AUTO_KEY, on ? "on" : "off");
    } catch {
      /* storage unavailable */
    }
    set({ autoCheck: on });
  },

  // Asks GitHub for the latest release's update manifest. Nothing about the
  // user or this Mac is sent; the update is verified against DiskPilot's
  // signing key before it is installed.
  checkNow: async (manual = false) => {
    if (get().checking || get().installing) return;
    set({ checking: true, error: null, upToDate: false });
    try {
      const update = await check();
      set({ update, upToDate: manual && !update, lastCheckedMs: Date.now() });
    } catch (e) {
      // Automatic checks fail quietly (offline, GitHub down); manual ones say why.
      if (manual) set({ error: errorMessage(e) });
    } finally {
      set({ checking: false });
    }
  },

  install: async () => {
    const { update } = get();
    if (!update || get().installing) return;
    set({ installing: true, error: null, progress: { done: 0, total: null } });
    try {
      await update.downloadAndInstall((event) => {
        if (event.event === "Started") {
          set({ progress: { done: 0, total: event.data.contentLength ?? null } });
        } else if (event.event === "Progress") {
          const p = get().progress;
          set({
            progress: { done: (p?.done ?? 0) + event.data.chunkLength, total: p?.total ?? null },
          });
        }
      });
      await relaunch();
    } catch (e) {
      set({ error: errorMessage(e), installing: false, progress: null });
    }
  },
}));

let started = false;

/** Checks shortly after launch and then once a day, while automatic checks are on. */
export function startUpdateChecks(): void {
  if (started) return;
  started = true;
  const tick = () => {
    if (useUpdates.getState().autoCheck) void useUpdates.getState().checkNow();
  };
  setTimeout(tick, 10_000);
  setInterval(tick, DAY_MS);
}
