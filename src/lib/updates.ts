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
  /** Why the last check the person asked for failed. */
  error: string | null;
  /** Why installing failed, in plain words. */
  installError: string | null;
  /** The update failed verification: retrying cannot help, download it instead. */
  unverified: boolean;
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
  installError: null,
  unverified: false,
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
      const same = update && get().update?.version === update.version;
      set({
        update,
        upToDate: manual && !update,
        lastCheckedMs: Date.now(),
        // A different version gets a fresh chance to install.
        ...(same ? {} : { installError: null, unverified: false }),
      });
    } catch (e) {
      // Automatic checks fail quietly (offline, GitHub down); manual ones say why.
      if (manual) set({ error: errorMessage(e) });
    } finally {
      set({ checking: false });
    }
  },

  install: async () => {
    const { update } = get();
    if (!update || get().installing || get().unverified) return;
    set({ installing: true, installError: null, progress: { done: 0, total: null } });
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
      const { message, unverified } = installFailure(errorMessage(e));
      set({ installError: message, unverified, installing: false, progress: null });
    }
  },
}));

/** What went wrong installing an update, for people rather than logs. */
export function installFailure(raw: string): { message: string; unverified: boolean } {
  if (/signature|minisign|SignedVersion/i.test(raw)) {
    return {
      message:
        "This update could not be verified, so it was not installed and nothing changed. Download the latest version from GitHub instead.",
      unverified: true,
    };
  }
  if (/read-only file system|os error 30|TempDirNotOnSameMountPoint/i.test(raw)) {
    return {
      message:
        "DiskPilot cannot update itself where it is. Quit it, drag it into Applications, and open it from there.",
      unverified: false,
    };
  }
  if (/error sending request|dns|timed out|connection|network|offline/i.test(raw)) {
    return {
      message: "Could not download the update. Check your connection and try again.",
      unverified: false,
    };
  }
  return { message: `The update could not be installed: ${raw}`, unverified: false };
}

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
