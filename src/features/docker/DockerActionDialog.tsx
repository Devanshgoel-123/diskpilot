import { AlertTriangle, Ban, CheckCircle2, Loader2, Terminal, XCircle } from "lucide-react";
import { type CSSProperties, useEffect, useState } from "react";

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
import { formatBytes } from "@/lib/format";
import type { DockerAction, DockerActionPreview, DockerActionResult } from "@/lib/types";
import { cn } from "@/lib/utils";

/** Preview of a predefined Docker action → explicit confirmation → result. */
export function DockerActionDialog({
  action,
  onClose,
  onDone,
}: {
  action: DockerAction;
  onClose: () => void;
  onDone: () => void;
}) {
  const [preview, setPreview] = useState<DockerActionPreview | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [running, setRunning] = useState(false);
  const [result, setResult] = useState<DockerActionResult | null>(null);

  useEffect(() => {
    let live = true;
    api
      .dockerPreview(action)
      .then((p) => live && setPreview(p))
      .catch((e: unknown) => live && setError(errorMessage(e)));
    return () => {
      live = false;
    };
  }, [action]);

  const run = async () => {
    setRunning(true);
    setError(null);
    try {
      setResult(await api.dockerExecute(action));
      onDone();
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setRunning(false);
    }
  };

  const command = preview?.command.join(" ") ?? "";

  return (
    <Dialog open onOpenChange={(o) => !o && !running && onClose()}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            {result ? (
              result.succeeded ? (
                <CheckCircle2 className="size-4 text-good" aria-hidden />
              ) : (
                <XCircle className="size-4 text-critical" aria-hidden />
              )
            ) : null}
            {preview?.title ?? "Preparing…"}
          </DialogTitle>
          <DialogDescription>
            {result
              ? result.succeeded
                ? "Done. Docker's output is shown below."
                : "Docker reported an error."
              : "DiskPilot runs exactly this command, with no shell, after you confirm."}
          </DialogDescription>
        </DialogHeader>

        <div className="min-h-0 flex-1 space-y-4 overflow-y-auto px-5 py-4">
          {!preview && !error ? (
            <p className="flex items-center gap-2 text-xs text-muted-foreground">
              <Loader2 className="size-3.5 animate-spin" aria-hidden /> Checking current Docker
              state…
            </p>
          ) : null}

          {preview ? (
            <>
              <div className="flex items-center gap-2">
                <code className="selectable flex min-w-0 flex-1 items-center gap-2 rounded-md border border-border bg-panel-raised px-3 py-2 font-mono text-xs">
                  <Terminal className="size-3.5 shrink-0 text-subtle-foreground" aria-hidden />
                  <span className="break-all">{command}</span>
                </code>
                <CopyButton text={command} />
              </div>

              {preview.blocked && !result ? (
                <p
                  className="tinted flex gap-2 rounded-lg border px-3 py-2.5 text-xs"
                  style={{ "--tint": "var(--critical)" } as CSSProperties}
                >
                  <Ban className="mt-0.5 size-3.5 shrink-0 text-critical" aria-hidden />
                  {preview.blocked}
                </p>
              ) : null}

              {preview.affected.length ? (
                <div>
                  <p className="mb-1.5 flex justify-between text-xs text-muted-foreground">
                    <span>Affects</span>
                    {preview.reclaimBytes != null ? (
                      <span className="tabular text-foreground">
                        ~{formatBytes(preview.reclaimBytes)} reclaimed
                      </span>
                    ) : null}
                  </p>
                  <ul className="max-h-48 divide-y divide-border overflow-y-auto rounded-lg border border-border">
                    {preview.affected.map((a, i) => (
                      <li key={`${a.label}-${i}`} className="flex items-center gap-3 px-3 py-1.5">
                        <div className="min-w-0 flex-1">
                          <p className="truncate text-xs">{a.label}</p>
                          <p className="truncate text-2xs text-subtle-foreground">{a.detail}</p>
                        </div>
                        {a.bytes != null ? (
                          <span className="tabular shrink-0 text-xs">{formatBytes(a.bytes)}</span>
                        ) : null}
                      </li>
                    ))}
                  </ul>
                </div>
              ) : null}

              {preview.effects.length ? (
                <ul className="list-disc space-y-1 pl-4 text-xs text-muted-foreground">
                  {preview.effects.map((e) => (
                    <li key={e}>{e}</li>
                  ))}
                </ul>
              ) : null}

              {preview.warnings.length ? (
                <ul
                  className="tinted space-y-1 rounded-lg border px-3 py-2.5"
                  style={{ "--tint": "var(--tier-review)" } as CSSProperties}
                >
                  {preview.warnings.map((w) => (
                    <li key={w} className="flex gap-2 text-xs">
                      <AlertTriangle
                        className="mt-0.5 size-3.5 shrink-0"
                        style={{ color: "var(--tier-review)" }}
                        aria-hidden
                      />
                      {w}
                    </li>
                  ))}
                </ul>
              ) : null}
            </>
          ) : null}

          {result ? (
            <pre className="selectable max-h-40 overflow-auto rounded-md border border-border bg-panel-raised p-3 font-mono text-2xs whitespace-pre-wrap">
              {result.error ?? (result.output || "(no output)")}
            </pre>
          ) : null}
          {error ? <p className="text-xs text-critical">{error}</p> : null}
        </div>

        <DialogFooter>
          {result ? (
            <Button onClick={onClose}>Done</Button>
          ) : (
            <>
              <Button variant="ghost" onClick={onClose} disabled={running}>
                Cancel
              </Button>
              <Button
                onClick={() => void run()}
                disabled={!preview || !!preview.blocked || running}
                className={cn(
                  preview?.destructive && "bg-critical text-white hover:bg-critical/90",
                )}
              >
                {running ? <Loader2 className="animate-spin" /> : null}
                {running ? "Running…" : preview?.destructive ? "Confirm removal" : "Confirm"}
              </Button>
            </>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
