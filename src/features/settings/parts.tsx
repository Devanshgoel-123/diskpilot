import type * as React from "react";

import { Panel } from "@/components/ui/panel";
import { cn } from "@/lib/utils";

export function Section({
  title,
  id,
  children,
}: {
  title: string;
  id?: string;
  children: React.ReactNode;
}) {
  return (
    <section id={id} className="scroll-mt-6">
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

/** An on/off switch for a setting. */
export function Switch({
  on,
  onChange,
  label,
}: {
  on: boolean;
  onChange: (v: boolean) => void;
  label: string;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={on}
      aria-label={label}
      onClick={() => onChange(!on)}
      className={cn(
        "relative h-5 w-9 rounded-full transition-colors",
        on ? "bg-accent" : "bg-border-strong",
      )}
    >
      <span
        className={cn(
          "absolute top-0.5 left-0.5 size-4 rounded-full bg-white shadow transition-transform",
          on && "translate-x-4",
        )}
      />
    </button>
  );
}
