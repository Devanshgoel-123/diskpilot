import { CATEGORY_META } from "@/lib/categories";
import type { Category } from "@/lib/types";
import { cn } from "@/lib/utils";

/** A category as a colored dot and plain text. Quiet by design: categories
 * describe data, while tier tags are the only colored labels. */
export function CategoryBadge({ category, className }: { category: Category; className?: string }) {
  const meta = CATEGORY_META[category];
  return (
    <span
      title={meta.description}
      className={cn(
        "inline-flex items-center gap-1.5 text-xs whitespace-nowrap text-muted-foreground",
        className,
      )}
    >
      <span
        className="size-[7px] shrink-0 rounded-full"
        style={{ background: meta.color }}
        aria-hidden
      />
      {meta.label}
    </span>
  );
}
