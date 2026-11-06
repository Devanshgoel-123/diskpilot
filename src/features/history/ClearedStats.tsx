import { Flame, Sparkles } from "lucide-react";
import { useEffect, useState } from "react";

import { api } from "@/lib/api";
import { type CleanupStats, cleanupStats } from "@/lib/cleanupStats";
import { formatBytes, formatCount } from "@/lib/format";
import { useApp } from "@/lib/store";
import { cn } from "@/lib/utils";

/** Lifetime stats from this Mac's cleanup history; refreshes after each cleanup. */
function useCleanupStats(): CleanupStats | null {
  const cleanups = useApp((s) => s.cleanups);
  const [stats, setStats] = useState<CleanupStats | null>(null);
  useEffect(() => {
    let live = true;
    void api
      .getCleanupActivity()
      .then((a) => live && setStats(cleanupStats(a, Date.now())))
      .catch(() => live && setStats(null));
    return () => {
      live = false;
    };
  }, [cleanups]);
  return stats;
}

function weeks(n: number) {
  return `${formatCount(n)} ${n === 1 ? "week" : "weeks"}`;
}

function streakHint(s: CleanupStats): string {
  if (s.currentStreak === 0) return "Clean up once this week to start a streak.";
  if (s.clearedThisWeek) return "This week counts. Clean up again next week to keep it going.";
  return "Clean up before Sunday ends to keep it going.";
}

/** Space cleared and the weekly streak, as one line for the Overview. */
export function ClearedStatsLine() {
  const s = useCleanupStats();
  if (!s || s.cleanups === 0) return null;
  return (
    <div className="flex flex-wrap items-center gap-x-5 gap-y-1 rounded-[10px] border border-border bg-panel-raised px-3 py-2.5 text-xs">
      <span className="flex items-center gap-1.5">
        <Sparkles className="size-3.5 text-muted-foreground" aria-hidden />
        You have cleared{" "}
        <span className="tabular font-medium text-foreground">
          {formatBytes(s.clearedBytes)}
        </span>{" "}
        with DiskPilot
      </span>
      <span className="flex items-center gap-1.5">
        <Flame
          className={cn(
            "size-3.5",
            s.currentStreak > 0 ? "text-[var(--tier-review)]" : "text-muted-foreground",
          )}
          aria-hidden
        />
        {s.currentStreak > 0 ? (
          <>
            <span className="tabular font-medium text-foreground">{weeks(s.currentStreak)}</span> in
            a row
          </>
        ) : (
          "No streak yet"
        )}
      </span>
      <span className="text-muted-foreground">{streakHint(s)}</span>
    </div>
  );
}

/** The full stats for the History page. */
export function ClearedStatsPanel() {
  const s = useCleanupStats();
  if (!s) return null;
  const tiles = [
    {
      label: "Space cleared",
      value: formatBytes(s.clearedBytes),
      note: s.firstAtMs
        ? `${formatCount(s.cleanups)} ${s.cleanups === 1 ? "cleanup" : "cleanups"} since ${new Date(s.firstAtMs).toLocaleDateString("en-US", { year: "numeric", month: "short", day: "numeric" })}`
        : "No cleanups yet",
    },
    {
      label: "Weekly streak",
      value: weeks(s.currentStreak),
      note: streakHint(s),
    },
    {
      label: "Longest streak",
      value: weeks(s.longestStreak),
      note: "Weeks in a row with at least one cleanup",
    },
  ];
  return (
    <section aria-label="Your cleanup stats" className="space-y-2">
      <div className="grid gap-3 sm:grid-cols-3">
        {tiles.map((t) => (
          <div key={t.label} className="rounded-lg border border-border bg-panel px-4 py-3">
            <p className="text-xs text-muted-foreground">{t.label}</p>
            <p className="tabular mt-1 text-xl font-medium text-foreground">{t.value}</p>
            <p className="mt-1 text-2xs text-subtle-foreground">{t.note}</p>
          </div>
        ))}
      </div>
      <p className="text-2xs text-subtle-foreground">
        Counts files and Docker data DiskPilot deleted or moved to the Trash. Emptying the Trash is
        not counted again. Kept on this Mac only; nothing is sent.
      </p>
    </section>
  );
}
