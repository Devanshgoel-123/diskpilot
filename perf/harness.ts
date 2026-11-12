// Runs the real app against synthetic data (perf/fixture.ts) with the engine
// mocked, so the UI can be measured in a browser: `pnpm perf:ui`.
import { mockIPC } from "@tauri-apps/api/mocks";

import { HOME, makeFixture } from "./fixture";

const t0 = performance.now();
const fx = makeFixture();
// File types carry their totals and listed count, and their files come on
// request (get_file_kind_files). The files stay in the summary too, so CI can
// run this harness against a base branch from before that change.
const windowSummary = {
  ...fx.summary,
  fileKinds: fx.summary.fileKinds.map((g) => ({ ...g, listed: g.files.length })),
};
(window as unknown as { isTauri: boolean }).isTauri = true;

// One small JPEG for every thumbnail, after a fixed delay standing in for
// Quick Look, so preview timings measure the UI and not the machine.
const THUMB_DELAY_MS = 15;
let thumb: Promise<ArrayBuffer> | null = null;
function thumbnail(): Promise<ArrayBuffer> {
  thumb ??= new Promise((resolve) => {
    const c = document.createElement("canvas");
    c.width = c.height = 256;
    const g = c.getContext("2d")!;
    g.fillStyle = "#6a8caf";
    g.fillRect(0, 0, 256, 256);
    c.toBlob((b) => void b!.arrayBuffer().then(resolve), "image/jpeg", 0.8);
  });
  return thumb.then((b) => new Promise((r) => setTimeout(() => r(b.slice(0)), THUMB_DELAY_MS)));
}

mockIPC(
  (cmd, args) => {
    const a = (args ?? {}) as Record<string, unknown>;
    switch (cmd) {
      case "get_environment":
        return { homeDir: HOME, os: "macos", arch: "aarch64", appVersion: "0.0.0", readOnly: true };
      case "get_disk_info":
        return fx.disk;
      case "get_scan_summary":
        return windowSummary;
      case "get_file_kind_files":
        return fx.summary.fileKinds.find((g) => g.kind === a.kind)?.files ?? [];
      case "is_scan_running":
        return false;
      case "get_saved_scan_info":
        return { fileBytes: 1_600_000 };
      case "get_scan_history":
        return [];
      case "list_directory":
        return fx.listing((a.path as string | null) ?? HOME);
      case "get_recommendations":
        return fx.recommendations;
      case "get_cleanup_history":
      case "get_cleanup_activity":
        return [];
      case "file_thumbnail":
        return thumbnail();
      case "get_full_disk_access":
        return "granted";
      case "get_trash_summary":
        return { access: "granted", items: 0, totalBytes: 0, totalFiles: 0 };
      case "get_app_badges":
        return {};
      default:
        return null;
    }
  },
  { shouldMockEvents: true },
);

await import("../src/main");
const { useApp } = await import("../src/lib/store");
Object.assign(window, { __perf: { useApp, t0 } });
