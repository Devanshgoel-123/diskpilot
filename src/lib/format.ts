import { isWindows } from "./platform";

/**
 * Formatting helpers. Sizes match the system's own: decimal units on macOS
 * (1 GB = 10^9 bytes), like Finder, and units of 1024 on Windows, which File
 * Explorer and Settings also label "GB". Otherwise a 492 GB disk here shows
 * as 458 GB in Windows.
 */

const UNITS = ["B", "KB", "MB", "GB", "TB", "PB"] as const;

export function formatBytes(bytes: number | null | undefined, digits = 1): string {
  if (bytes == null || !Number.isFinite(bytes)) return "—";
  const base = isWindows() ? 1024 : 1000;
  if (bytes < base) return `${Math.max(0, Math.round(bytes))} B`;
  let value = bytes;
  let unit = 0;
  while (value >= base && unit < UNITS.length - 1) {
    value /= base;
    unit += 1;
  }
  if (isWindows()) return `${windowsDigits(value)} ${UNITS[unit]}`;
  const fixed = value >= 100 ? 0 : digits;
  return `${value.toFixed(fixed)} ${UNITS[unit]}`;
}

/**
 * Three significant digits, cut off rather than rounded, as Settings and File
 * Explorer show them: 358.78 GB used reads "358 GB" and 99.28 GB free reads
 * "99.2 GB" there, so rounding to "359 GB" and "99.3 GB" would disagree with
 * the numbers people check against.
 */
function windowsDigits(value: number): string {
  const decimals = value >= 100 ? 0 : value >= 10 ? 1 : 2;
  const scale = 10 ** decimals;
  // The small nudge keeps 1.15 (stored as 1.1499…) from cutting to 1.14.
  return (Math.floor(value * scale + 1e-9) / scale).toFixed(decimals);
}

export function formatCount(n: number | null | undefined): string {
  if (n == null) return "—";
  return new Intl.NumberFormat("en-US").format(n);
}

export function formatPercent(part: number, whole: number): string {
  if (whole <= 0) return "0%";
  const pct = (part / whole) * 100;
  if (pct > 0 && pct < 0.1) return "<0.1%";
  return `${pct < 10 ? pct.toFixed(1) : Math.round(pct)}%`;
}

export function formatDuration(ms: number): string {
  if (ms < 1000) return `${Math.round(ms)} ms`;
  const s = ms / 1000;
  if (s < 60) return `${s.toFixed(s < 10 ? 1 : 0)} s`;
  const m = Math.floor(s / 60);
  const rest = Math.round(s % 60);
  return `${m}m ${rest}s`;
}

const rtf = new Intl.RelativeTimeFormat("en", { numeric: "auto" });

export function formatRelative(ms: number | null | undefined, now = Date.now()): string {
  if (ms == null) return "—";
  const diff = ms - now;
  const abs = Math.abs(diff);
  const minute = 60_000;
  const hour = 60 * minute;
  const day = 24 * hour;
  if (abs < minute) return "just now";
  if (abs < hour) return rtf.format(Math.round(diff / minute), "minute");
  if (abs < day) return rtf.format(Math.round(diff / hour), "hour");
  if (abs < 30 * day) return rtf.format(Math.round(diff / day), "day");
  if (abs < 365 * day) return rtf.format(Math.round(diff / (30 * day)), "month");
  return rtf.format(Math.round(diff / (365 * day)), "year");
}

/** Whether `ms` lies more than `ageMs` in the past. */
export function isOlderThan(ms: number, ageMs: number, now = Date.now()): boolean {
  return now - ms > ageMs;
}

export function formatDate(ms: number | null | undefined): string {
  if (ms == null) return "—";
  return new Date(ms).toLocaleString("en-US", {
    year: "numeric",
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

// Path display lives in paths.ts; re-exported for existing imports.
export { basename, tildify } from "./paths";
