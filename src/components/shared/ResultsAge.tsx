import { History } from "lucide-react";

import { Button } from "@/components/ui/button";
import { formatDate, formatRelative, isOlderThan } from "@/lib/format";
import { useApp } from "@/lib/store";

const DAY_MS = 86_400_000;

/**
 * Shown when the results on screen come from a scan more than a day old
 * (usually one saved from an earlier launch). Sizes may have changed since;
 * cleanup re-measures every item before it is previewed.
 */
export function ResultsAge() {
  const summary = useApp((s) => s.summary);
  const scanning = useApp((s) => s.scanning);
  const startScan = useApp((s) => s.startScan);
  if (!summary || scanning || !isOlderThan(summary.finishedAtMs, DAY_MS)) return null;

  return (
    <div
      role="status"
      className="flex items-center justify-between gap-4 rounded-[10px] border border-border bg-panel-raised px-3 py-2"
    >
      <p className="flex min-w-0 items-center gap-2 text-xs text-muted-foreground">
        <History className="size-3.5 shrink-0" aria-hidden />
        <span>
          <span className="text-foreground" title={formatDate(summary.finishedAtMs)}>
            Results from {formatRelative(summary.finishedAtMs)}.
          </span>{" "}
          Files may have changed since. Cleanup measures each item again before you confirm.
        </span>
      </p>
      <Button size="sm" variant="secondary" onClick={() => void startScan()}>
        Rescan
      </Button>
    </div>
  );
}
