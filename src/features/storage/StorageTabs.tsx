import { useApp, type StorageView } from "@/lib/store";
import { cn } from "@/lib/utils";

const VIEWS: { id: StorageView; label: string }[] = [
  { id: "browse", label: "Folders" },
  { id: "largest", label: "Largest items" },
];

/** Switches the Storage page between browsing folders and the largest items. */
export function StorageTabs() {
  const view = useApp((s) => s.storageView);
  const setView = useApp((s) => s.setStorageView);
  return (
    <div
      role="tablist"
      aria-label="Storage view"
      className="flex rounded-md border border-border-strong p-0.5"
    >
      {VIEWS.map((v) => (
        <button
          key={v.id}
          type="button"
          role="tab"
          aria-selected={view === v.id}
          onClick={() => setView(v.id)}
          className={cn(
            "h-6 rounded px-2.5 text-xs",
            view === v.id
              ? "bg-hover font-medium text-foreground"
              : "text-muted-foreground hover:text-foreground",
          )}
        >
          {v.label}
        </button>
      ))}
    </div>
  );
}
