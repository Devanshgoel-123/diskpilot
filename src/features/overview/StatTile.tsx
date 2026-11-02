import type * as React from "react";

import { Panel } from "@/components/ui/panel";

export function StatTile({
  label,
  value,
  hint,
}: {
  label: string;
  value: React.ReactNode;
  hint?: React.ReactNode;
}) {
  return (
    <Panel className="px-4 py-3">
      <p className="text-muted-foreground text-xs">{label}</p>
      <p className="tabular mt-1 text-lg font-semibold tracking-tight">{value}</p>
      {hint ? <p className="text-2xs text-subtle-foreground mt-0.5 leading-snug">{hint}</p> : null}
    </Panel>
  );
}
