import type { LucideIcon } from "lucide-react";
import type { CSSProperties, ReactNode } from "react";

import { Panel } from "@/components/ui/panel";

export function StatTile({
  label,
  value,
  hint,
  icon: Icon,
  tint = "var(--accent)",
}: {
  label: string;
  value: ReactNode;
  hint?: ReactNode;
  icon?: LucideIcon;
  /** Color of the icon chip. */
  tint?: string;
}) {
  return (
    <Panel className="px-4 py-3">
      <div className="flex items-center justify-between gap-2">
        <p className="text-xs text-muted-foreground">{label}</p>
        {Icon ? (
          <span
            className="tinted flex size-6 items-center justify-center rounded-md border"
            style={{ "--tint": tint } as CSSProperties}
          >
            <Icon className="size-3.5" style={{ color: tint }} aria-hidden />
          </span>
        ) : null}
      </div>
      <p className="tabular mt-1 text-lg font-semibold tracking-tight">{value}</p>
      {hint ? <p className="mt-0.5 text-2xs leading-snug text-subtle-foreground">{hint}</p> : null}
    </Panel>
  );
}
