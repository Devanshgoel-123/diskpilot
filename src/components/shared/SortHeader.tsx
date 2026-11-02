import { ChevronDown, ChevronUp } from "lucide-react";

import { cn } from "@/lib/utils";

export function SortHeader<K extends string>({
  label,
  k,
  sort,
  onSort,
  className,
  align = "left",
}: {
  label: string;
  k: K;
  sort: { key: K; desc: boolean };
  onSort: (k: K) => void;
  className?: string;
  align?: "left" | "right";
}) {
  const active = sort.key === k;
  const Icon = sort.desc ? ChevronDown : ChevronUp;
  return (
    <th
      className={cn("px-2 py-2 font-normal", className)}
      aria-sort={active ? (sort.desc ? "descending" : "ascending") : "none"}
    >
      <button
        type="button"
        onClick={() => onSort(k)}
        className={cn(
          "hover:text-foreground inline-flex w-full items-center gap-0.5",
          align === "right" && "justify-end",
          active && "text-foreground",
        )}
      >
        {label}
        <Icon className={cn("size-3", active ? "opacity-100" : "opacity-0")} aria-hidden />
      </button>
    </th>
  );
}
