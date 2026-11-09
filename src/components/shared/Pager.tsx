import { ChevronLeft, ChevronRight } from "lucide-react";

import { Button } from "@/components/ui/button";
import { formatCount } from "@/lib/format";
import type { Page } from "@/lib/usePage";
import { cn } from "@/lib/utils";

/** "101–200 of 2,000" with previous and next page buttons. Hidden for one page. */
export function Pager<T>({
  page,
  label = "items",
  className,
}: {
  page: Page<T>;
  label?: string;
  className?: string;
}) {
  if (page.pageCount <= 1) return null;
  const first = page.start + 1;
  const last = page.start + page.items.length;
  return (
    <nav
      aria-label={`Pages of ${label}`}
      className={cn("flex items-center justify-end gap-2 text-xs text-muted-foreground", className)}
    >
      <span className="tabular" aria-live="polite">
        {formatCount(first)}–{formatCount(last)} of {formatCount(page.total)}
      </span>
      <Button
        variant="ghost"
        size="icon"
        aria-label="Previous page"
        disabled={page.page === 0}
        onClick={() => page.setPage(page.page - 1)}
      >
        <ChevronLeft />
      </Button>
      <span className="tabular">
        Page {page.page + 1} of {page.pageCount}
      </span>
      <Button
        variant="ghost"
        size="icon"
        aria-label="Next page"
        disabled={page.page >= page.pageCount - 1}
        onClick={() => page.setPage(page.page + 1)}
      >
        <ChevronRight />
      </Button>
    </nav>
  );
}
