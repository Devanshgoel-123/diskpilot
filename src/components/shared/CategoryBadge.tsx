import type { CSSProperties } from "react";

import { CATEGORY_META } from "@/lib/categories";
import type { Category } from "@/lib/types";
import { cn } from "@/lib/utils";

export function CategoryBadge({ category, className }: { category: Category; className?: string }) {
  const meta = CATEGORY_META[category];
  const Icon = meta.icon;
  return (
    <span
      title={meta.description}
      style={{ "--tint": meta.color } as CSSProperties}
      className={cn(
        "tinted inline-flex items-center gap-1 rounded-[5px] border px-1.5 py-px text-2xs font-medium whitespace-nowrap text-foreground/90",
        className,
      )}
    >
      <Icon className="size-3 shrink-0" style={{ color: meta.color }} aria-hidden />
      {meta.label}
    </span>
  );
}
