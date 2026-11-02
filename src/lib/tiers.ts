import { CircleCheck, CircleHelp, Lock, type LucideIcon } from "lucide-react";

import type { Tier } from "./types";

export interface TierMeta {
  label: string;
  description: string;
  icon: LucideIcon;
  color: string;
}

export const TIER_META: Record<Tier, TierMeta> = {
  likelySafe: {
    label: "Likely safe",
    description: "Well-understood, regenerable data. Still reviewed and confirmed by you.",
    icon: CircleCheck,
    color: "var(--tier-safe)",
  },
  reviewFirst: {
    label: "Review first",
    description: "Possibly disposable, but an app or project may depend on it.",
    icon: CircleHelp,
    color: "var(--tier-review)",
  },
  keep: {
    label: "Keep",
    description: "Important or not understood well enough to remove. Explained, never offered.",
    icon: Lock,
    color: "var(--tier-keep)",
  },
};

export const TIER_ORDER: Tier[] = ["likelySafe", "reviewFirst", "keep"];
