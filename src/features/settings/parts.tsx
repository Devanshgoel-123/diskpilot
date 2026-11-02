import type * as React from "react";

import { Panel } from "@/components/ui/panel";

export function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section>
      <h2 className="mb-2 text-xs font-medium text-muted-foreground">{title}</h2>
      <Panel className="divide-y divide-border">{children}</Panel>
    </section>
  );
}

export function Setting({
  label,
  description,
  control,
}: {
  label: string;
  description?: React.ReactNode;
  control: React.ReactNode;
}) {
  return (
    <div className="flex items-center justify-between gap-6 px-4 py-3">
      <div className="min-w-0">
        <p className="text-[13px]">{label}</p>
        {description ? (
          <div className="mt-0.5 text-xs leading-relaxed text-muted-foreground">{description}</div>
        ) : null}
      </div>
      <div className="shrink-0">{control}</div>
    </div>
  );
}
