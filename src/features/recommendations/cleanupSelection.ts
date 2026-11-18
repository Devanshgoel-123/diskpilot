/**
 * Pure selection rules for the Cleanup list. Kept separate from the page so
 * range behavior is easy to test without rendering the app.
 */
import type { Recommendation } from "@/lib/types";

/**
 * Apply a checkbox click that may have been a shift-click. The range follows
 * the order the rows are shown in. Only rows with an automated action take
 * part; a row without one has no checkbox and is skipped.
 *
 * `checked` is the state the clicked row will have after the click, so it
 * decides whether the range is selected or cleared. When there is no anchor
 * or the anchor is not in the shown rows, only the target changes.
 */
export function applyRangeSelection(
  shown: Recommendation[],
  anchorId: string | null,
  targetId: string,
  checked: boolean,
  current: ReadonlySet<string>,
): Set<string> {
  const next = new Set(current);
  const start = shown.findIndex((r) => r.id === anchorId);
  const end = shown.findIndex((r) => r.id === targetId);
  if (end < 0) return next;

  const from = start < 0 ? end : Math.min(start, end);
  const to = start < 0 ? end : Math.max(start, end);
  for (const rec of shown.slice(from, to + 1)) {
    if (!rec.methods.length) continue;
    if (checked) next.add(rec.id);
    else next.delete(rec.id);
  }
  return next;
}
