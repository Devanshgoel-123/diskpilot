import type { LucideIcon } from "lucide-react";
import type * as React from "react";

import { cn } from "@/lib/utils";

export function EmptyState({
  icon: Icon,
  title,
  description,
  action,
  className,
}: {
  icon: LucideIcon;
  title: string;
  description?: React.ReactNode;
  action?: React.ReactNode;
  className?: string;
}) {
  return (
    <div className={cn("flex flex-col items-center px-6 py-12 text-center", className)}>
      <Icon className="size-6 text-subtle-foreground" strokeWidth={1.5} aria-hidden />
      <p className="mt-3 text-[15px] font-semibold tracking-tight">{title}</p>
      {description ? (
        <div className="mt-1.5 max-w-md text-[13px] leading-relaxed text-muted-foreground">
          {description}
        </div>
      ) : null}
      {action ? <div className="mt-4">{action}</div> : null}
    </div>
  );
}
