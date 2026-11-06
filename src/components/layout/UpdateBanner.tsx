import { ArrowDownCircle, Loader2 } from "lucide-react";
import { useState } from "react";

import { WhatsNewDialog } from "@/components/shared/WhatsNewDialog";
import { Button } from "@/components/ui/button";
import { inAppNotes } from "@/lib/releaseNotes";
import { useUpdates } from "@/lib/updates";

/** Shown in the sidebar when a newer DiskPilot is available. */
export function UpdateBanner() {
  const { update, installing, progress, error, install } = useUpdates();
  const [notesOpen, setNotesOpen] = useState(false);
  if (!update) return null;

  const pct =
    progress?.total && progress.total > 0
      ? Math.min(100, Math.round((progress.done / progress.total) * 100))
      : null;
  const updateButton = (
    <Button size="sm" onClick={() => void install()} disabled={installing}>
      {installing ? <Loader2 className="animate-spin" /> : null}
      {installing ? "Updating…" : "Update and restart"}
    </Button>
  );

  return (
    <div className="mb-1 rounded-md border border-border bg-panel px-2.5 py-2" aria-live="polite">
      <p className="flex items-center gap-1.5 text-xs font-medium">
        <ArrowDownCircle className="size-3.5 text-accent" aria-hidden />
        DiskPilot {update.version} is available
      </p>
      {installing ? (
        <div className="mt-2">
          <div
            className="h-[3px] overflow-hidden rounded-full bg-track"
            role="progressbar"
            aria-label="Downloading the update"
            aria-valuemin={0}
            aria-valuemax={100}
            aria-valuenow={pct ?? undefined}
          >
            <div
              className="h-full rounded-full bg-accent transition-[width] duration-150"
              style={{ width: `${pct ?? 30}%` }}
            />
          </div>
          <p className="mt-1 text-2xs text-subtle-foreground">
            {pct === 100 ? "Installing, DiskPilot will restart…" : "Downloading…"}
          </p>
        </div>
      ) : (
        <div className="mt-1.5 flex gap-1.5">
          <Button size="sm" variant="secondary" onClick={() => setNotesOpen(true)}>
            What's new
          </Button>
          {updateButton}
        </div>
      )}
      {error ? <p className="mt-1 text-2xs text-critical">{error}</p> : null}
      <WhatsNewDialog
        open={notesOpen}
        title={`What's new in DiskPilot ${update.version}`}
        subtitle="Updating keeps your scan history, settings and saved results."
        notes={update.body ? inAppNotes(update.body) : null}
        onClose={() => setNotesOpen(false)}
        footer={updateButton}
      />
    </div>
  );
}
