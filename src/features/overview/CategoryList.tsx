import { CATEGORY_META } from "@/lib/categories";
import { formatBytes, formatCount, formatPercent } from "@/lib/format";
import type { CategoryTotal } from "@/lib/types";

/** Scanned bytes by category, one row each, largest first. */
export function CategoryList({ categories }: { categories: CategoryTotal[] }) {
  const rows = [...categories].filter((c) => c.bytes > 0).sort((a, b) => b.bytes - a.bytes);
  const total = rows.reduce((a, c) => a + c.bytes, 0);
  const max = rows[0]?.bytes ?? 0;
  return (
    <ul className="space-y-0.5">
      {rows.map((c) => {
        const meta = CATEGORY_META[c.category];
        return (
          <li
            key={c.category}
            className="grid grid-cols-[minmax(0,9.5rem)_minmax(0,1fr)_4.5rem_3rem] items-center gap-3 py-1"
            title={`${meta.description} ${formatCount(c.fileCount)} files.`}
          >
            <span className="flex min-w-0 items-center gap-2 text-[13px]">
              <span
                className="size-2 shrink-0 rounded-full"
                style={{ background: meta.color }}
                aria-hidden
              />
              <span className="truncate">{meta.label}</span>
            </span>
            <span className="h-1.5 overflow-hidden rounded-full bg-track" aria-hidden>
              <span
                className="fill-in block h-full rounded-full"
                style={{ width: `${(c.bytes / max) * 100}%`, background: meta.color }}
              />
            </span>
            <span className="tabular text-right text-[13px]">{formatBytes(c.bytes)}</span>
            <span className="tabular text-right text-xs text-subtle-foreground">
              {formatPercent(c.bytes, total)}
            </span>
          </li>
        );
      })}
    </ul>
  );
}
