/**
 * TypeScript mirror of `src-tauri/src/model.rs`. Keep both in sync.
 * All sizes are bytes; all timestamps are Unix epoch milliseconds.
 */

export type Category =
  | "cache"
  | "buildArtifacts"
  | "developerTools"
  | "dockerStorage"
  | "applicationData"
  | "logs"
  | "downloads"
  | "userData"
  | "binaryData"
  | "systemFiles"
  | "unknown";

export type Confidence = "high" | "medium" | "low";

export interface Classification {
  category: Category;
  label: string;
  explanation: string;
  ruleId: string;
  confidence: Confidence;
  matchedPath: string | null;
}

export interface DiskInfo {
  mountPoint: string;
  fileSystem: string;
  totalBytes: number;
  availableBytes: number;
  usedBytes: number;
}

export interface AppEnvironment {
  homeDir: string;
  os: string;
  arch: string;
  appVersion: string;
  readOnly: boolean;
}

export interface ScanProgress {
  scanId: string;
  filesScanned: number;
  dirsScanned: number;
  bytesScanned: number;
  currentPath: string;
  elapsedMs: number;
}

export type ScanStatus = "completed" | "cancelled" | "failed";

export interface ScanFinished {
  scanId: string;
  status: ScanStatus;
  error: string | null;
}

export interface CategoryTotal {
  category: Category;
  bytes: number;
  fileCount: number;
}

export interface FileEntry {
  path: string;
  name: string;
  bytes: number;
  logicalBytes: number;
  modifiedMs: number | null;
  classification: Classification;
}

export interface DetectedLocation {
  path: string;
  bytes: number;
  fileCount: number;
  /** Broad area that can contain other detected locations. */
  container: boolean;
  classification: Classification;
}

export interface DirectorySize {
  path: string;
  bytes: number;
  fileCount: number;
  classification: Classification;
}

export interface ScanIssue {
  path: string;
  message: string;
}

export interface ScanSummary {
  id: string;
  root: string;
  startedAtMs: number;
  finishedAtMs: number;
  durationMs: number;
  totalBytes: number;
  fileCount: number;
  dirCount: number;
  symlinkCount: number;
  skippedVolumes: number;
  errorCount: number;
  errorSamples: ScanIssue[];
  categories: CategoryTotal[];
  largestFiles: FileEntry[];
  detectedLocations: DetectedLocation[];
  heavyDirectories: DirectorySize[];
}

export type EntryKind = "directory" | "file" | "symlink" | "other";

export interface DirectoryEntry {
  name: string;
  path: string;
  kind: EntryKind;
  bytes: number | null;
  fileCount: number | null;
  modifiedMs: number | null;
  extension: string | null;
  classification: Classification;
}

export interface DirectoryListing {
  path: string;
  parent: string | null;
  scanRoot: string;
  bytes: number | null;
  entries: DirectoryEntry[];
  omittedCount: number;
  omittedBytes: number;
  classification: Classification;
}

export interface ItemDetails {
  path: string;
  name: string;
  kind: EntryKind;
  bytes: number | null;
  logicalBytes: number | null;
  fileCount: number | null;
  createdMs: number | null;
  modifiedMs: number | null;
  accessedMs: number | null;
  readonly: boolean;
  symlinkTarget: string | null;
  classification: Classification;
}

export interface ScanRecord {
  id: string;
  root: string;
  startedAtMs: number;
  finishedAtMs: number;
  durationMs: number;
  totalBytes: number;
  fileCount: number;
  dirCount: number;
  errorCount: number;
  status: ScanStatus;
}

/** Error shape returned by every Rust command. */
export interface AppError {
  kind: string;
  message: string;
}

// ---- Recommendations & cleanup (src-tauri/src/model.rs) -------------------

export type Tier = "likelySafe" | "reviewFirst" | "keep";
export type CleanupMethod = "trash" | "delete";

export interface Recommendation {
  id: string;
  title: string;
  path: string;
  bytes: number;
  fileCount: number;
  tier: Tier;
  confidence: Confidence;
  classification: Classification;
  detectedBecause: string;
  safeBecause: string;
  risks: string;
  proposedAction: string;
  /** Empty: DiskPilot offers no automated action for this item. */
  methods: CleanupMethod[];
  /** A command the user can run themselves; DiskPilot never runs it. */
  alternative: string | null;
  estimatedReclaimBytes: number;
  lastActivityMs: number | null;
  notes: string[];
  /** "scan", or the id of the developer-tool analyzer that found it. */
  source: string;
}

export interface PlanItem {
  recommendationId: string;
  title: string;
  path: string;
  bytes: number;
  fileCount: number;
  tier: Tier;
  operation: string;
}

export interface PlanRejection {
  recommendationId: string;
  path: string;
  reason: string;
}

export interface CleanupPlan {
  id: string;
  createdAtMs: number;
  method: CleanupMethod;
  items: PlanItem[];
  rejected: PlanRejection[];
  totalBytes: number;
  warnings: string[];
}

export type ItemStatus = "succeeded" | "failed" | "skipped";
export type ActionStatus = "succeeded" | "partial" | "failed";

export interface CleanupItemResult {
  target: string;
  operation: string;
  bytes: number;
  status: ItemStatus;
  error: string | null;
}

export interface CleanupRecord {
  id: string;
  kind: "files" | "docker";
  title: string;
  startedAtMs: number;
  finishedAtMs: number;
  status: ActionStatus;
  bytesFreed: number;
  bytesTrashed: number;
  items: CleanupItemResult[];
}

// ---- Docker (src-tauri/src/docker) ----------------------------------------

export type DockerState = "notInstalled" | "notRunning" | "running";

export interface DockerStatus {
  state: DockerState;
  cliPath: string | null;
  clientVersion: string | null;
  serverVersion: string | null;
  platform: string | null;
  message: string | null;
}

export interface UsageRow {
  totalCount: number;
  active: number;
  sizeBytes: number;
  reclaimableBytes: number;
}

export interface DockerUsage {
  images: UsageRow;
  containers: UsageRow;
  volumes: UsageRow;
  buildCache: UsageRow;
}

export interface DockerImage {
  id: string;
  shortId: string;
  repository: string;
  tag: string;
  createdAtMs: number | null;
  sizeBytes: number;
  sharedBytes: number;
  uniqueBytes: number;
  usedBy: string[];
  dangling: boolean;
}

export interface DockerContainer {
  id: string;
  shortId: string;
  name: string;
  image: string;
  imageId: string | null;
  state: string;
  status: string;
  running: boolean;
  createdAtMs: number | null;
  sizeBytes: number;
  mounts: string[];
  composeProject: string | null;
}

export interface DockerVolume {
  name: string;
  driver: string;
  sizeBytes: number | null;
  links: number | null;
  usedBy: string[];
  composeProject: string | null;
}

export interface BuildCacheEntry {
  id: string;
  cacheType: string;
  description: string;
  sizeBytes: number;
  created: string;
  lastUsed: string;
  inUse: boolean;
  shared: boolean;
}

export interface DockerOverview {
  status: DockerStatus;
  usage: DockerUsage;
  images: DockerImage[];
  containers: DockerContainer[];
  volumes: DockerVolume[];
  buildCache: BuildCacheEntry[];
}

export type DockerAction =
  | { type: "startContainer"; id: string }
  | { type: "stopContainer"; id: string }
  | { type: "removeContainer"; id: string }
  | { type: "removeImage"; id: string }
  | { type: "pruneBuildCache" };

export interface AffectedItem {
  label: string;
  detail: string;
  bytes: number | null;
}

export interface DockerActionPreview {
  action: DockerAction;
  title: string;
  command: string[];
  destructive: boolean;
  affected: AffectedItem[];
  effects: string[];
  warnings: string[];
  blocked: string | null;
  reclaimBytes: number | null;
}

export interface DockerActionResult {
  preview: DockerActionPreview;
  succeeded: boolean;
  output: string;
  error: string | null;
  record: CleanupRecord | null;
}

export type FindingLevel = "info" | "suggestion" | "warning";

export interface DiagnosticFinding {
  level: FindingLevel;
  title: string;
  detail: string;
  bytes: number | null;
}

// ---- Developer tools analysis (src-tauri/src/devtools) ---------------------

export type ItemState = "current" | "stale" | "orphaned" | "superseded" | "info";

export interface ToolItem {
  label: string;
  path: string | null;
  bytes: number;
  detail: string;
  state: ItemState;
  lastUsedMs: number | null;
  /** Present when the item can be cleaned up through Recommendations. */
  recommendationId: string | null;
}

export interface ToolSection {
  title: string;
  description: string;
  items: ToolItem[];
}

export interface ToolReport {
  id: string;
  name: string;
  detected: boolean;
  version: string | null;
  totalBytes: number;
  summary: string;
  sections: ToolSection[];
}

export interface DevToolsReport {
  generatedAtMs: number;
  tools: ToolReport[];
  reclaimableBytes: number;
}

// ---- Assistant (src-tauri/src/assistant) -----------------------------------

export interface AssistantStatus {
  configured: boolean;
  keySource: "keychain" | "environment" | null;
  models: string[];
}

export interface AssistantReply {
  text: string;
  highlighted: string[];
  toolsUsed: string[];
  model: string;
  stopReason: string;
}

export type ChatEntry =
  | { role: "user"; text: string }
  | { role: "assistant"; reply: AssistantReply }
  | { role: "error"; text: string };

/** The installed app a folder belongs to, with its own icon. */
export interface AppBadge {
  name: string;
  /** PNG data URL rendered from the installed app. */
  icon: string;
}

/** One of an app's folders in ~/Library. */
export interface AppDataLocation {
  path: string;
  /** The Library folder it lives in, such as "Caches". */
  kind: string;
  bytes: number;
}

export interface AppFootprint {
  name: string;
  bundleId: string;
  appPath: string;
  /** Null for apps on the read-only system volume. */
  appBytes: number | null;
  dataBytes: number;
  dataModifiedMs: number | null;
  locations: AppDataLocation[];
}

/** Sandbox data named for an app that is no longer installed. */
export interface Leftover {
  name: string;
  path: string;
  kind: string;
  bytes: number;
  modifiedMs: number | null;
}

export interface AppStorageReport {
  apps: AppFootprint[];
  leftovers: Leftover[];
  /** False when no scan covers ~/Library; data sizes are then zero. */
  dataScanned: boolean;
}
