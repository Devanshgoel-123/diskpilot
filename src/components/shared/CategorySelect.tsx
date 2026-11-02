import { ChevronDown } from "lucide-react";

import { CATEGORY_META } from "@/lib/categories";
import type { Category } from "@/lib/types";

/** Native select styled to match; native gives correct keyboard + a11y for free. */
export function CategorySelect({
  value,
  onChange,
  options,
}: {
  value: Category | "all";
  onChange: (v: Category | "all") => void;
  options: Category[];
}) {
  return (
    <div className="relative">
      <select
        value={value}
        onChange={(e) => onChange(e.target.value as Category | "all")}
        aria-label="Filter by category"
        className="h-7 appearance-none rounded-md border border-border-strong bg-panel pr-7 pl-2.5 text-[13px] text-foreground focus-visible:border-accent focus-visible:outline-none"
      >
        <option value="all">All categories</option>
        {options.map((c) => (
          <option key={c} value={c}>
            {CATEGORY_META[c].label}
          </option>
        ))}
      </select>
      <ChevronDown
        className="pointer-events-none absolute top-1/2 right-2 size-3.5 -translate-y-1/2 text-subtle-foreground"
        aria-hidden
      />
    </div>
  );
}
