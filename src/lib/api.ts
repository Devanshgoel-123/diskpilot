/**
 * Typed wrappers around the Rust commands. The rest of the UI talks to the
 * engine only through this module, so it stays independent of how scanning
 * is implemented.
 */
import type { CleanupActivity } from "@/lib/cleanupStats";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import type {
  AppBadge,
  AppStorageReport,
  AppEnvironment,
  AssistantProvider,
  AssistantReply,
  AssistantStatus,
  CleanupMethod,
  CleanupPlan,
  CleanupRecord,
  DevToolsReport,
  DiagnosticFinding,
  DockerAction,
  DockerActionPreview,
  DockerActionResult,
  Schedule,
  DockerOverview,
  Recommendation,
  SavedScanInfo,
  SecretsReport,
  DirectoryListing,
  DiskInfo,
  ModelInfo,
  DuplicateProgress,
  DuplicateReport,
  ItemDetails,
  ScanFinished,
  ScanProgress,
  ScanRecord,
  ScanSummary,
  TrashPreview,
  TrashProgress,
  TrashResult,
  TrashSummary,
  CompactMethod,
  CompactPreview,
  CompactResult,
  WslOverview,
} from "./types";

export const inDesktopApp = (): boolean => isTauri();

export const api = {
  getEnvironment: () => invoke<AppEnvironment>("get_environment"),
  getDiskInfo: () => invoke<DiskInfo>("get_disk_info"),
  startScan: (root?: string) => invoke<string>("start_scan", { root: root ?? null }),
  cancelScan: () => invoke<boolean>("cancel_scan"),
  isScanRunning: () => invoke<boolean>("is_scan_running"),
  getScanSummary: () => invoke<ScanSummary | null>("get_scan_summary"),
  getSavedScanInfo: () => invoke<SavedScanInfo>("get_saved_scan_info"),
  clearSavedScan: () => invoke<null>("clear_saved_scan"),
  listDirectory: (path?: string) =>
    invoke<DirectoryListing>("list_directory", { path: path ?? null }),
  getItemDetails: (path: string) => invoke<ItemDetails>("get_item_details", { path }),
  revealInFinder: (path: string) => invoke<null>("reveal_in_finder", { path }),
  openOwnerApp: (ruleId: string) => invoke<null>("open_owner_app", { ruleId }),
  /** PNG bytes of a Quick Look thumbnail; files inside the last scan only. */
  fileThumbnail: (path: string, size: number) =>
    invoke<ArrayBuffer>("file_thumbnail", { path, size }),
  getAppStorage: () => invoke<AppStorageReport>("get_app_storage"),
  getAppBadges: (paths: string[]) => invoke<Record<string, AppBadge>>("get_app_badges", { paths }),

  getScanHistory: (limit?: number) =>
    invoke<ScanRecord[]>("get_scan_history", { limit: limit ?? null }),

  getRecommendations: () => invoke<Recommendation[]>("get_recommendations"),
  createCleanupPlan: (ids: string[], method: CleanupMethod) =>
    invoke<CleanupPlan>("create_cleanup_plan", { ids, method }),
  discardCleanupPlan: (planId: string) => invoke<null>("discard_cleanup_plan", { planId }),
  executeCleanupPlan: (planId: string) => invoke<CleanupRecord>("execute_cleanup_plan", { planId }),
  getCleanupActivity: () => invoke<CleanupActivity[]>("get_cleanup_activity"),
  getCleanupHistory: (limit?: number) =>
    invoke<CleanupRecord[]>("get_cleanup_history", { limit: limit ?? null }),

  getTrashSummary: () => invoke<TrashSummary>("get_trash_summary"),
  previewTrash: () => invoke<TrashPreview>("preview_trash"),
  discardTrashPreview: (previewId: string) => invoke<null>("discard_trash_preview", { previewId }),
  emptyTrash: (previewId: string) => invoke<TrashResult>("empty_trash", { previewId }),
  cancelEmptyTrash: () => invoke<boolean>("cancel_empty_trash"),
  getFullDiskAccess: () => invoke<"granted" | "denied" | "unknown">("get_full_disk_access"),
  openFullDiskAccessSettings: () => invoke<null>("open_full_disk_access_settings"),

  checkAiTranscripts: () => invoke<SecretsReport>("check_ai_transcripts"),
  findDuplicates: (minBytes: number) => invoke<DuplicateReport>("find_duplicates", { minBytes }),
  cancelFindDuplicates: () => invoke<boolean>("cancel_find_duplicates"),
  getDuplicates: () => invoke<DuplicateReport | null>("get_duplicates"),
  planDuplicateCleanup: (paths: string[]) =>
    invoke<CleanupPlan>("plan_duplicate_cleanup", { paths }),
  planFileKindCleanup: (paths: string[]) =>
    invoke<CleanupPlan>("plan_file_kind_cleanup", { paths }),

  analyzeDevTools: () => invoke<DevToolsReport>("analyze_dev_tools"),
  getDevToolsReport: () => invoke<DevToolsReport | null>("get_dev_tools_report"),

  assistantStatus: () => invoke<AssistantStatus>("assistant_status"),
  assistantSetKey: (provider: AssistantProvider, key: string) =>
    invoke<AssistantStatus>("assistant_set_key", { provider, key }),
  assistantClearKey: (provider: AssistantProvider) =>
    invoke<AssistantStatus>("assistant_clear_key", { provider }),
  assistantModels: (provider: AssistantProvider) =>
    invoke<ModelInfo[]>("assistant_models", { provider }),
  assistantAsk: (question: string, provider: AssistantProvider, model: string) =>
    invoke<AssistantReply>("assistant_ask", { question, provider, model }),
  assistantReset: () => invoke<null>("assistant_reset"),

  getSchedule: () => invoke<Schedule>("get_schedule"),
  setSchedule: (schedule: Schedule) => invoke<Schedule>("set_schedule", { schedule }),
  sendTestReminder: () => invoke<null>("send_test_reminder"),

  dockerOverview: () => invoke<DockerOverview>("docker_overview"),
  dockerPreview: (action: DockerAction) =>
    invoke<DockerActionPreview>("docker_preview", { action }),
  /** Runs the reviewed preview `previewId`; `typed` is the name typed to confirm a volume delete. */
  dockerExecute: (previewId: string, typed: string) =>
    invoke<DockerActionResult>("docker_execute", { previewId, typed }),
  dockerInspect: (kind: "container" | "image" | "volume", id: string) =>
    invoke<string>("docker_inspect", { kind, id }),
  dockerDiagnostics: () => invoke<DiagnosticFinding[]>("docker_diagnostics"),

  /** WSL 2 and Docker Desktop virtual disks (Windows; `supported` is false on macOS). */
  wslDisks: () => invoke<WslOverview>("wsl_disks"),
  wslCompactPreview: (diskId: string, method: CompactMethod) =>
    invoke<CompactPreview>("wsl_compact_preview", { diskId, method }),
  /** Runs the reviewed preview `previewId`; `typed` must equal its `confirmName`. */
  wslCompactExecute: (previewId: string, typed: string) =>
    invoke<CompactResult>("wsl_compact_execute", { previewId, typed }),
};

export function onScanProgress(cb: (p: ScanProgress) => void): Promise<UnlistenFn> {
  return listen<ScanProgress>("scan://progress", (e) => cb(e.payload));
}

/** Results without app containers, which are still being measured. */
export function onScanPartial(cb: (scanId: string) => void): Promise<UnlistenFn> {
  return listen<string>("scan://partial", (e) => cb(e.payload));
}

export function onScanFinished(cb: (p: ScanFinished) => void): Promise<UnlistenFn> {
  return listen<ScanFinished>("scan://finished", (e) => cb(e.payload));
}

export function onTrashProgress(cb: (p: TrashProgress) => void): Promise<UnlistenFn> {
  return listen<TrashProgress>("trash://progress", (e) => cb(e.payload));
}

export function onDuplicatesProgress(cb: (p: DuplicateProgress) => void): Promise<UnlistenFn> {
  return listen<DuplicateProgress>("duplicates://progress", (e) => cb(e.payload));
}

/** Menu bar shortcuts: a page id, or "trash" for the Empty Trash window. */
export function onTrayNavigate(cb: (target: string) => void): Promise<UnlistenFn> {
  return listen<string>("tray://navigate", (e) => cb(e.payload));
}

export function errorMessage(err: unknown): string {
  if (typeof err === "string") return err;
  if (err && typeof err === "object" && "message" in err) {
    return String((err as { message: unknown }).message);
  }
  return "Something went wrong";
}
