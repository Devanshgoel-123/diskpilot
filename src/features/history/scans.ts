import { formatBytes } from "@/lib/format";
import type { ScanRecord } from "@/lib/types";

/** Smaller than this, a change between two scans reads as "Same". */
const SAME_BELOW = 1024 * 1024;

const DAY = 24 * 60 * 60 * 1000;

function startOfDay(ms: number): number {
  const d = new Date(ms);
  d.setHours(0, 0, 0, 0);
  return d.getTime();
}

/** "Today", "Yesterday", "Wed, Nov 5", or with the year when it is not this year. */
export function dayLabel(ms: number, now = Date.now()): string {
  const day = startOfDay(ms);
  const today = startOfDay(now);
  if (day === today) return "Today";
  // A day before today; DST can make it 23 or 25 hours long.
  if (Math.round((today - day) / DAY) === 1) return "Yesterday";
  const sameYear = new Date(ms).getFullYear() === new Date(now).getFullYear();
  return new Date(ms).toLocaleDateString("en-US", {
    weekday: "short",
    month: "short",
    day: "numeric",
    ...(sameYear ? {} : { year: "numeric" }),
  });
}

export function timeOfDay(ms: number): string {
  return new Date(ms).toLocaleTimeString("en-US", { hour: "numeric", minute: "2-digit" });
}

/**
 * For each completed scan, how much its folder grew or shrank since the
 * previous completed scan of the same folder. Missing when there is none.
 */
export function sizeChanges(records: ScanRecord[]): Map<string, number> {
  const changes = new Map<string, number>();
  const last = new Map<string, number>();
  const oldestFirst = records
    .filter((r) => r.status === "completed")
    .sort((a, b) => a.startedAtMs - b.startedAtMs);
  for (const r of oldestFirst) {
    const before = last.get(r.root);
    if (before !== undefined) changes.set(r.id, r.totalBytes - before);
    last.set(r.root, r.totalBytes);
  }
  return changes;
}

export function formatChange(delta: number): string {
  if (Math.abs(delta) < SAME_BELOW) return "Same";
  return `${delta > 0 ? "+" : "−"}${formatBytes(Math.abs(delta))}`;
}
