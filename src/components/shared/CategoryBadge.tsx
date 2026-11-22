import { CATEGORY_META } from "@/lib/categories";
import type { Category } from "@/lib/types";
import { cn } from "@/lib/utils";

/** A category as a neutral icon and plain text. Quiet by design: categories
 * describe data, while tier tags are the only colored labels. A colored dot
 * here would read as a tier (the cache hue sits next to "Review first"), so
 * category hues stay in charts, where there is no tier to confuse them with. */
export function CategoryBadge({ category, className }: { category: Category; className?: string }) {
  const meta = CATEGORY_META[category];
  const Icon = meta.icon;
  return (
    <span
      title={meta.description}
      className={cn(
        "inline-flex items-center gap-1 text-xs whitespace-nowrap text-muted-foreground",
        className,
      )}
    >
      <Icon className="size-3 shrink-0 text-subtle-foreground" aria-hidden />
      {meta.label}
    </span>
  );
}
