import { Milestone } from "lucide-react";
import type * as React from "react";

/** A clear, honest note about functionality that is planned but not built. */
export function RoadmapNote({
  phase,
  title,
  children,
}: {
  phase: string;
  title: string;
  children: React.ReactNode;
}) {
  return (
    <div className="rounded-lg border border-dashed border-border-strong px-4 py-3.5">
      <p className="flex items-center gap-2 text-xs font-medium">
        <Milestone className="size-3.5 text-subtle-foreground" aria-hidden />
        {title}
        <span className="rounded bg-panel-raised px-1.5 py-px text-2xs font-normal text-muted-foreground">
          {phase}
        </span>
      </p>
      <div className="mt-1.5 text-xs leading-relaxed text-muted-foreground">{children}</div>
    </div>
  );
}
