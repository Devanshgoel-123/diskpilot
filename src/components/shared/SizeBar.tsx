import { cn } from "@/lib/utils";

/** A thin inline bar showing `value` relative to `max`. */
export function SizeBar({
  value,
  max,
  color = "var(--accent)",
  className,
}: {
  value: number | null;
  max: number;
  /** Fill color; pass the item's category color so bars carry identity. */
  color?: string;
  className?: string;
}) {
  const pct = value != null && max > 0 ? Math.max(0, Math.min(100, (value / max) * 100)) : 0;
  return (
    <div className={cn("h-1 w-full overflow-hidden rounded-full bg-track", className)} aria-hidden>
      <div
        className="h-full rounded-full"
        style={{ width: `${pct}%`, minWidth: pct > 0 ? 2 : 0, background: color }}
      />
    </div>
  );
}
