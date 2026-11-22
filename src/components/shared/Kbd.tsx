import { keyLabel } from "@/lib/shortcuts";
import { cn } from "@/lib/utils";

/** One key, drawn like a keycap, as this platform's keyboard prints it. */
export function Kbd({ children, className }: { children: string; className?: string }) {
  return (
    <kbd
      className={cn(
        "min-w-5 rounded border border-border-strong bg-track px-1.5 py-px text-center font-mono text-2xs text-muted-foreground",
        className,
      )}
    >
      {keyLabel(children)}
    </kbd>
  );
}

/** The keys of one shortcut, side by side. */
export function Keys({ keys, className }: { keys: string[]; className?: string }) {
  return (
    <span className={cn("flex shrink-0 gap-1", className)}>
      {keys.map((k) => (
        <Kbd key={k}>{k}</Kbd>
      ))}
    </span>
  );
}
