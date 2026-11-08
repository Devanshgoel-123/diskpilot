import { Loader2, RefreshCw, ScanSearch, Square, Zap } from "lucide-react";

import { DrivePicker } from "@/components/shared/DrivePicker";
import { Button } from "@/components/ui/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { isDriveRoot } from "@/lib/drives";
import { formatBytes, formatCount, tildify } from "@/lib/format";
import { samePath } from "@/lib/paths";
import { isWindows } from "@/lib/platform";
import { useApp } from "@/lib/store";

import { FastScanOffer } from "./FastScanDialog";

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
    drives,
    elevated,
    fastScan,
    startScan,
    cancelScan,
  } = useApp();
  if (!desktop) return null;

  const target = tildify(scanRoot ?? env?.homeDir ?? "~", env?.homeDir);

  if (scanning) {
    // A fast scan reads the whole file table before it counts any file.
    const readingTable = fastScan && !progress?.filesScanned;
    return (
      <div className="flex items-center gap-3">
        <div className="text-right">
          <p className="tabular flex items-center justify-end gap-1.5 text-xs font-medium">
            <Loader2 className="size-3 animate-spin text-accent" aria-hidden />
            {readingTable ? (
              "Fast scan"
            ) : (
              <>
                {formatCount(progress?.filesScanned ?? 0)} files ·{" "}
                {formatBytes(progress?.bytesScanned ?? 0)}
              </>
            )}
          </p>
          {readingTable ? (
            <p className="max-w-[260px] truncate text-2xs text-subtle-foreground">
              Reading the drive&apos;s file table…
            </p>
          ) : partial && progress ? (
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
  // Windows: a whole drive can be read from its file table, as administrator.
  const wholeDrive = isWindows() && isDriveRoot(scanRoot, drives);
  const fast = wholeDrive && elevated;

  return (
    <>
      {picker}
      {picker ? <FastScanOffer /> : null}
      {fast ? (
        <span className="flex items-center gap-1 text-2xs text-subtle-foreground">
          <Zap className="size-3" aria-hidden />
          Fast scan
        </span>
      ) : null}
      <Tooltip>
        <TooltipTrigger asChild>
          <Button variant={summary ? "secondary" : "default"} onClick={() => void startScan()}>
            {rescan ? <RefreshCw /> : <ScanSearch />}
            {rescan ? "Rescan" : "Scan"}
          </Button>
        </TooltipTrigger>
        <TooltipContent side="bottom" align="end">
          {fast ? "Fast read-only scan" : "Read-only scan"} of{" "}
          <span className="font-mono">{target}</span>
          {fast ? ", from the drive's file table" : ""}. Nothing is modified.
        </TooltipContent>
      </Tooltip>
    </>
  );
}
