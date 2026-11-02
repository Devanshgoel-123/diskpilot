import { Badge } from "@/components/ui/badge";
import { CATEGORY_META } from "@/lib/categories";
import type { Category } from "@/lib/types";

export function CategoryBadge({ category }: { category: Category }) {
  const meta = CATEGORY_META[category];
  const Icon = meta.icon;
  return (
    <Badge title={meta.description}>
      <Icon aria-hidden />
      {meta.label}
    </Badge>
  );
}
