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
      className="flex min-h-[52px] shrink-0 items-center justify-between gap-6 border-b border-border px-6 py-2.5"
    >
      <div className="pointer-events-none min-w-0">
        <h1 className="text-[15px] leading-5 font-semibold">{title}</h1>
        {description ? (
          <p className="truncate text-xs text-muted-foreground">{description}</p>
        ) : null}
      </div>
      <div className="flex shrink-0 items-center gap-2">
        {actions}
        <ScanControl />
      </div>
    </header>
  );
}
