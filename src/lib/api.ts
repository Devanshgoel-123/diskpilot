/**
 * Typed wrappers around the Rust commands. The rest of the UI talks to the
 * engine only through this module, so it stays independent of how scanning
 * is implemented.
 */
import type { CleanupActivity } from "@/lib/cleanupStats";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import { isWindows } from "./platform";

import type {
  Signup,
  FileEntry,
  FileKind,
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
  SystemExtra,
  DriveInfo,
  ModelInfo,
  DuplicateProgress,
  DuplicateReport,
  ItemDetails,
  ScanFinished,
  ScanProgress,
  ScanRecord,
  ScanSummary,
  TrashPreview,
  TrashListing,
  TrashProgress,
  CleanupProgress,
  TrashResult,
  TrashSummary,
  UninstallOutcome,
  CompactMethod,
  CompactPreview,
  CompactResult,
  WslOverview,
} from "./types";

export const inDesktopApp = (): boolean => isTauri();

export const api = {
  getEnvironment: () => invoke<AppEnvironment>("get_environment"),
  getSignup: () => invoke<Signup | null>("get_signup"),
  submitSignup: (email: string) => invoke<Signup>("submit_signup", { email }),
  getDiskInfo: () => invoke<DiskInfo>("get_disk_info"),
  /** Windows system areas with the tool that reclaims each; empty on macOS. */
  getSystemExtras: () => invoke<SystemExtra[]>("get_system_extras"),
  /** Drives to scan: every fixed and removable drive on Windows, the home volume on macOS. */
  listDrives: () => invoke<DriveInfo[]>("list_drives"),
  /**
   * `fast` reads the drive's file table directly (Windows, drive roots, when
   * DiskPilot runs as administrator); the engine ignores it otherwise.
   */
  startScan: (root?: string, fast = false) =>
    invoke<string>("start_scan", { root: root ?? null, fast }),
  /** Whether DiskPilot runs as administrator (Windows; always false on macOS). */
  isElevated: () => invoke<boolean>("is_elevated"),
  /** Restarts DiskPilot as administrator after a UAC prompt; this window then closes. */
  relaunchElevated: () => invoke<null>("relaunch_elevated"),
  cancelScan: () => invoke<boolean>("cancel_scan"),
  isScanRunning: () => invoke<boolean>("is_scan_running"),
  getScanSummary: () => invoke<ScanSummary | null>("get_scan_summary"),
  /** The files the last scan listed for one type, largest first. */
  getFileKindFiles: (kind: FileKind) => invoke<FileEntry[]>("get_file_kind_files", { kind }),
  getSavedScanInfo: () => invoke<SavedScanInfo>("get_saved_scan_info"),
  clearSavedScan: () => invoke<null>("clear_saved_scan"),
  listDirectory: (path?: string) =>
    invoke<DirectoryListing>("list_directory", { path: path ?? null }),
  getItemDetails: (path: string) => invoke<ItemDetails>("get_item_details", { path }),
  /** Shows the item in Finder or File Explorer. */
  revealInFinder: (path: string) => invoke<null>("reveal_in_finder", { path }),
  openOwnerApp: (ruleId: string) => invoke<null>("open_owner_app", { ruleId }),
  /** JPEG or PNG bytes of a system thumbnail (Quick Look on macOS); files inside the last scan only. */
  fileThumbnail: (path: string, size: number) =>
    invoke<ArrayBuffer>("file_thumbnail", { path, size }),
  /** Opens a listed file in its default app; apps and scripts are refused. */
  openListedFile: (path: string) => invoke<null>("open_listed_file", { path }),
  getAppStorage: () => invoke<AppStorageReport>("get_app_storage"),
  getAppBadges: (paths: string[]) => invoke<Record<string, AppBadge>>("get_app_badges", { paths }),
  /**
   * Windows: starts the program's own uninstaller (`uninstaller`), or opens
   * Settings > Apps when it has none DiskPilot can run (`settings`). `id` is
   * the app's `bundleId`. DiskPilot never deletes the program's folder itself.
   */
  uninstallApp: (id: string) => invoke<UninstallOutcome>("uninstall_app", { id }),

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
  /** macOS only: Windows has no Full Disk Access, so nothing is asked there. */
  getFullDiskAccess: (): Promise<"granted" | "denied" | "unknown"> =>
    isWindows()
      ? Promise.resolve("unknown")
      : invoke<"granted" | "denied" | "unknown">("get_full_disk_access"),
  /** macOS only; does nothing on Windows. */
  openFullDiskAccessSettings: (): Promise<null> =>
    isWindows() ? Promise.resolve(null) : invoke<null>("open_full_disk_access_settings"),

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

/** The last scan's numbers changed because DiskPilot removed something. */
export function onScanUpdated(cb: () => void): Promise<UnlistenFn> {
  return listen<null>("scan://updated", () => cb());
}

export function onCleanupProgress(cb: (p: CleanupProgress) => void): Promise<UnlistenFn> {
  return listen<CleanupProgress>("cleanup://progress", (e) => cb(e.payload));
}

export function onTrashProgress(cb: (p: TrashProgress) => void): Promise<UnlistenFn> {
  return listen<TrashProgress>("trash://progress", (e) => cb(e.payload));
}

export function onTrashListing(cb: (p: TrashListing) => void): Promise<UnlistenFn> {
  return listen<TrashListing>("trash://listing", (e) => cb(e.payload));
}

export function onDuplicatesProgress(cb: (p: DuplicateProgress) => void): Promise<UnlistenFn> {
  return listen<DuplicateProgress>("duplicates://progress", (e) => cb(e.payload));
}

/** Menu bar (tray) shortcuts: a page id, or "trash" for the Empty Trash window. */
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
