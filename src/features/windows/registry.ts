import type * as React from "react";

import { SystemExtrasSection } from "./extras/SystemExtras";
import { VirtualDisksSection, VirtualDisksSummary } from "./wsl/VirtualDisks";

/**
 * Windows-only sections, by page. Each Windows feature adds its component to
 * the list for the page it belongs on; `<WindowsSections page=… />` renders
 * them in order at a fixed place, and only on Windows. A component returns
 * `null` when it has nothing to show (for example before its first check).
 *
 * - `overview`: below the drive list, above the scan results. For WSL and
 *   Docker virtual disks, system extras (Windows.old, the update cache, the
 *   hibernation file, WinSxS) and OneDrive notes.
 * - `developer`: below the tool cards. For WSL distributions.
 * - `applications`: below the app list. For uninstalling apps.
 */
export const WINDOWS_SECTIONS: Record<
  "overview" | "developer" | "applications",
  React.ComponentType[]
> = {
  overview: [VirtualDisksSummary, SystemExtrasSection],
  developer: [VirtualDisksSection],
  applications: [],
};

export type WindowsSectionPage = keyof typeof WINDOWS_SECTIONS;
