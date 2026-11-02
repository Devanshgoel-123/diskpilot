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
    <div className="border-border-strong rounded-lg border border-dashed px-4 py-3.5">
      <p className="flex items-center gap-2 text-xs font-medium">
        <Milestone className="text-subtle-foreground size-3.5" aria-hidden />
        {title}
        <span className="bg-panel-raised text-2xs text-muted-foreground rounded px-1.5 py-px font-normal">
          {phase}
        </span>
      </p>
      <div className="text-muted-foreground mt-1.5 text-xs leading-relaxed">{children}</div>
    </div>
  );
}
