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
