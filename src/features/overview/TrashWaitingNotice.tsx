import { Trash2 } from "lucide-react";
import { useEffect } from "react";

import { EmptyTrashButton } from "@/features/trash/EmptyTrashButton";
import { useTrash } from "@/features/trash/trashStore";
import { formatBytes, formatCount } from "@/lib/format";

/**
 * Cleanup moves items to the Trash, so the space only comes back once the
 * Trash is emptied. Shows what is waiting there when DiskPilot can read it.
 */
export function TrashWaitingNotice() {
  const { summary, refresh } = useTrash();

  useEffect(() => {
    void refresh();
    const onFocus = () => void refresh();
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  }, [refresh]);

  if (summary?.access !== "granted" || summary.items === 0) return null;

  return (
    <div
      role="status"
      className="flex items-center justify-between gap-4 rounded-[10px] border border-border bg-panel-raised px-3 py-2.5"
    >
      <div className="flex min-w-0 gap-2 text-xs">
        <Trash2 className="mt-0.5 size-3.5 shrink-0 text-muted-foreground" aria-hidden />
        <div>
          <p className="text-foreground">
            <span className="tabular font-medium">{formatBytes(summary.totalBytes)}</span> is
            waiting in the Trash ({formatCount(summary.items)}{" "}
            {summary.items === 1 ? "item" : "items"}).
          </p>
          <p className="mt-0.5 text-muted-foreground">
            Moving items to the Trash keeps them recoverable. The space comes back when the Trash is
            emptied.
          </p>
        </div>
      </div>
      <EmptyTrashButton />
    </div>
  );
}
