import type { ActionStatus } from "@/lib/types";

/** One cleanup action's totals, as stored on this Mac. */
export interface CleanupActivity {
  kind: "files" | "docker" | "trash" | "wsl";
  finishedAtMs: number;
  status: ActionStatus;
  bytesFreed: number;
  bytesTrashed: number;
}

export interface CleanupStats {
  /** Space removed by DiskPilot cleanups: deleted plus moved to the Trash. */
  clearedBytes: number;
  /** Cleanups that removed something. */
  cleanups: number;
  firstAtMs: number | null;
  /** Weeks in a row, up to this week or last week, with a cleanup. */
  currentStreak: number;
  longestStreak: number;
  /** Whether this week already counts toward the streak. */
  clearedThisWeek: boolean;
}

const DAY_MS = 24 * 60 * 60 * 1000;

/** Local midnight of the Monday that starts the week containing `ms`. */
export function weekStart(ms: number): number {
  const d = new Date(ms);
  d.setHours(0, 0, 0, 0);
  const sinceMonday = (d.getDay() + 6) % 7;
  d.setDate(d.getDate() - sinceMonday);
  return d.getTime();
}

/** Whole weeks from week `a` to week `b` (both week starts), across DST. */
function weeksBetween(a: number, b: number): number {
  return Math.round((b - a) / (7 * DAY_MS));
}

/**
 * What counts: cleanups of files and Docker data that removed something,
 * whether deleted or moved to the Trash. Emptying the Trash is not counted
 * again, since most of what it removes was already counted when it was
 * moved there.
 */
function counts(a: CleanupActivity): number {
  if (a.kind === "trash" || a.status === "failed") return 0;
  return a.bytesFreed + a.bytesTrashed;
}

export function cleanupStats(activity: CleanupActivity[], nowMs: number): CleanupStats {
  const done = activity.filter((a) => counts(a) > 0);
  const weeks = [...new Set(done.map((a) => weekStart(a.finishedAtMs)))].sort((a, b) => a - b);

  let longest = 0;
  let run = 0;
  let prev: number | null = null;
  for (const week of weeks) {
    run = prev !== null && weeksBetween(prev, week) === 1 ? run + 1 : 1;
    longest = Math.max(longest, run);
    prev = week;
  }

  const thisWeek = weekStart(nowMs);
  const last = weeks[weeks.length - 1];
  // A streak stays alive through the current week until it ends.
  const alive = last !== undefined && weeksBetween(last, thisWeek) <= 1;

  return {
    clearedBytes: done.reduce((sum, a) => sum + counts(a), 0),
    cleanups: done.length,
    firstAtMs: done.length ? Math.min(...done.map((a) => a.finishedAtMs)) : null,
    currentStreak: alive ? run : 0,
    longestStreak: longest,
    clearedThisWeek: last === thisWeek,
  };
}
