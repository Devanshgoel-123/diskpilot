import {
  AppWindow,
  Archive,
  Box,
  Cpu,
  Download,
  FileQuestion,
  FileText,
  Hammer,
  HardDrive,
  Package,
  User,
  type LucideIcon,
} from "lucide-react";

import type { Category, Confidence } from "./types";

export interface CategoryMeta {
  label: string;
  description: string;
  icon: LucideIcon;
  /** CSS color (a theme token) for marks and tints. */
  color: string;
}

/**
 * Category identity is carried by icon + label, never by color alone. Eight
 * categories get validated hues; binary data, system files and unknown use
 * neutrals, since eleven hues cannot all stay distinguishable.
 */
export const CATEGORY_META: Record<Category, CategoryMeta> = {
  cache: {
    label: "Cache",
    description: "Data apps and tools can regenerate or re-download.",
    icon: Archive,
    color: "var(--cat-cache)",
  },
  buildArtifacts: {
    label: "Build artifacts",
    description: "Output of compilers and bundlers, rebuilt on the next build.",
    icon: Hammer,
    color: "var(--cat-build-artifacts)",
  },
  developerTools: {
    label: "Developer tools",
    description: "Dependencies, toolchains, SDKs, simulators and repositories.",
    icon: Cpu,
    color: "var(--cat-developer-tools)",
  },
  dockerStorage: {
    label: "Docker storage",
    description: "Container VM disks holding images, containers and volumes.",
    icon: Box,
    color: "var(--cat-docker-storage)",
  },
  applicationData: {
    label: "Application data",
    description: "Data apps need to run: databases, settings, downloaded content.",
    icon: AppWindow,
    color: "var(--cat-application-data)",
  },
  logs: {
    label: "Logs",
    description: "Diagnostic logs from apps and the system.",
    icon: FileText,
    color: "var(--cat-logs)",
  },
  downloads: {
    label: "Downloads",
    description: "Everything in your Downloads folder.",
    icon: Download,
    color: "var(--cat-downloads)",
  },
  userData: {
    label: "User data",
    description: "Your documents, media, projects and other personal files.",
    icon: User,
    color: "var(--cat-user-data)",
  },
  binaryData: {
    label: "Blob / binary data",
    description: "Disk images, archives, datasets and model weights.",
    icon: Package,
    color: "var(--cat-binary-data)",
  },
  systemFiles: {
    label: "System files",
    description: "Files that belong to macOS. Never modified by DiskPilot.",
    icon: HardDrive,
    color: "var(--cat-system-files)",
  },
  unknown: {
    label: "Unknown",
    description: "No rule matched. Unknown does not mean disposable.",
    icon: FileQuestion,
    color: "var(--cat-unknown)",
  },
};

export const CONFIDENCE_LABEL: Record<Confidence, string> = {
  high: "High confidence",
  medium: "Medium confidence",
  low: "Low confidence",
};

export const ALL_CATEGORIES = Object.keys(CATEGORY_META) as Category[];
