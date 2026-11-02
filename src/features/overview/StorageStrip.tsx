import { Info } from "lucide-react";
import { useMemo } from "react";

import { Button } from "@/components/ui/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { recoverableByCategory } from "@/lib/analysis";
import { CATEGORY_META } from "@/lib/categories";
import { formatBytes, formatPercent } from "@/lib/format";
import type { DiskInfo, Recommendation, ScanSummary } from "@/lib/types";

interface Segment {
  key: string;
  label: string;
  bytes: number;
  color: string;
  /** Bytes in this segment covered by Likely safe recommendations. */
  recoverable: number;
}

const HATCH =
  "repeating-linear-gradient(135deg, rgb(255 255 255 / 0.62) 0 1.5px, transparent 1.5px 4.5px)";

/**
 * The whole volume as one strip: scanned data by category, the rest of used
 * space, and free space. Hatching inside a category marks the part covered by
 * Likely safe recommendations, so "how much can I get back" sits on top of
 * "where did it go". Every number comes from statfs, the scan or the
 * recommendations; nothing is estimated here.
 */
export function StorageStrip({
  disk,
  summary,
  recommendations,
  onReview,
}: {
  disk: DiskInfo;
  summary: ScanSummary | null;
  recommendations: Recommendation[];
  onReview: () => void;
}) {
  const recoverable = useMemo(() => recoverableByCategory(recommendations), [recommendations]);
  const recoverableTotal = [...recoverable.values()].reduce((a, b) => a + b, 0);

  const scanned = summary ? Math.min(summary.totalBytes, disk.usedBytes) : 0;
  // Scale down only if the scan exceeds used space (APFS clones count per copy).
  const scale = summary && summary.totalBytes > 0 ? scanned / summary.totalBytes : 0;
  const segments: Segment[] = [
    ...(summary
      ? [...summary.categories]
          .sort((a, b) => b.bytes - a.bytes)
          .map((c) => ({
            key: c.category,
            label: CATEGORY_META[c.category].label,
            bytes: c.bytes * scale,
            color: CATEGORY_META[c.category].color,
            recoverable: Math.min((recoverable.get(c.category) ?? 0) * scale, c.bytes * scale),
          }))
      : []),
    {
      key: "other",
      label: summary ? "Not scanned" : "Used",
      bytes: Math.max(disk.usedBytes - scanned, 0),
      color: "var(--chart-other)",
      recoverable: 0,
    },
  ];
  const total = disk.totalBytes;
  const legend = segments.filter((s) => s.key === "other" || s.bytes / total >= 0.012);

  return (
    <section aria-labelledby="storage-heading">
      <div className="flex flex-wrap items-end justify-between gap-x-10 gap-y-4">
        <div>
          <h2 id="storage-heading" className="text-xs text-muted-foreground">
            Startup disk
          </h2>
          <p className="tabular mt-1 text-[28px] leading-8 font-semibold tracking-tight">
            {formatBytes(disk.usedBytes)}
            <span className="ml-2 text-[15px] font-normal tracking-normal text-muted-foreground">
              used of {formatBytes(total)}
            </span>
          </p>
        </div>
        <div className="flex items-end gap-10">
          <Figure
            label="Available"
            value={formatBytes(disk.availableBytes)}
            tip="Space free right now. Finder may show more because it also counts purgeable space, such as local snapshots, that macOS frees on demand."
          />
          {summary ? (
            <Figure
              label="Likely recoverable"
              value={formatBytes(recoverableTotal)}
              tone="safe"
              tip="The total of Likely safe recommendations: regenerable data such as caches and build output. Every item is still previewed and confirmed, and goes to the Trash first."
            />
          ) : null}
          {summary && recoverableTotal > 0 ? (
            <Button className="mb-0.5" onClick={onReview}>
              Review cleanup
            </Button>
          ) : null}
        </div>
      </div>

      <div
        className="mt-4 flex h-6 w-full gap-[2px] overflow-hidden rounded-md bg-track"
        role="img"
        aria-label={[
          ...segments.map((s) => `${s.label}: ${formatBytes(s.bytes)}`),
          `Available: ${formatBytes(disk.availableBytes)}`,
        ].join(", ")}
      >
        {segments.map((s, i) =>
          s.bytes > 0 ? (
            <Tooltip key={s.key}>
              <TooltipTrigger asChild>
                <div
                  className="fill-in relative h-full"
                  style={{
                    width: `${(s.bytes / total) * 100}%`,
                    minWidth: 3,
                    background: s.color,
                    animationDelay: `${i * 40}ms`,
                  }}
                >
                  {s.recoverable > 0 ? (
                    <div
                      className="absolute inset-y-0 right-0"
                      style={{ width: `${(s.recoverable / s.bytes) * 100}%`, background: HATCH }}
                    />
                  ) : null}
                </div>
              </TooltipTrigger>
              <TooltipContent>
                <p className="font-medium">{s.label}</p>
                <p className="tabular text-muted-foreground">
                  {formatBytes(s.bytes)} · {formatPercent(s.bytes, total)} of disk
                </p>
                {s.recoverable > 0 ? (
                  <p className="tabular text-muted-foreground">
                    {formatBytes(s.recoverable)} likely recoverable
                  </p>
                ) : null}
              </TooltipContent>
            </Tooltip>
          ) : null,
        )}
      </div>

      <ul className="mt-3 flex flex-wrap items-center gap-x-5 gap-y-1.5">
        {legend.map((s) => (
          <li key={s.key} className="flex items-center gap-1.5 text-xs">
            <span className="size-2 rounded-full" style={{ background: s.color }} aria-hidden />
            <span className="text-muted-foreground">{s.label}</span>
            <span className="tabular">{formatBytes(s.bytes)}</span>
          </li>
        ))}
        <li className="flex items-center gap-1.5 text-xs">
          <span className="size-2 rounded-full border border-border-strong bg-track" aria-hidden />
          <span className="text-muted-foreground">Available</span>
          <span className="tabular">{formatBytes(disk.availableBytes)}</span>
        </li>
        {recoverableTotal > 0 ? (
          <li className="flex items-center gap-1.5 text-xs">
            <span
              className="size-2.5 rounded-[3px] bg-muted-foreground"
              style={{ backgroundImage: HATCH }}
              aria-hidden
            />
            <span className="text-muted-foreground">Likely recoverable</span>
          </li>
        ) : null}
      </ul>
    </section>
  );
}

function Figure({
  label,
  value,
  tip,
  tone,
}: {
  label: string;
  value: string;
  tip: string;
  tone?: "safe";
}) {
  return (
    <div>
      <p className="flex items-center gap-1 text-xs text-muted-foreground">
        {label}
        <Tooltip>
          <TooltipTrigger asChild>
            <button
              type="button"
              className="rounded-sm text-subtle-foreground"
              aria-label={`About ${label.toLowerCase()}`}
            >
              <Info className="size-3" aria-hidden />
            </button>
          </TooltipTrigger>
          <TooltipContent>{tip}</TooltipContent>
        </Tooltip>
      </p>
      <p
        className="tabular mt-0.5 text-[20px] leading-7 font-semibold tracking-tight"
        style={tone === "safe" ? { color: "var(--tier-safe)" } : undefined}
      >
        {value}
      </p>
    </div>
  );
}
