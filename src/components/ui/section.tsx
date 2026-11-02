import type * as React from "react";

import { cn } from "@/lib/utils";

/**
 * A titled region of a page. Sections are separated by space and hairlines,
 * not boxed: use Panel only for things that are separate objects.
 */
export function Section({
  title,
  description,
  actions,
  className,
  children,
  ...props
}: {
  title: React.ReactNode;
  description?: React.ReactNode;
  actions?: React.ReactNode;
} & React.HTMLAttributes<HTMLElement>) {
  return (
    <section className={cn("min-w-0", className)} {...props}>
      <header className="mb-2 flex min-h-7 items-end justify-between gap-4">
        <div className="min-w-0">
          <h2 className="text-[13px] font-semibold">{title}</h2>
          {description ? (
            <p className="mt-0.5 text-xs text-muted-foreground">{description}</p>
          ) : null}
        </div>
        {actions ? <div className="flex shrink-0 items-center gap-1.5">{actions}</div> : null}
      </header>
      {children}
    </section>
  );
}
