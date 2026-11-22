import { Loader2, ShieldCheck, Zap } from "lucide-react";
import { useState } from "react";

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
import { isDriveRoot } from "@/lib/drives";
import { isWindows } from "@/lib/platform";
import { useApp } from "@/lib/store";

/**
 * Windows: explains the fast whole-drive scan, which reads the drive's file
 * table and needs administrator rights, then restarts DiskPilot as
 * administrator once the person confirms. Windows shows its own UAC prompt.
 */
export function FastScanDialog({ onClose }: { onClose: () => void }) {
  const [restarting, setRestarting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const restart = async () => {
    setRestarting(true);
    setError(null);
    try {
      // On success this window closes and the new one takes over.
      await api.relaunchElevated();
    } catch (e) {
      setError(errorMessage(e));
      setRestarting(false);
    }
  };

  return (
    <Dialog open onOpenChange={(o) => !o && !restarting && onClose()}>
      <DialogContent className="w-[min(520px,calc(100vw-32px))]">
        <DialogHeader>
          <DialogTitle>Faster whole-drive scan</DialogTitle>
          <DialogDescription>
            DiskPilot can read the drive&apos;s file table directly instead of opening every folder.
            A whole drive then takes seconds instead of minutes.
          </DialogDescription>
        </DialogHeader>
        <div className="space-y-3 px-5 py-4 text-xs">
          <ul className="list-disc space-y-1 pl-4 text-muted-foreground">
            <li>
              Reading the file table needs administrator approval. DiskPilot restarts, and Windows
              asks you to allow it.
            </li>
            <li>The scan stays read-only. Nothing on the drive is changed.</li>
            <li>
              While DiskPilot runs as administrator, dragging files in from File Explorer may not
              work. Close and reopen DiskPilot to go back.
            </li>
          </ul>
          {error ? <p className="text-critical">{error}</p> : null}
        </div>
        <DialogFooter>
          <Button variant="ghost" onClick={onClose} disabled={restarting}>
            Cancel
          </Button>
          <Button onClick={() => void restart()} disabled={restarting}>
            {restarting ? <Loader2 className="animate-spin" /> : <ShieldCheck />}
            {restarting ? "Waiting for Windows…" : "Restart as administrator"}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

/**
 * Windows: a quiet "Faster whole-drive scan" button, shown when the next scan
 * covers a whole drive and DiskPilot is not administrator. Nothing otherwise.
 */
export function FastScanOffer({ size = "default" }: { size?: "default" | "lg" }) {
  const { drives, scanRoot, elevated, scanning } = useApp();
  const [open, setOpen] = useState(false);
  if (!isWindows() || elevated || scanning || !isDriveRoot(scanRoot, drives)) return null;
  return (
    <>
      <Button variant="ghost" size={size} onClick={() => setOpen(true)}>
        <Zap />
        Faster whole-drive scan
      </Button>
      {open ? <FastScanDialog onClose={() => setOpen(false)} /> : null}
    </>
  );
}
