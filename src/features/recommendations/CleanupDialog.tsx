import { AlertTriangle, CheckCircle2, Loader2, Trash2, XCircle } from "lucide-react";
import { type CSSProperties, useEffect, useMemo, useState } from "react";

import { TierBadge } from "@/components/shared/TierBadge";
import { Pager } from "@/components/shared/Pager";
import { usePage } from "@/lib/usePage";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { api, errorMessage, onCleanupProgress } from "@/lib/api";
import { formatBytes, formatCount, tildify } from "@/lib/format";
import { useApp } from "@/lib/store";
import { TrashGlyph } from "@/features/trash/TrashGlyph";
import { useTrash } from "@/features/trash/trashStore";
import { useCountUp } from "@/features/trash/useCountUp";
import { playCleanupDone } from "@/lib/sounds";
import type { CleanupMethod, CleanupPlan, CleanupProgress, CleanupRecord } from "@/lib/types";
import { cn } from "@/lib/utils";

import "./cleanup.css";

const METHODS: { id: CleanupMethod; title: string; text: string }[] = [
  {
    id: "trash",
    title: "Move to Trash",
    text: "Recoverable until you empty the Trash. Space is freed when you do.",
  },
  {
    id: "delete",
    title: "Delete permanently",
    text: "Frees space now. Cannot be undone. Only offered for regenerable data.",
  },
];

/**
 * Preview → explicit confirmation → result. The plan (exact paths) comes from
 * the engine; executing it is single-use and re-validated by the engine.
 */
export function CleanupDialog({
  open,
  ids,
  allowedMethods,
  onOpenChange,
  onFinished,
  createPlan,
}: {
  open: boolean;
  ids: string[];
  /** Builds the plan instead of planning recommendation `ids`. Keep it stable. */
  createPlan?: (method: CleanupMethod) => Promise<CleanupPlan>;
  allowedMethods: CleanupMethod[];
  onOpenChange: (open: boolean) => void;
  onFinished: (record: CleanupRecord) => void;
}) {
  const home = useApp((s) => s.env?.homeDir);
  const [method, setMethod] = useState<CleanupMethod>("trash");
  const [plan, setPlan] = useState<CleanupPlan | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [confirmed, setConfirmed] = useState(false);
  const [running, setRunning] = useState(false);
  const [record, setRecord] = useState<CleanupRecord | null>(null);
  const [progress, setProgress] = useState<CleanupProgress | null>(null);

  useEffect(() => {
    if (!open || record) return;
    let live = true;
    let created: string | null = null;
    (createPlan ? createPlan(method) : api.createCleanupPlan(ids, method))
      .then((p) => {
        created = p.id;
        if (live) {
          setPlan(p);
          setError(null);
          setConfirmed(false);
        } else {
          void api.discardCleanupPlan(p.id);
        }
      })
      .catch((e: unknown) => live && setError(errorMessage(e)));
    return () => {
      live = false;
      // A plan that was not executed is discarded when it is replaced or closed.
      if (created) void api.discardCleanupPlan(created).catch(() => undefined);
    };
  }, [open, ids, method, record, createPlan]);

  const loading = !plan || plan.method !== method;
  const reviewPage = usePage(plan?.items ?? [], 50, plan?.id ?? null);

  const execute = async () => {
    if (!plan) return;
    setRunning(true);
    setError(null);
    setProgress(null);
    const planId = plan.id;
    const unlisten = await onCleanupProgress((p) => {
      if (p.planId === planId) setProgress(p);
    }).catch(() => null);
    try {
      const result = await api.executeCleanupPlan(planId);
      setRecord(result);
      onFinished(result);
      if (result.items.some((i) => i.status === "succeeded")) playCleanupDone();
      // Space cleared and streaks count every cleanup, from any page.
      void useApp.getState().refreshHistory();
      void useApp.getState().refreshDisk();
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      unlisten?.();
      setRunning(false);
    }
  };

  const close = (next: boolean) => {
    if (running) return;
    onOpenChange(next);
  };

  const count = plan?.items.length ?? 0;
  const destructiveLabel =
    method === "trash"
      ? `Move ${count} item${count === 1 ? "" : "s"} to Trash`
      : `Delete ${count} item${count === 1 ? "" : "s"} permanently`;

  return (
    <Dialog open={open} onOpenChange={close}>
      <DialogContent className="w-[min(720px,calc(100vw-32px))]">
        {record ? (
          <ResultView record={record} home={home} onClose={() => close(false)} />
        ) : (
          <>
            <DialogHeader>
              <DialogTitle>Review cleanup</DialogTitle>
              <DialogDescription>
                These are the exact paths DiskPilot will act on. Nothing happens until you confirm.
              </DialogDescription>
            </DialogHeader>

            <div className="min-h-0 flex-1 space-y-4 overflow-y-auto px-5 py-4">
              <div role="radiogroup" aria-label="Cleanup method" className="grid grid-cols-2 gap-2">
                {METHODS.map((m) => {
                  const allowed = allowedMethods.includes(m.id);
                  return (
                    <button
                      key={m.id}
                      type="button"
                      role="radio"
                      aria-checked={method === m.id}
                      disabled={!allowed || running}
                      onClick={() => setMethod(m.id)}
                      className={cn(
                        "rounded-lg border p-3 text-left transition-colors disabled:cursor-not-allowed disabled:opacity-40",
                        method === m.id
                          ? "border-accent bg-accent-soft"
                          : "border-border-strong hover:bg-hover",
                      )}
                    >
                      <p className="text-[13px] font-medium">{m.title}</p>
                      <p className="mt-0.5 text-2xs leading-snug text-muted-foreground">
                        {allowed ? m.text : "Not available for every selected item."}
                      </p>
                    </button>
                  );
                })}
              </div>

              {plan && !loading && plan.warnings.length ? (
                <ul
                  className="tinted space-y-1 rounded-lg border px-3 py-2.5"
                  style={{ "--tint": "var(--tier-review)" } as CSSProperties}
                >
                  {plan.warnings.map((w) => (
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

              {running && plan ? (
                <Working plan={plan} progress={progress} />
              ) : loading ? (
                <p className="flex items-center gap-2 py-6 text-xs text-muted-foreground">
                  <Loader2 className="size-3.5 animate-spin" aria-hidden /> Preparing preview…
                </p>
              ) : plan ? (
                <div>
                  <p className="mb-1.5 flex items-baseline justify-between text-xs text-muted-foreground">
                    <span>
                      {count} item{count === 1 ? "" : "s"}
                    </span>
                    <span className="tabular text-foreground">{formatBytes(plan.totalBytes)}</span>
                  </p>
                  <ul
                    aria-label="Items in this cleanup"
                    className="divide-y divide-border rounded-lg border border-border"
                  >
                    {reviewPage.items.map((i) => (
                      <li key={i.path}>
                        <div className="flex items-center gap-3 px-3 py-2">
                          <div className="min-w-0 flex-1">
                            <p className="flex items-center gap-2 truncate text-xs">
                              <span className="truncate">{i.title}</span>
                              <TierBadge tier={i.tier} />
                            </p>
                            <p
                              className="selectable mt-0.5 font-mono text-2xs break-all text-subtle-foreground"
                              title={i.path}
                            >
                              {tildify(i.path, home)}
                            </p>
                          </div>
                          <span className="tabular shrink-0 text-xs">{formatBytes(i.bytes)}</span>
                        </div>
                      </li>
                    ))}
                  </ul>
                  <Pager page={reviewPage} label="items" className="mt-2" />
                  {plan.rejected.length ? (
                    <div className="mt-3">
                      <p className="mb-1 text-xs text-muted-foreground">
                        Excluded by the safety checks
                      </p>
                      <ul className="space-y-1">
                        {plan.rejected.map((r) => (
                          <li key={r.recommendationId} className="text-2xs text-subtle-foreground">
                            <span className="font-mono">{tildify(r.path, home)}</span> — {r.reason}
                          </li>
                        ))}
                      </ul>
                    </div>
                  ) : null}
                </div>
              ) : null}
              {error ? <p className="text-xs text-critical">{error}</p> : null}
            </div>

            <DialogFooter className="justify-between">
              <label className="flex items-center gap-2 text-xs text-muted-foreground">
                <input
                  type="checkbox"
                  checked={confirmed}
                  onChange={(e) => setConfirmed(e.target.checked)}
                  disabled={loading || count === 0 || running}
                  className="size-3.5 accent-[var(--accent)]"
                />
                I reviewed these paths
              </label>
              <div className="flex gap-2">
                <Button variant="ghost" onClick={() => close(false)} disabled={running}>
                  Cancel
                </Button>
                <Button
                  onClick={() => void execute()}
                  disabled={!confirmed || loading || count === 0 || running}
                  className={cn(
                    method === "delete" && "bg-critical text-white hover:bg-critical/90",
                  )}
                >
                  {running ? <Loader2 className="animate-spin" /> : <Trash2 />}
                  {running
                    ? progress
                      ? `${formatCount(progress.itemsDone)} of ${formatCount(progress.itemsTotal)}…`
                      : "Starting…"
                    : destructiveLabel}
                </Button>
              </div>
            </DialogFooter>
          </>
        )}
      </DialogContent>
    </Dialog>
  );
}

/** Live progress while the engine works through the plan. */
function Working({ plan, progress }: { plan: CleanupPlan; progress: CleanupProgress | null }) {
  const total = plan.items.length;
  const done = progress?.itemsDone ?? 0;
  const bytes = useCountUp(progress?.bytesDone ?? 0, 420);
  const fraction = total ? done / total : 0;
  const verb = plan.method === "trash" ? "Moving to the Trash" : "Deleting";
  return (
    <div className="cl-working py-4" aria-live="polite">
      <p className="flex items-baseline gap-2">
        <span className="tabular text-[28px] leading-8 font-semibold tracking-[-0.02em]">
          {formatBytes(bytes)}
        </span>
        <span className="tabular text-xs text-muted-foreground">
          of {formatBytes(plan.totalBytes)} {plan.method === "trash" ? "moved" : "freed"}
        </span>
      </p>
      <div
        role="progressbar"
        aria-label={verb}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={Math.round(fraction * 100)}
        className="cl-track mt-3 h-1.5 overflow-hidden rounded-full bg-track"
      >
        <div
          className="cl-bar h-full rounded-full bg-accent"
          style={{ transform: `scaleX(${fraction})` }}
        />
      </div>
      <p className="mt-2 flex items-center justify-between gap-3 text-xs text-muted-foreground">
        <span className="min-w-0 truncate">
          {progress?.current ? `${verb}: ${progress.current}` : `${verb}…`}
        </span>
        <span className="tabular shrink-0">
          {formatCount(done)} of {formatCount(total)}
        </span>
      </p>
      <p className="mt-4 text-2xs text-subtle-foreground">
        Each batch is checked again just before it is touched. Keep DiskPilot open until it
        finishes.
      </p>
    </div>
  );
}

function ResultView({
  record,
  home,
  onClose,
}: {
  record: CleanupRecord;
  home: string | undefined;
  onClose: () => void;
}) {
  const ok = record.items.filter((i) => i.status === "succeeded").length;
  // Anything not done comes first, so it is seen without scrolling.
  const items = useMemo(
    () => [
      ...record.items.filter((i) => i.status !== "succeeded"),
      ...record.items.filter((i) => i.status === "succeeded"),
    ],
    [record],
  );
  const resultPage = usePage(items, 50, record.id);
  return (
    <>
      <DialogHeader>
        <DialogTitle className="flex items-center gap-2">
          {record.status === "failed" ? (
            <XCircle className="size-4 text-critical" aria-hidden />
          ) : (
            <CheckCircle2 className="size-4 text-good" aria-hidden />
          )}
          {record.status === "succeeded"
            ? "Cleanup complete"
            : record.status === "partial"
              ? "Cleanup partly complete"
              : "Cleanup failed"}
        </DialogTitle>
        <DialogDescription>
          {formatCount(ok)} of {formatCount(record.items.length)} items done.{" "}
          {record.bytesTrashed > 0
            ? `${formatBytes(record.bytesTrashed)} moved to the Trash. Empty the Trash to free the space.`
            : null}
          {record.bytesFreed > 0 ? `${formatBytes(record.bytesFreed)} freed.` : null} Sizes
          elsewhere reflect the last scan until you rescan.
        </DialogDescription>
      </DialogHeader>
      <div className="min-h-0 flex-1 overflow-y-auto px-5 py-2">
        <ul aria-label="Cleanup results" className="divide-y divide-border">
          {resultPage.items.map((i) => (
            <li key={i.target}>
              <div className="py-2">
                <p className="flex items-center gap-2 text-xs">
                  {i.status === "succeeded" ? (
                    <CheckCircle2 className="size-3.5 shrink-0 text-good" aria-hidden />
                  ) : (
                    <XCircle className="size-3.5 shrink-0 text-critical" aria-hidden />
                  )}
                  <span className="selectable truncate font-mono text-2xs" title={i.target}>
                    {tildify(i.target, home)}
                  </span>
                  <span className="tabular ml-auto shrink-0">{formatBytes(i.bytes)}</span>
                </p>
                {i.error ? (
                  <p className="mt-0.5 truncate pl-5 text-2xs text-critical" title={i.error}>
                    {i.error}
                  </p>
                ) : null}
              </div>
            </li>
          ))}
        </ul>
        <Pager page={resultPage} label="results" className="mt-2" />
      </div>
      <DialogFooter>
        {record.bytesTrashed > 0 ? (
          <Button
            variant="secondary"
            className="group"
            onClick={() => {
              onClose();
              useTrash.getState().openDialog();
            }}
          >
            <TrashGlyph />
            Empty Trash…
          </Button>
        ) : null}
        <Button onClick={onClose}>Done</Button>
      </DialogFooter>
    </>
  );
}
