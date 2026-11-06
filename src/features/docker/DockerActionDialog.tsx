import { AlertTriangle, Ban, CheckCircle2, Loader2, Terminal, XCircle } from "lucide-react";
import { type CSSProperties, useEffect, useState } from "react";

import { CopyButton } from "@/components/shared/CopyButton";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
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
  action: initial,
  onClose,
  onDone,
}: {
  action: DockerAction;
  onClose: () => void;
  onDone: () => void;
}) {
  // Ticking a volume changes the action, and the preview is rebuilt for it.
  const [action, setAction] = useState<DockerAction>(initial);
  const [preview, setPreview] = useState<DockerActionPreview | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [running, setRunning] = useState(false);
  const [result, setResult] = useState<DockerActionResult | null>(null);
  const ticked = action.type === "removeContainer" ? (action.volumes ?? []) : [];
  const stopsFirst = preview?.steps[0]?.command[1] === "stop" && action.type === "removeContainer";
  // Deleting a volume cannot be undone, so a name must be typed to confirm:
  // the volume's own, or the container's when volumes go with it.
  const mustType =
    action.type === "removeVolume"
      ? action.name
      : ticked.length
        ? (preview?.affected[0]?.label ?? null)
        : null;
  const [typed, setTyped] = useState("");
  const confirmed = mustType === null || typed.trim() === mustType;
  const confirmLabel =
    action.type === "removeVolume"
      ? "Delete volume"
      : action.type === "stopContainer"
        ? "Stop container"
        : action.type === "removeContainer"
          ? `${stopsFirst ? "Stop and remove" : "Remove container"}${
              ticked.length ? ` + ${ticked.length} volume${ticked.length === 1 ? "" : "s"}` : ""
            }`
          : preview?.destructive
            ? "Confirm removal"
            : "Confirm";

  const toggleVolume = (name: string) => {
    if (action.type !== "removeContainer") return;
    const next = ticked.includes(name) ? ticked.filter((v) => v !== name) : [...ticked, name];
    setTyped("");
    setAction({ ...action, volumes: next });
  };

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

  const commands = preview?.steps.map((s) => s.command.join(" ")) ?? [];

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
              : commands.length > 1
                ? "DiskPilot runs exactly these commands, in order, with no shell, after you confirm. If one fails, the rest do not run."
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
              <div className="flex items-start gap-2">
                <code className="selectable flex min-w-0 flex-1 flex-col gap-1 rounded-md border border-border bg-panel-raised px-3 py-2 font-mono text-xs">
                  {commands.map((c) => (
                    <span key={c} className="flex items-start gap-2">
                      <Terminal
                        className="mt-0.5 size-3.5 shrink-0 text-subtle-foreground"
                        aria-hidden
                      />
                      <span className="break-all">{c}</span>
                    </span>
                  ))}
                </code>
                <CopyButton text={commands.join("\n")} />
              </div>

              {preview.volumeChoices.length && !result ? (
                <fieldset>
                  <legend className="mb-1.5 text-xs text-muted-foreground">
                    Volumes it uses. Tick any you also want deleted; the rest are kept.
                  </legend>
                  <ul className="divide-y divide-border rounded-lg border border-border">
                    {preview.volumeChoices.map((v) => (
                      <li key={v.name}>
                        <label
                          className={cn(
                            "flex items-center gap-3 px-3 py-1.5",
                            v.keptBecause ? "opacity-60" : "cursor-pointer",
                          )}
                        >
                          <input
                            type="checkbox"
                            checked={ticked.includes(v.name)}
                            disabled={!!v.keptBecause || running}
                            onChange={() => toggleVolume(v.name)}
                            className="size-3.5 shrink-0 accent-[var(--accent)]"
                          />
                          <span className="min-w-0 flex-1">
                            <span className="block truncate font-mono text-xs">{v.name}</span>
                            <span className="block truncate text-2xs text-subtle-foreground">
                              {v.keptBecause ??
                                (v.composeProject
                                  ? `Compose project “${v.composeProject}”`
                                  : "Only this container uses it")}
                            </span>
                          </span>
                          {v.bytes != null ? (
                            <span className="tabular shrink-0 text-xs">{formatBytes(v.bytes)}</span>
                          ) : null}
                        </label>
                      </li>
                    ))}
                  </ul>
                </fieldset>
              ) : null}

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
          {mustType && preview && !preview.blocked && !result ? (
            <label className="flex flex-col gap-1.5 text-xs">
              <span>
                Type <span className="selectable font-mono font-medium">{mustType}</span> to
                confirm. Volume data cannot be recovered.
              </span>
              <Input
                value={typed}
                onChange={(e) => setTyped(e.target.value)}
                autoComplete="off"
                autoCorrect="off"
                spellCheck={false}
                aria-label={`Type ${mustType} to confirm`}
                className="font-mono"
              />
            </label>
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
                disabled={!preview || !!preview.blocked || running || !confirmed}
                className={cn(
                  preview?.destructive && "bg-critical text-white hover:bg-critical/90",
                )}
              >
                {running ? <Loader2 className="animate-spin" /> : null}
                {running ? "Running…" : confirmLabel}
              </Button>
            </>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
