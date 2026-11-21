import { ExternalLink, Loader2, RefreshCw, Terminal, Trash2 } from "lucide-react";
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

/** What the dialog says for each outcome, and before anything happens. */
function wording(app: AppFootprint, command: string | null, outcome: UninstallOutcome | null) {
  switch (outcome) {
    case "uninstaller":
      return {
        title: "Finish in the uninstaller",
        text: `The uninstaller for ${app.name} has started. Follow its steps, then refresh the list.`,
      };
    case "removed":
      return {
        title: `${app.name} was uninstalled`,
        text: `Windows removed ${app.name} for your account. Refresh the list to update it.`,
      };
    case "settingsApp":
      return {
        title: "Finish in Settings",
        text: `Windows did not let DiskPilot remove ${app.name} directly, so Settings is open on its own page. Choose Uninstall there if Windows offers it, then refresh the list.`,
      };
    case "settings":
      return {
        title: "Finish in Settings",
        text: `Settings is open at Installed apps, and the name is copied. Press Ctrl+V in its search box to find ${app.name}, choose Uninstall, then refresh the list here.`,
      };
    default:
      return {
        title: `Uninstall ${app.name}`,
        text: command
          ? "DiskPilot starts the program's own uninstaller. It may ask questions of its own. DiskPilot does not delete any files itself."
          : app.storeApp
            ? "Windows removes this app for your account, the same as Uninstall in Settings. If Windows does not allow it here, Settings opens on this app's page instead."
            : "This program can only be removed from Settings. DiskPilot opens Settings > Installed apps and copies the app's name, so you can paste it into the search box.",
      };
  }
}

/**
 * Windows: starts an app's own uninstaller; removes a Store app through
 * Windows (or opens its page in Settings when Windows declines); otherwise
 * opens Settings > Apps with the name copied for its search box. DiskPilot
 * never deletes a program's files itself.
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

  const say = wording(app, command, outcome);

  const start = async () => {
    setStarting(true);
    setError(null);
    // Settings has no link to one desktop program; the name goes on the
    // clipboard so its search box finds the app with a paste.
    if (!command && !app.storeApp) {
      await navigator.clipboard?.writeText(app.name).catch(() => undefined);
    }
    try {
      const result = await api.uninstallApp(app.bundleId);
      setOutcome(result);
      // Gone already: update the list behind the dialog right away.
      if (result === "removed") onRefresh();
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
          <DialogTitle>{say.title}</DialogTitle>
          <DialogDescription>{say.text}</DialogDescription>
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
                {starting ? (
                  <Loader2 className="animate-spin" />
                ) : app.storeApp && !command ? (
                  <Trash2 />
                ) : (
                  <ExternalLink />
                )}
                {command
                  ? "Start uninstaller"
                  : app.storeApp
                    ? starting
                      ? "Uninstalling…"
                      : "Uninstall"
                    : "Open Settings"}
              </Button>
            </>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
