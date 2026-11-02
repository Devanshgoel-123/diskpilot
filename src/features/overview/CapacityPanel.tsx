import { Info } from "lucide-react";

import { Panel } from "@/components/ui/panel";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { CATEGORY_META } from "@/lib/categories";
import { formatBytes, formatPercent, tildify } from "@/lib/format";
import type { DiskInfo, ScanSummary } from "@/lib/types";

interface Segment {
  key: string;
  label: string;
  bytes: number;
  color: string;
}

/**
 * Volume capacity as one segmented meter: the scanned folder split by
 * category, the rest of used space, and free space. Values come from statfs
 * and the scan; nothing is estimated.
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
  // Scale categories down only in the rare case the scan exceeds used space
  // (APFS clones are counted per copy).
  const scale = summary && summary.totalBytes > 0 ? scanned / summary.totalBytes : 0;
  const categorySegments: Segment[] = summary
    ? [...summary.categories]
        .sort((a, b) => b.bytes - a.bytes)
        .map((c) => ({
          key: c.category,
          label: CATEGORY_META[c.category].label,
          bytes: c.bytes * scale,
          color: CATEGORY_META[c.category].color,
        }))
    : [];
  const segments: Segment[] = [
    ...categorySegments,
    {
      key: "other",
      label: summary ? "Other used space" : "Used",
      bytes: disk.usedBytes - scanned,
      color: "var(--chart-other)",
    },
    { key: "free", label: "Available", bytes: disk.availableBytes, color: "var(--track)" },
  ];
  const total = disk.totalBytes;
  // Legend: categories that are visible at this scale, plus the two totals.
  const legend = segments.filter(
    (s) => s.key === "other" || s.key === "free" || s.bytes / total >= 0.015,
  );

  return (
    <Panel className="p-4">
      <div className="flex flex-wrap items-end justify-between gap-x-8 gap-y-3">
        <div>
          <p className="text-xs text-muted-foreground">
            Startup disk <span className="font-mono text-subtle-foreground">{disk.mountPoint}</span>
          </p>
          <p className="tabular mt-1 text-2xl font-semibold tracking-tight">
            {formatBytes(disk.usedBytes)}
            <span className="ml-1.5 text-sm font-normal text-muted-foreground">
              used of {formatBytes(total)}
            </span>
          </p>
        </div>
        <div className="text-right">
          <p className="flex items-center justify-end gap-1 text-xs text-muted-foreground">
            Available
            <Tooltip>
              <TooltipTrigger asChild>
                <Info
                  className="size-3 text-subtle-foreground"
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
        className="mt-4 flex h-3 w-full gap-[2px] overflow-hidden rounded-full"
        role="img"
        aria-label={segments.map((s) => `${s.label}: ${formatBytes(s.bytes)}`).join(", ")}
      >
        {segments.map((s) =>
          s.bytes > 0 ? (
            <Tooltip key={s.key}>
              <TooltipTrigger asChild>
                <div
                  className="h-full first:rounded-l-full last:rounded-r-full"
                  style={{
                    width: `${(s.bytes / total) * 100}%`,
                    minWidth: 2,
                    background: s.color,
                  }}
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

      <ul className="mt-3 flex flex-wrap items-center gap-x-4 gap-y-1">
        {summary ? (
          <li className="text-xs text-muted-foreground">
            Scanned <span className="font-mono">{tildify(summary.root, home)}</span>{" "}
            <span className="tabular text-foreground">{formatBytes(summary.totalBytes)}</span>
            <span className="ml-3 text-border-strong">|</span>
          </li>
        ) : null}
        {legend.map((s) => (
          <li key={s.key} className="flex items-center gap-1.5 text-xs text-muted-foreground">
            <span className="size-2 rounded-[3px]" style={{ background: s.color }} aria-hidden />
            <span className="max-w-[260px] truncate">{s.label}</span>
            <span className="tabular text-foreground">{formatBytes(s.bytes)}</span>
          </li>
        ))}
      </ul>
    </Panel>
  );
}
