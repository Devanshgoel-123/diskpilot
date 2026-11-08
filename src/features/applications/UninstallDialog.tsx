import { ExternalLink, Loader2, RefreshCw, Terminal } from "lucide-react";
import { useState } from "react";

import { CopyButton } from "@/components/shared/CopyButton";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { api, errorMessage } from "@/lib/api";
import type { AppFootprint, UninstallOutcome } from "@/lib/types";

/**
 * Windows: starts an app's own uninstaller, or opens Settings > Apps when it
 * has none DiskPilot can run. DiskPilot never deletes the program itself;
 * the person finishes in the uninstaller, then refreshes the list.
 */
export function UninstallDialog({
  app,
  onClose,
  onRefresh,
}: {
  app: AppFootprint;
  onClose: () => void;
  onRefresh: () => void;
}) {
  const [starting, setStarting] = useState(false);
  const [outcome, setOutcome] = useState<UninstallOutcome | null>(null);
  const [error, setError] = useState<string | null>(null);
  const command = app.uninstallCommand;

  const start = async () => {
    setStarting(true);
    setError(null);
    try {
      setOutcome(await api.uninstallApp(app.bundleId));
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setStarting(false);
    }
  };

  return (
    <Dialog open onOpenChange={(o) => !o && !starting && onClose()}>
      <DialogContent className="w-[min(560px,calc(100vw-32px))]">
        <DialogHeader>
          <DialogTitle>
            {outcome === "uninstaller"
              ? "Finish in the uninstaller"
              : outcome === "settings"
                ? "Finish in Settings"
                : `Uninstall ${app.name}`}
          </DialogTitle>
          <DialogDescription>
            {outcome === "uninstaller"
              ? `The uninstaller for ${app.name} has started. Follow its steps, then refresh the list.`
              : outcome === "settings"
                ? `Settings is open at Installed apps. Find ${app.name}, choose Uninstall, then refresh the list here.`
                : command
                  ? "DiskPilot starts the program's own uninstaller. It may ask questions of its own. DiskPilot does not delete any files itself."
                  : "Windows removes this app from Settings. DiskPilot opens Settings > Apps > Installed apps, where you can uninstall it."}
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-3 px-5 py-4 text-xs">
          {command && !outcome ? (
            <div>
              <p className="mb-1.5 text-muted-foreground">The command it starts:</p>
              <div className="flex items-start gap-2">
                <code className="selectable flex min-w-0 flex-1 items-start gap-2 rounded-md border border-border bg-panel-raised px-3 py-2 font-mono text-xs">
                  <Terminal
                    className="mt-0.5 size-3.5 shrink-0 text-subtle-foreground"
                    aria-hidden
                  />
                  <span className="break-all">{command}</span>
                </code>
                <CopyButton text={command} />
              </div>
            </div>
          ) : null}
          <p className="text-muted-foreground">
            {outcome
              ? "If the app still appears after it is removed, scan again to update its sizes."
              : "Data the app keeps in AppData may stay behind after it is removed. It appears here after your next scan."}
          </p>
          {error ? <p className="text-critical">{error}</p> : null}
        </div>

        <DialogFooter>
          {outcome ? (
            <>
              <Button variant="ghost" onClick={onClose}>
                Close
              </Button>
              <Button
                onClick={() => {
                  onRefresh();
                  onClose();
                }}
              >
                <RefreshCw />
                Refresh list
              </Button>
            </>
          ) : (
            <>
              <Button variant="ghost" onClick={onClose} disabled={starting}>
                Cancel
              </Button>
              <Button onClick={() => void start()} disabled={starting}>
                {starting ? <Loader2 className="animate-spin" /> : <ExternalLink />}
                {command ? "Start uninstaller" : "Open Settings"}
              </Button>
            </>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
