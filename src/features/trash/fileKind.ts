import {
  AppWindow,
  File,
  FileArchive,
  FileAudio,
  FileCode,
  FileImage,
  FileText,
  FileVideo,
  Folder,
  type LucideIcon,
  Package,
} from "lucide-react";

import type { TrashEntry } from "@/lib/types";

const BY_EXTENSION: [LucideIcon, string[]][] = [
  [Package, ["dmg", "pkg", "mpkg", "iso", "img"]],
  [FileArchive, ["zip", "gz", "tgz", "bz2", "xz", "7z", "rar", "tar", "zst"]],
  [FileImage, ["png", "jpg", "jpeg", "heic", "gif", "webp", "tiff", "psd", "svg", "raw", "dng"]],
  [FileVideo, ["mov", "mp4", "m4v", "mkv", "avi", "webm"]],
  [FileAudio, ["mp3", "m4a", "wav", "aiff", "flac", "aac"]],
  [
    FileCode,
    ["js", "ts", "tsx", "jsx", "rs", "py", "go", "swift", "java", "json", "yml", "yaml", "sh"],
  ],
  [FileText, ["txt", "md", "pdf", "doc", "docx", "pages", "rtf", "csv", "xlsx", "key", "pptx"]],
];

/** An icon for a Trash entry, from its name only. Nothing is read from disk. */
export function entryIcon(entry: TrashEntry): LucideIcon {
  const ext = entry.name.includes(".") ? (entry.name.split(".").pop() ?? "").toLowerCase() : "";
  if (entry.isDir) return ext === "app" ? AppWindow : Folder;
  for (const [icon, exts] of BY_EXTENSION) if (exts.includes(ext)) return icon;
  return File;
}
