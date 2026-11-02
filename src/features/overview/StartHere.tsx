import { ChevronRight } from "lucide-react";

import { AppIcon } from "@/components/shared/AppIcon";
import { TierBadge } from "@/components/shared/TierBadge";
import { formatBytes, tildify } from "@/lib/format";
import { useApp } from "@/lib/store";
import type { Recommendation } from "@/lib/types";

function TierDot({ tier }: { tier: Recommendation["tier"] }) {
  const color = tier === "likelySafe" ? "var(--tier-safe)" : "var(--tier-review)";
  return (
    <span
      className="flex size-5 shrink-0 items-center justify-center rounded-[5px]"
      style={{ background: `color-mix(in oklab, ${color} 16%, transparent)` }}
      aria-hidden
    >
      <span className="size-1.5 rounded-full" style={{ background: color }} />
    </span>
  );
}

export function StartHere({ items }: { items: Recommendation[] }) {
  const home = useApp((s) => s.env?.homeDir);
  const review = useApp((s) => s.reviewInRecommendations);
  if (!items.length) {
    return (
      <p className="py-3 text-[13px] text-muted-foreground">
        Nothing large stands out. The biggest folders are listed under Largest items.
      </p>
    );
  }
  return (
    <ul className="-mx-2">
      {items.map((r) => (
        <li key={r.id}>
          <button
            type="button"
            onClick={() => review([r.id])}
            className="group flex w-full items-center gap-3 rounded-md px-2 py-2 text-left transition-colors duration-[120ms] hover:bg-hover"
          >
            <AppIcon path={r.path} className="size-5" fallback={<TierDot tier={r.tier} />} />
            <div className="min-w-0 flex-1">
              <p className="truncate text-[13px] font-medium">{r.title}</p>
              <p className="truncate text-xs text-muted-foreground" title={r.safeBecause}>
                {r.safeBecause}
              </p>
              <p className="truncate font-mono text-2xs text-subtle-foreground">
                {tildify(r.path, home)}
              </p>
            </div>
            <TierBadge tier={r.tier} className="shrink-0" />
            <span className="tabular w-[72px] shrink-0 text-right text-[13px] font-medium">
              {formatBytes(r.estimatedReclaimBytes)}
            </span>
            <ChevronRight
              className="size-3.5 shrink-0 text-subtle-foreground opacity-0 transition-opacity group-hover:opacity-100"
              aria-hidden
            />
          </button>
        </li>
      ))}
    </ul>
  );
}
