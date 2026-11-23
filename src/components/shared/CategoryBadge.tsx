import { CATEGORY_META } from "@/lib/categories";
import type { Category } from "@/lib/types";
import { cn } from "@/lib/utils";

const symbolId = (category: Category) => `category-icon-${category}`;

/**
 * Every category icon once, as SVG symbols the badges point at. Rendered once
 * at the app root, so a list of 80 recommendations draws 160 small `<use>`
 * elements instead of 160 full icon components with their paths.
 */
export function CategoryIconSprite() {
  return (
    <svg aria-hidden width="0" height="0" style={{ position: "absolute" }}>
      {(Object.keys(CATEGORY_META) as Category[]).map((c) => {
        const Icon = CATEGORY_META[c].icon;
        return (
          <symbol key={c} id={symbolId(c)} viewBox="0 0 24 24">
            <Icon />
          </symbol>
        );
      })}
    </svg>
  );
}

/** A category as a neutral icon and plain text. Quiet by design: categories
 * describe data, while tier tags are the only colored labels. A colored dot
 * here would read as a tier (the cache hue sits next to "Review first"), so
 * category hues stay in charts, where there is no tier to confuse them with.
 * The icon comes from CategoryIconSprite. */
export function CategoryBadge({ category, className }: { category: Category; className?: string }) {
  const meta = CATEGORY_META[category];
  return (
    <span
      title={meta.description}
      className={cn(
        "inline-flex items-center gap-1 text-xs whitespace-nowrap text-muted-foreground",
        className,
      )}
    >
      <svg className="size-3 shrink-0 text-subtle-foreground" aria-hidden>
        <use href={`#${symbolId(category)}`} />
      </svg>
      {meta.label}
    </span>
  );
}
