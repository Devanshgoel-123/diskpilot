import type { CSSProperties } from "react";

import { TIER_META } from "@/lib/tiers";
import type { Tier } from "@/lib/types";
import { cn } from "@/lib/utils";

export function TierBadge({ tier, className }: { tier: Tier; className?: string }) {
  const meta = TIER_META[tier];
  const Icon = meta.icon;
  return (
    <span
      title={meta.description}
      style={{ "--tint": meta.color } as CSSProperties}
      className={cn(
        "tinted tint-text inline-flex items-center gap-1 rounded-full border px-2 py-px text-2xs font-medium whitespace-nowrap",
        className,
      )}
    >
      <Icon className="size-3 shrink-0" aria-hidden />
      {meta.label}
    </span>
  );
}
