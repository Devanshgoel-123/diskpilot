import type * as React from "react";

import { ScanControl } from "./ScanControl";

export function PageHeader({
  title,
  description,
  actions,
}: {
  title: string;
  description?: React.ReactNode;
  actions?: React.ReactNode;
}) {
  return (
    <header
      data-tauri-drag-region
      className="flex shrink-0 items-end justify-between gap-6 border-b border-border px-6 pt-[18px] pb-3.5"
    >
      <div className="pointer-events-none min-w-0">
        <h1 className="text-base font-semibold tracking-tight">{title}</h1>
        {description ? (
          <p className="mt-0.5 truncate text-xs text-muted-foreground">{description}</p>
        ) : null}
      </div>
      <div className="flex shrink-0 items-center gap-2">
        {actions}
        <ScanControl />
      </div>
    </header>
  );
}
