import { useMemo } from "react";

import { Button } from "@/components/ui/button";
import { HATCH } from "@/features/overview/StorageStrip";
import { CATEGORY_META } from "@/lib/categories";
import { formatBytes, formatCount, formatPercent } from "@/lib/format";
import type { DirectoryEntry, DirectoryListing, Recommendation } from "@/lib/types";
import { cn } from "@/lib/utils";

import { recommendationsUnder } from "./underPath";

/** Items drawn as their own segment; the rest share one. */
const MAX_SEGMENTS = 8;
/** Items smaller than this share of the folder join the rest. */
const MIN_SHARE = 0.01;
/** Items named under the strip. */
const LEGEND_ITEMS = 6;

interface Segment {
  entry: DirectoryEntry | null;
  bytes: number;
  color: string;
}

/**
 * The open folder at a glance: one strip of what fills it, the biggest items
 * by name, and how much the recommendations inside it can free. Every number
 * comes from the scan or the recommendations.
 */
export function FolderSummary({
  listing,
  cleanup,
  onOpen,
  onReview,
}: {
  listing: DirectoryListing;
  cleanup: Recommendation[];
  onOpen: (entry: DirectoryEntry) => void;
  onReview: (ids: string[]) => void;
}) {
  const total = listing.bytes ?? 0;

  const segments = useMemo(() => {
    const sized = listing.entries
      .filter((e) => (e.bytes ?? 0) > 0)
      .sort((a, b) => (b.bytes ?? 0) - (a.bytes ?? 0));
    const own: Segment[] = [];
    for (const e of sized) {
      if (own.length >= MAX_SEGMENTS || (e.bytes ?? 0) < total * MIN_SHARE) break;
      own.push({
        entry: e,
        bytes: e.bytes ?? 0,
        color: CATEGORY_META[e.classification.category].color,
      });
    }
    const shown = own.reduce((a, s) => a + s.bytes, 0);
    const rest = Math.max(0, total - shown);
    return rest > 0 ? [...own, { entry: null, bytes: rest, color: "var(--track)" }] : own;
  }, [listing, total]);

  const here = useMemo(() => recommendationsUnder(listing.path, cleanup), [listing.path, cleanup]);
  const freeable = here.reduce((a, r) => a + r.estimatedReclaimBytes, 0);
  // Hatch the part of each item that recommendations cover.
  const covered = useMemo(() => {
    const m = new Map<string, number>();
    for (const s of segments) {
      if (!s.entry) continue;
      const bytes = recommendationsUnder(s.entry.path, here).reduce(
        (a, r) => a + r.estimatedReclaimBytes,
        0,
      );
      if (bytes > 0) m.set(s.entry.path, Math.min(bytes, s.bytes));
    }
    return m;
  }, [segments, here]);

  const legend = segments
    .flatMap((s) => (s.entry ? [{ entry: s.entry, bytes: s.bytes, color: s.color }] : []))
    .slice(0, LEGEND_ITEMS);
  const others = listing.entries.length - legend.length;
  const label = listing.classification.category === "unknown" ? null : listing.classification;

  return (
    <section
      aria-label="This folder"
      className="shrink-0 space-y-3 border-b border-border bg-panel px-4 py-3"
    >
      <div className="flex flex-wrap items-center gap-x-4 gap-y-2">
        <p className="text-xs text-muted-foreground">
          <span className="tabular text-[15px] font-semibold tracking-tight text-foreground">
            {formatBytes(total)}
          </span>{" "}
          in {formatCount(listing.entries.length)} items
          {label ? (
            <span title={label.explanation}>
              {" · "}
              {label.label}
            </span>
          ) : null}
        </p>
        <div className="ml-auto flex items-center gap-3">
          {here.length ? (
            <>
              <span className="text-xs text-muted-foreground">
                <span className="tabular font-medium text-foreground">{formatBytes(freeable)}</span>{" "}
                recommended for cleanup here
              </span>
              <Button size="sm" variant="secondary" onClick={() => onReview(here.map((r) => r.id))}>
                Review {formatCount(here.length)}
              </Button>
            </>
          ) : (
            <span className="text-xs text-subtle-foreground">Nothing to clean up here</span>
          )}
        </div>
      </div>

      {total > 0 ? (
        <div className="flex h-2.5 w-full gap-px overflow-hidden rounded-full" aria-hidden>
          {segments.map((s, i) => {
            const hatched = s.entry ? (covered.get(s.entry.path) ?? 0) : 0;
            return (
              <div
                key={s.entry?.path ?? `rest-${i}`}
                className="relative h-full min-w-[3px]"
                style={{ flexGrow: s.bytes, flexBasis: 0, background: s.color }}
                title={`${s.entry?.name ?? "Everything else"} · ${formatBytes(s.bytes)} · ${formatPercent(s.bytes, total)}`}
              >
                {hatched > 0 ? (
                  <div
                    className="absolute inset-y-0 right-0"
                    style={{ width: `${(hatched / s.bytes) * 100}%`, backgroundImage: HATCH }}
                  />
                ) : null}
              </div>
            );
          })}
        </div>
      ) : null}

      {legend.length ? (
        <ul className="flex flex-wrap gap-x-1 gap-y-1">
          {legend.map((s) => {
            const e = s.entry;
            const dir = e.kind === "directory";
            return (
              <li key={e.path}>
                <button
                  type="button"
                  onClick={() => onOpen(e)}
                  disabled={!dir}
                  title={dir ? `Open ${e.name}` : e.name}
                  className={cn(
                    "flex max-w-[16rem] items-center gap-1.5 rounded px-1.5 py-0.5 text-xs",
                    dir && "hover:bg-hover",
                  )}
                >
                  <span
                    className="size-[7px] shrink-0 rounded-full"
                    style={{ background: s.color }}
                    aria-hidden
                  />
                  <span className="truncate text-foreground">{e.name}</span>
                  <span className="tabular shrink-0 text-muted-foreground">
                    {formatBytes(s.bytes)}
                  </span>
                  <span className="tabular shrink-0 text-subtle-foreground">
                    {formatPercent(s.bytes, total)}
                  </span>
                </button>
              </li>
            );
          })}
          {others > 0 ? (
            <li className="px-1.5 py-0.5 text-xs text-subtle-foreground">
              and {formatCount(others)} more
            </li>
          ) : null}
        </ul>
      ) : null}
    </section>
  );
}
