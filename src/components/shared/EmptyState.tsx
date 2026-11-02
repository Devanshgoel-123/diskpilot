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
      <div className="border-border-strong bg-panel-raised text-muted-foreground flex size-9 items-center justify-center rounded-lg border">
        <Icon className="size-4" aria-hidden />
      </div>
      <p className="mt-3 text-[13px] font-medium">{title}</p>
      {description ? (
        <div className="text-muted-foreground mt-1 max-w-md text-xs leading-relaxed">
          {description}
        </div>
      ) : null}
      {action ? <div className="mt-4">{action}</div> : null}
    </div>
  );
}
