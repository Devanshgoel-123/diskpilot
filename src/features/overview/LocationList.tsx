import { ChevronRight } from "lucide-react";

import { AppIcon } from "@/components/shared/AppIcon";
import { CategoryBadge } from "@/components/shared/CategoryBadge";
import { SizeBar } from "@/components/shared/SizeBar";
import { CATEGORY_META } from "@/lib/categories";
import { formatBytes, tildify } from "@/lib/format";
import { useApp } from "@/lib/store";
import type { DetectedLocation } from "@/lib/types";

/** Rows of detected locations; clicking one opens it in the Storage Explorer. */
export function LocationList({ locations }: { locations: DetectedLocation[] }) {
  const home = useApp((s) => s.env?.homeDir);
  const openInExplorer = useApp((s) => s.openInExplorer);
  const max = locations[0]?.bytes ?? 0;
  return (
    <ul className="divide-y divide-border">
      {locations.map((l) => (
        <li key={l.path}>
          <button
            type="button"
            onClick={() => openInExplorer(l.path)}
            className="group flex w-full items-center gap-3 px-4 py-2 text-left hover:bg-hover/60"
            title={l.classification.explanation}
          >
            <div className="min-w-0 flex-1">
              <div className="flex items-center gap-2">
                <AppIcon path={l.path} />
                <span className="truncate text-[13px]">{l.classification.label}</span>
                <CategoryBadge category={l.classification.category} />
              </div>
              <p className="mt-0.5 truncate font-mono text-2xs text-subtle-foreground">
                {tildify(l.path, home)}
              </p>
            </div>
            <div className="w-28 shrink-0">
              <p className="tabular text-right text-[13px]">{formatBytes(l.bytes)}</p>
              <SizeBar
                value={l.bytes}
                max={max}
                color={CATEGORY_META[l.classification.category].color}
                className="mt-1"
              />
            </div>
            <ChevronRight
              className="size-3.5 shrink-0 text-subtle-foreground opacity-0 group-hover:opacity-100"
              aria-hidden
            />
          </button>
        </li>
      ))}
    </ul>
  );
}
