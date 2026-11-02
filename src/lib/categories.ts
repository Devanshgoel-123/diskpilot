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
}

/**
 * Category identity is carried by icon + label, never by color alone:
 * eleven categories are more than any colorblind-safe palette can separate.
 */
export const CATEGORY_META: Record<Category, CategoryMeta> = {
  cache: {
    label: "Cache",
    description: "Data apps and tools can regenerate or re-download.",
    icon: Archive,
  },
  buildArtifacts: {
    label: "Build artifacts",
    description: "Output of compilers and bundlers, rebuilt on the next build.",
    icon: Hammer,
  },
  developerTools: {
    label: "Developer tools",
    description: "Dependencies, toolchains, SDKs, simulators and repositories.",
    icon: Cpu,
  },
  dockerStorage: {
    label: "Docker storage",
    description: "Container VM disks holding images, containers and volumes.",
    icon: Box,
  },
  applicationData: {
    label: "Application data",
    description: "Data apps need to run: databases, settings, downloaded content.",
    icon: AppWindow,
  },
  logs: {
    label: "Logs",
    description: "Diagnostic logs from apps and the system.",
    icon: FileText,
  },
  downloads: {
    label: "Downloads",
    description: "Everything in your Downloads folder.",
    icon: Download,
  },
  userData: {
    label: "User data",
    description: "Your documents, media, projects and other personal files.",
    icon: User,
  },
  binaryData: {
    label: "Blob / binary data",
    description: "Disk images, archives, datasets and model weights.",
    icon: Package,
  },
  systemFiles: {
    label: "System files",
    description: "Files that belong to macOS. Never modified by DiskPilot.",
    icon: HardDrive,
  },
  unknown: {
    label: "Unknown",
    description: "No rule matched. Unknown does not mean disposable.",
    icon: FileQuestion,
  },
};

export const CONFIDENCE_LABEL: Record<Confidence, string> = {
  high: "High confidence",
  medium: "Medium confidence",
  low: "Low confidence",
};

export const ALL_CATEGORIES = Object.keys(CATEGORY_META) as Category[];
