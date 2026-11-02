import { cn } from "@/lib/utils";

/** A thin inline bar showing `value` relative to `max`. */
export function SizeBar({
  value,
  max,
  className,
}: {
  value: number | null;
  max: number;
  className?: string;
}) {
  const pct = value != null && max > 0 ? Math.max(0, Math.min(100, (value / max) * 100)) : 0;
  return (
    <div className={cn("bg-track h-1 w-full overflow-hidden rounded-full", className)} aria-hidden>
      <div
        className="bg-accent h-full rounded-full"
        style={{ width: `${pct}%`, minWidth: pct > 0 ? 2 : 0 }}
      />
    </div>
  );
}
