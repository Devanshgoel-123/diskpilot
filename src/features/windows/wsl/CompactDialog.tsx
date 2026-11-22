import {
  AlertTriangle,
  ArrowRight,
  Ban,
  CheckCircle2,
  Loader2,
  RotateCcw,
  ShieldCheck,
  Terminal,
  XCircle,
} from "lucide-react";
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
import { Input } from "@/components/ui/input";
import { api, errorMessage } from "@/lib/api";
import { formatBytes } from "@/lib/format";
import { useApp } from "@/lib/store";
import type { CompactMethod, CompactPreview, CompactResult, VirtualDisk } from "@/lib/types";

/** The engine forgets a reviewed compaction after this long. */
export const REVIEW_TTL_MS = 10 * 60 * 1000;

/**
 * Preview of compacting a WSL or Docker virtual disk, or making it sparse →
 * typed confirmation → running (minutes, no cancel) → result. The preview is
 * single-use and expires; if it is refused, the person reviews it again.
 */
export function CompactDialog({
  disk,
  method,
  onClose,
  onDone,
}: {
  disk: VirtualDisk;
  method: CompactMethod;
  onClose: () => void;
  onDone: () => void;
}) {
  // Bumped to ask the engine for a fresh preview.
  const [review, setReview] = useState(0);
  // Each answer remembers the review it belongs to; only the current one is shown.
  const [loaded, setLoaded] = useState<{
    review: number;
    preview: CompactPreview | null;
    error: string | null;
  } | null>(null);
  const current = loaded?.review === review ? loaded : null;
  const preview = current?.preview ?? null;
  const loadError = current?.error ?? null;
  // The reviewed preview can no longer run: it expired or the engine refused it.
  const [stale, setStale] = useState<string | null>(null);
  const [typed, setTyped] = useState("");
  const [running, setRunning] = useState(false);
  const [result, setResult] = useState<CompactResult | null>(null);

  useEffect(() => {
    let live = true;
    api
      .wslCompactPreview(disk.id, method)
      .then((p) => live && setLoaded({ review, preview: p, error: null }))
      .catch((e: unknown) => live && setLoaded({ review, preview: null, error: errorMessage(e) }));
    return () => {
      live = false;
    };
  }, [disk.id, method, review]);

  // Say so as soon as the review expires, rather than when it is refused.
  useEffect(() => {
    if (!preview || preview.blocked || result) return;
    const timer = window.setTimeout(
      () => setStale("This review has expired. Review it again to continue."),
      REVIEW_TTL_MS,
    );
    return () => window.clearTimeout(timer);
  }, [preview, result]);

  const confirmName = preview?.confirmName ?? "";
  const confirmed = !!preview && typed.trim() === confirmName;
  const canRun = !!preview && !preview.blocked && !stale && confirmed && !running;

  const run = async () => {
    if (!preview) return;
    setRunning(true);
    try {
      const r = await api.wslCompactExecute(preview.id, typed);
      setResult(r);
      onDone();
      void useApp.getState().refreshHistory();
      void useApp.getState().refreshDisk();
    } catch (e) {
      // A preview runs once; it must be reviewed again.
      setStale(errorMessage(e));
    } finally {
      setRunning(false);
    }
  };

  const reviewAgain = () => {
    setStale(null);
    setTyped("");
    setReview((n) => n + 1);
  };

  const confirmLabel = method === "sparse" ? "Turn on" : "Compact";
  const title =
    preview?.title ??
    (method === "sparse" ? `Let ${disk.name} shrink by itself` : `Compact ${disk.name}`);

  return (
    <Dialog open onOpenChange={(o) => !o && !running && onClose()}>
      <DialogContent className="w-[min(680px,calc(100vw-32px))]" showClose={!running}>
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            {result ? (
              result.succeeded ? (
                <CheckCircle2 className="size-4 text-good" aria-hidden />
              ) : (
                <XCircle className="size-4 text-critical" aria-hidden />
              )
            ) : null}
            {title}
          </DialogTitle>
          <DialogDescription>
            {result
              ? result.succeeded
                ? "Done. WSL and Docker Desktop can be started again."
                : "The disk could not be compacted. The details are below."
              : running
                ? "Working. A large disk can take several minutes. Keep DiskPilot open and do not start WSL or Docker Desktop until it finishes."
                : "DiskPilot runs exactly these steps, in order, after you confirm. If one fails, the rest do not run."}
          </DialogDescription>
        </DialogHeader>

        <div className="min-h-0 flex-1 space-y-4 overflow-y-auto px-5 py-4">
          {!preview && !loadError ? (
            <p className="flex items-center gap-2 text-xs text-muted-foreground">
              <Loader2 className="size-3.5 animate-spin" aria-hidden /> Checking the disk…
            </p>
          ) : null}
          {loadError ? <p className="text-xs text-critical">{loadError}</p> : null}

          {result ? <ResultView result={result} /> : null}

          {preview && !result ? (
            <>
              {preview.steps.length ? (
                <ol className="space-y-2">
                  {preview.steps.map((s, i) => (
                    <Step key={`${s.label}-${i}`} step={s} n={i + 1} />
                  ))}
                </ol>
              ) : null}

              {preview.blocked ? (
                <p
                  className="tinted flex gap-2 rounded-lg border px-3 py-2.5 text-xs"
                  style={{ "--tint": "var(--critical)" } as CSSProperties}
                >
                  <Ban className="mt-0.5 size-3.5 shrink-0 text-critical" aria-hidden />
                  {preview.blocked}
                </p>
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

              {!preview.blocked ? (
                <p className="flex justify-between text-xs text-muted-foreground">
                  <span>Size on disk now</span>
                  <span className="tabular text-foreground">
                    {formatBytes(preview.beforeBytes)}
                    {preview.estimatedReclaimBytes
                      ? `, about ${formatBytes(preview.estimatedReclaimBytes)} can be returned`
                      : ""}
                  </span>
                </p>
              ) : null}

              {!preview.blocked && !stale && !running ? (
                <label className="flex flex-col gap-1.5 text-xs">
                  <span>
                    Type <span className="selectable font-mono font-medium">{confirmName}</span> to
                    continue.
                  </span>
                  <Input
                    value={typed}
                    onChange={(e) => setTyped(e.target.value)}
                    autoComplete="off"
                    autoCorrect="off"
                    spellCheck={false}
                    aria-label={`Type ${confirmName} to continue`}
                    className="font-mono"
                  />
                </label>
              ) : null}

              {running ? (
                <p
                  className="flex items-center gap-2 text-xs text-muted-foreground"
                  aria-live="polite"
                >
                  <Loader2 className="size-3.5 animate-spin" aria-hidden />
                  {method === "sparse" ? "Changing the disk…" : "Compacting the disk…"}
                </p>
              ) : null}

              {stale ? <p className="text-xs text-critical">{stale}</p> : null}
            </>
          ) : null}
        </div>

        <DialogFooter>
          {result ? (
            <Button onClick={onClose}>Done</Button>
          ) : (
            <>
              <Button variant="ghost" onClick={onClose} disabled={running}>
                Cancel
              </Button>
              {stale || loadError ? (
                <Button onClick={reviewAgain}>
                  <RotateCcw />
                  Review again
                </Button>
              ) : preview?.blocked ? null : (
                <Button onClick={() => void run()} disabled={!canRun}>
                  {running ? <Loader2 className="animate-spin" /> : null}
                  {running ? "Working…" : confirmLabel}
                </Button>
              )}
            </>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function Step({ step, n }: { step: CompactPreview["steps"][number]; n: number }) {
  const command = step.command.join(" ");
  return (
    <li className="space-y-1.5">
      <p className="flex items-center gap-2 text-xs">
        <span className="tabular text-subtle-foreground">{n}.</span>
        <span className="min-w-0 flex-1">{step.label}</span>
        {step.elevated ? (
          <span className="flex shrink-0 items-center gap-1 text-2xs text-muted-foreground">
            <ShieldCheck className="size-3" aria-hidden />
            Needs administrator approval
          </span>
        ) : null}
      </p>
      <div className="flex items-start gap-2">
        <code className="selectable flex min-w-0 flex-1 items-start gap-2 rounded-md border border-border bg-panel-raised px-3 py-2 font-mono text-xs">
          <Terminal className="mt-0.5 size-3.5 shrink-0 text-subtle-foreground" aria-hidden />
          <span className="break-all">{command}</span>
        </code>
        <CopyButton text={command} />
      </div>
      {step.script ? (
        <div>
          <p className="mb-1 text-2xs text-subtle-foreground">
            <span className="font-mono">&lt;script&gt;</span> contains:
          </p>
          <pre className="selectable max-h-40 overflow-auto rounded-md border border-border bg-panel-raised p-3 font-mono text-2xs whitespace-pre-wrap">
            {step.script}
          </pre>
        </div>
      ) : null}
    </li>
  );
}

function ResultView({ result }: { result: CompactResult }) {
  return (
    <div className="space-y-3">
      <div className="flex items-center gap-3 rounded-lg border border-border px-4 py-3">
        <div>
          <p className="text-2xs text-muted-foreground">Before</p>
          <p className="tabular text-[15px] font-semibold">{formatBytes(result.beforeBytes)}</p>
        </div>
        <ArrowRight className="size-3.5 text-subtle-foreground" aria-hidden />
        <div>
          <p className="text-2xs text-muted-foreground">After</p>
          <p className="tabular text-[15px] font-semibold">{formatBytes(result.afterBytes)}</p>
        </div>
        <p className="tabular ml-auto text-[13px]">
          {result.freedBytes > 0 ? `${formatBytes(result.freedBytes)} freed` : "No space was freed"}
        </p>
      </div>
      {result.error ? <p className="text-xs text-critical">{result.error}</p> : null}
      {result.output ? (
        <details className="text-xs">
          <summary className="cursor-pointer text-muted-foreground">Output</summary>
          <pre className="selectable mt-1.5 max-h-40 overflow-auto rounded-md border border-border bg-panel-raised p-3 font-mono text-2xs whitespace-pre-wrap">
            {result.output}
          </pre>
        </details>
      ) : null}
    </div>
  );
}
