import { ChevronRight } from "lucide-react";

import { AppIcon } from "@/components/shared/AppIcon";
import { CATEGORY_META } from "@/lib/categories";
import { formatBytes, tildify } from "@/lib/format";
import { useApp } from "@/lib/store";
import type { DetectedLocation } from "@/lib/types";

/** Rows of detected locations; clicking one opens it in Storage. */
export function LocationList({ locations }: { locations: DetectedLocation[] }) {
  const home = useApp((s) => s.env?.homeDir);
  const openInExplorer = useApp((s) => s.openInExplorer);
  const max = locations[0]?.bytes ?? 0;
  return (
    <ul className="-mx-2">
      {locations.map((l) => {
        const meta = CATEGORY_META[l.classification.category];
        return (
          <li key={l.path}>
            <button
              type="button"
              onClick={() => openInExplorer(l.path)}
              className="group flex w-full items-center gap-3 rounded-md px-2 py-1.5 text-left transition-colors duration-[120ms] hover:bg-hover"
              title={`${meta.label}. ${l.classification.explanation}`}
            >
              <AppIcon
                path={l.path}
                fallback={
                  <span className="flex size-4 shrink-0 items-center justify-center" aria-hidden>
                    <span className="size-2 rounded-full" style={{ background: meta.color }} />
                  </span>
                }
              />
              <div className="min-w-0 flex-1">
                <p className="truncate text-[13px]">{l.classification.label}</p>
                <p className="truncate font-mono text-2xs text-subtle-foreground">
                  {tildify(l.path, home)}
                </p>
              </div>
              <div className="w-24 shrink-0">
                <p className="tabular text-right text-[13px]">{formatBytes(l.bytes)}</p>
                <div className="mt-1 h-[3px] overflow-hidden rounded-full bg-track" aria-hidden>
                  <div
                    className="h-full rounded-full"
                    style={{ width: `${max ? (l.bytes / max) * 100 : 0}%`, background: meta.color }}
                  />
                </div>
              </div>
              <ChevronRight
                className="size-3.5 shrink-0 text-subtle-foreground opacity-0 transition-opacity group-hover:opacity-100"
                aria-hidden
              />
            </button>
          </li>
        );
      })}
    </ul>
  );
}
