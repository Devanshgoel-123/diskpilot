/**
 * Typed wrappers around the Rust commands. The rest of the UI talks to the
 * engine only through this module, so it stays independent of how scanning
 * is implemented.
 */
import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import type {
  AppEnvironment,
  DirectoryListing,
  DiskInfo,
  ItemDetails,
  ScanFinished,
  ScanProgress,
  ScanRecord,
  ScanSummary,
} from "./types";

export const inDesktopApp = (): boolean => isTauri();

export const api = {
  getEnvironment: () => invoke<AppEnvironment>("get_environment"),
  getDiskInfo: () => invoke<DiskInfo>("get_disk_info"),
  startScan: (root?: string) => invoke<string>("start_scan", { root: root ?? null }),
  cancelScan: () => invoke<boolean>("cancel_scan"),
  isScanRunning: () => invoke<boolean>("is_scan_running"),
  getScanSummary: () => invoke<ScanSummary | null>("get_scan_summary"),
  listDirectory: (path?: string) =>
    invoke<DirectoryListing>("list_directory", { path: path ?? null }),
  getItemDetails: (path: string) => invoke<ItemDetails>("get_item_details", { path }),
  revealInFinder: (path: string) => invoke<null>("reveal_in_finder", { path }),
  getScanHistory: (limit?: number) =>
    invoke<ScanRecord[]>("get_scan_history", { limit: limit ?? null }),
};

export function onScanProgress(cb: (p: ScanProgress) => void): Promise<UnlistenFn> {
  return listen<ScanProgress>("scan://progress", (e) => cb(e.payload));
}

export function onScanFinished(cb: (p: ScanFinished) => void): Promise<UnlistenFn> {
  return listen<ScanFinished>("scan://finished", (e) => cb(e.payload));
}

export function errorMessage(err: unknown): string {
  if (typeof err === "string") return err;
  if (err && typeof err === "object" && "message" in err) {
    return String((err as { message: unknown }).message);
  }
  return "Something went wrong";
}
