import { MARKER_META } from "@/lib/markers";
import type { Marker } from "@/lib/types";
import { cn } from "@/lib/utils";

/** Why an item is no longer needed: an outlined, uncolored tag (tier tags are
 * the only colored ones). The evidence shows on hover. */
export function MarkerTag({ marker, className }: { marker: Marker; className?: string }) {
  return (
    <span
      title={marker.detail}
      className={cn(
        "inline-flex h-[18px] shrink-0 items-center rounded-[4px] border border-border-strong px-1.5 text-2xs whitespace-nowrap text-muted-foreground",
        className,
      )}
    >
      {MARKER_META[marker.kind].label}
    </span>
  );
}
