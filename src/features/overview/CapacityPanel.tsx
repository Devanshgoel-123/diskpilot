import { Info } from "lucide-react";

import { Panel } from "@/components/ui/panel";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { formatBytes, formatPercent, tildify } from "@/lib/format";
import type { DiskInfo, ScanSummary } from "@/lib/types";

interface Segment {
  key: string;
  label: string;
  bytes: number;
  className: string;
}

/**
 * Volume capacity as one segmented meter: what the last scan accounts for,
 * the rest of used space, and free space. Values come from statfs + the scan.
 */
export function CapacityPanel({
  disk,
  summary,
  home,
}: {
  disk: DiskInfo;
  summary: ScanSummary | null;
  home: string | undefined;
}) {
  const scanned = summary ? Math.min(summary.totalBytes, disk.usedBytes) : 0;
  const segments: Segment[] = [
    ...(summary
      ? [
          {
            key: "scanned",
            label: `Scanned (${tildify(summary.root, home)})`,
            bytes: scanned,
            className: "bg-accent",
          },
        ]
      : []),
    {
      key: "other",
      label: summary ? "Other used space" : "Used",
      bytes: disk.usedBytes - scanned,
      className: "bg-chart-other",
    },
    { key: "free", label: "Available", bytes: disk.availableBytes, className: "bg-track" },
  ];
  const total = disk.totalBytes;

  return (
    <Panel className="p-4">
      <div className="flex flex-wrap items-end justify-between gap-x-8 gap-y-3">
        <div>
          <p className="text-muted-foreground text-xs">
            Startup disk <span className="text-subtle-foreground font-mono">{disk.mountPoint}</span>
          </p>
          <p className="tabular mt-1 text-2xl font-semibold tracking-tight">
            {formatBytes(disk.usedBytes)}
            <span className="text-muted-foreground ml-1.5 text-sm font-normal">
              used of {formatBytes(total)}
            </span>
          </p>
        </div>
        <div className="text-right">
          <p className="text-muted-foreground flex items-center justify-end gap-1 text-xs">
            Available
            <Tooltip>
              <TooltipTrigger asChild>
                <Info
                  className="text-subtle-foreground size-3"
                  aria-label="About available space"
                />
              </TooltipTrigger>
              <TooltipContent>
                Space free right now. Finder may show more because it also counts purgeable space
                (local snapshots, optimized iCloud files) that macOS can reclaim on demand.
              </TooltipContent>
            </Tooltip>
          </p>
          <p className="tabular mt-1 text-2xl font-semibold tracking-tight">
            {formatBytes(disk.availableBytes)}
          </p>
        </div>
      </div>

      <div
        className="mt-4 flex h-2.5 w-full gap-[2px] overflow-hidden rounded-full"
        role="img"
        aria-label={segments.map((s) => `${s.label}: ${formatBytes(s.bytes)}`).join(", ")}
      >
        {segments.map((s) =>
          s.bytes > 0 ? (
            <Tooltip key={s.key}>
              <TooltipTrigger asChild>
                <div
                  className={`${s.className} h-full first:rounded-l-full last:rounded-r-full`}
                  style={{ width: `${(s.bytes / total) * 100}%`, minWidth: 3 }}
                />
              </TooltipTrigger>
              <TooltipContent>
                <span className="font-medium">{s.label}</span> · {formatBytes(s.bytes)} (
                {formatPercent(s.bytes, total)})
              </TooltipContent>
            </Tooltip>
          ) : null,
        )}
      </div>

      <ul className="mt-3 flex flex-wrap gap-x-5 gap-y-1">
        {segments.map((s) => (
          <li key={s.key} className="text-muted-foreground flex items-center gap-1.5 text-xs">
            <span className={`size-2 rounded-[3px] ${s.className}`} aria-hidden />
            <span className="max-w-[260px] truncate">{s.label}</span>
            <span className="text-foreground tabular">{formatBytes(s.bytes)}</span>
          </li>
        ))}
      </ul>
    </Panel>
  );
}
