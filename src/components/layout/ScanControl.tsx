import { Loader2, RefreshCw, ScanSearch, Square } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { formatBytes, formatCount, tildify } from "@/lib/format";
import { useApp } from "@/lib/store";

/** Global scan button / live progress, shown in every page header. */
export function ScanControl() {
  const { desktop, scanning, progress, summary, scanRoot, env, startScan, cancelScan } = useApp();
  if (!desktop) return null;

  const target = tildify(scanRoot ?? env?.homeDir ?? "~", env?.homeDir);

  if (scanning) {
    return (
      <div className="flex items-center gap-3">
        <div className="text-right">
          <p className="tabular flex items-center justify-end gap-1.5 text-xs font-medium">
            <Loader2 className="text-accent size-3 animate-spin" aria-hidden />
            {formatCount(progress?.filesScanned ?? 0)} files ·{" "}
            {formatBytes(progress?.bytesScanned ?? 0)}
          </p>
          <p className="text-2xs text-subtle-foreground max-w-[260px] truncate font-mono">
            {progress ? tildify(progress.currentPath, env?.homeDir) : "Starting…"}
          </p>
        </div>
        <Button variant="secondary" onClick={() => void cancelScan()}>
          <Square />
          Stop
        </Button>
      </div>
    );
  }

  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button onClick={() => void startScan()}>
          {summary ? <RefreshCw /> : <ScanSearch />}
          {summary ? "Rescan" : "Scan"}
        </Button>
      </TooltipTrigger>
      <TooltipContent side="bottom" align="end">
        Read-only scan of <span className="font-mono">{target}</span>. Nothing is modified.
      </TooltipContent>
    </Tooltip>
  );
}
