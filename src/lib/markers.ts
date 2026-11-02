import type { MarkerKind, Recommendation } from "./types";

export const MARKER_ORDER: MarkerKind[] = ["olderVersion", "leftover", "installer", "duplicate"];

export const MARKER_META: Record<MarkerKind, { label: string; plural: string }> = {
  olderVersion: { label: "Older version", plural: "Older versions" },
  leftover: { label: "Leftover", plural: "Leftovers" },
  installer: { label: "Installer", plural: "Installers" },
  duplicate: { label: "Duplicate", plural: "Duplicates" },
};

/** Marker kinds present in `recs`, in display order, with counts. */
export function markerCounts(recs: Recommendation[]): { kind: MarkerKind; count: number }[] {
  const counts = new Map<MarkerKind, number>();
  for (const r of recs) {
    if (r.marker) counts.set(r.marker.kind, (counts.get(r.marker.kind) ?? 0) + 1);
  }
  return MARKER_ORDER.filter((k) => counts.has(k)).map((kind) => ({
    kind,
    count: counts.get(kind) ?? 0,
  }));
}
