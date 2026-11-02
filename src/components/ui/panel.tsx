import type * as React from "react";

import { cn } from "@/lib/utils";

/** The basic surface: a bordered panel with an optional header row. */
export function Panel({ className, ...props }: React.HTMLAttributes<HTMLDivElement>) {
  return (
    <section className={cn("rounded-lg border border-border bg-panel", className)} {...props} />
  );
}

export function PanelHeader({
  title,
  description,
  actions,
  className,
}: {
  title: React.ReactNode;
  description?: React.ReactNode;
  actions?: React.ReactNode;
  className?: string;
}) {
  return (
    <header className={cn("flex items-start justify-between gap-4 px-4 pt-3.5 pb-3", className)}>
      <div className="min-w-0">
        <h2 className="text-[13px] font-medium text-foreground">{title}</h2>
        {description ? <p className="mt-0.5 text-xs text-muted-foreground">{description}</p> : null}
      </div>
      {actions ? <div className="flex shrink-0 items-center gap-1.5">{actions}</div> : null}
    </header>
  );
}
