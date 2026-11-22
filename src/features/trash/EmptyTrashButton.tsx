import { useEffect } from "react";

import { formatBytes } from "@/lib/format";
import { words } from "@/lib/platform";
import { cn } from "@/lib/utils";

import { TrashGlyph } from "./TrashGlyph";
import { useTrash } from "./trashStore";

/**
 * Opens the Empty Trash window. Shows the Trash's current size when DiskPilot
 * can read it; the window lists the Trash again before anything is deleted.
 */
export function EmptyTrashButton({ className }: { className?: string }) {
  const { summary, openDialog, refresh } = useTrash();

  useEffect(() => {
    void refresh();
    const onFocus = () => void refresh();
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  }, [refresh]);

  const known = summary?.access === "granted";
  const empty = known && summary.items === 0;

  return (
    <button
      type="button"
      onClick={openDialog}
      aria-label={
        known && !empty
          ? `${words().emptyTrash}, ${formatBytes(summary.totalBytes)}`
          : words().emptyTrash
      }
      className={cn(
        "group inline-flex h-7 shrink-0 items-center gap-1.5 rounded-md border border-border-strong bg-panel pl-2.5 text-[13px] font-medium text-foreground",
        known ? "pr-1" : "pr-2.5",
        "transition-[background-color,border-color,box-shadow,transform] duration-[160ms] ease-out",
        "hover:border-[color-mix(in_oklab,var(--critical)_35%,var(--border-strong))] hover:bg-hover hover:shadow-[0_1px_8px_-3px_color-mix(in_oklab,var(--critical)_45%,transparent)]",
        "active:scale-[0.97] active:duration-75",
        className,
      )}
    >
      <TrashGlyph className="text-muted-foreground transition-colors duration-[160ms] group-hover:text-critical" />
      <span>{words().emptyTrash}</span>
      <span
        className={cn(
          "tabular ml-0.5 rounded-[4px] px-1.5 py-px text-2xs transition-colors duration-[160ms]",
          known
            ? "bg-track text-muted-foreground group-hover:text-foreground"
            : "w-0 overflow-hidden px-0",
        )}
      >
        {known ? (empty ? "Empty" : formatBytes(summary.totalBytes)) : null}
      </span>
    </button>
  );
}
