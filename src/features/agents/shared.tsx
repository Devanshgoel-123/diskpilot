import { type CSSProperties, type ReactNode } from "react";

/** Something a change needs the user to agree to first. */
export interface Confirmation {
  title: string;
  body: ReactNode;
  action: string;
  destructive?: boolean;
  run: () => Promise<boolean>;
}

export function Tint({ color, children }: { color: string; children: ReactNode }) {
  return (
    <span
      style={{ "--tint": color } as CSSProperties}
      className="tinted tint-text inline-flex shrink-0 items-center gap-1 rounded-full border px-1.5 py-px text-2xs font-medium"
    >
      {children}
    </span>
  );
}
