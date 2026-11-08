import { Loader2, RefreshCw, ScanSearch, Square } from "lucide-react";

import { DrivePicker } from "@/components/shared/DrivePicker";
import { Button } from "@/components/ui/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { formatBytes, formatCount, tildify } from "@/lib/format";
import { samePath } from "@/lib/paths";
import { isWindows } from "@/lib/platform";
import { useApp } from "@/lib/store";

/** Global scan button / live progress, shown in every page header. */
export function ScanControl() {
  const {
    desktop,
    scanning,
    partial,
    progress,
    summary,
    scanRoot,
    env,
    page,
    startScan,
    cancelScan,
  } = useApp();
  if (!desktop) return null;

  const target = tildify(scanRoot ?? env?.homeDir ?? "~", env?.homeDir);

  if (scanning) {
    return (
      <div className="flex items-center gap-3">
        <div className="text-right">
          <p className="tabular flex items-center justify-end gap-1.5 text-xs font-medium">
            <Loader2 className="size-3 animate-spin text-accent" aria-hidden />
            {formatCount(progress?.filesScanned ?? 0)} files ·{" "}
            {formatBytes(progress?.bytesScanned ?? 0)}
          </p>
          {partial && progress ? (
            <p className="tabular max-w-[260px] truncate text-2xs text-subtle-foreground">
              Measuring app containers: {formatCount(progress.containersDone)} of{" "}
              {formatCount(progress.containersTotal)}
            </p>
          ) : (
            <p className="max-w-[260px] truncate font-mono text-2xs text-subtle-foreground">
              {progress ? tildify(progress.currentPath, env?.homeDir) : "Starting…"}
            </p>
          )}
        </div>
        <Button variant="secondary" onClick={() => void cancelScan()}>
          <Square />
          Stop
        </Button>
      </div>
    );
  }

  // Windows: choose the user folder or a drive where scans are started.
  // After choosing another drive the button scans it rather than rescanning.
  const rescan =
    !!summary && (!isWindows() || samePath(summary.root, scanRoot ?? env?.homeDir ?? ""));
  const picker = page === "overview" || page === "explorer" ? <DrivePicker /> : null;

  return (
    <>
      {picker}
      <Tooltip>
        <TooltipTrigger asChild>
          <Button variant={summary ? "secondary" : "default"} onClick={() => void startScan()}>
            {rescan ? <RefreshCw /> : <ScanSearch />}
            {rescan ? "Rescan" : "Scan"}
          </Button>
        </TooltipTrigger>
        <TooltipContent side="bottom" align="end">
          Read-only scan of <span className="font-mono">{target}</span>. Nothing is modified.
        </TooltipContent>
      </Tooltip>
    </>
  );
}
