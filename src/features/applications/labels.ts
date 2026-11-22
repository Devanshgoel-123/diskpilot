import { isWindows } from "@/lib/platform";
import type { AppFootprint } from "@/lib/types";

/** Windows AppData kinds, as people know the folders from File Explorer. */
const WINDOWS_KIND_LABEL: Record<string, string> = {
  Roaming: "AppData\\Roaming",
  Local: "AppData\\Local",
  LocalLow: "AppData\\LocalLow",
  Packages: "Store app data",
  ProgramData: "ProgramData",
};

/** A readable name for where app data lives; macOS Library kinds read well as they are. */
export function dataKindLabel(kind: string): string {
  return (isWindows() && WINDOWS_KIND_LABEL[kind]) || kind;
}

/** The line under an app's name: its bundle id on macOS, publisher and version on Windows. */
export function appSubtitle(app: AppFootprint): string {
  if (!isWindows()) return app.bundleId;
  return [app.publisher, app.version].filter(Boolean).join(" · ");
}
