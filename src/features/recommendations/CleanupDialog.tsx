import { AlertTriangle, CheckCircle2, Loader2, Trash2, XCircle } from "lucide-react";
import { type CSSProperties, useEffect, useState } from "react";

import { TierBadge } from "@/components/shared/TierBadge";
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
import { formatBytes, tildify } from "@/lib/format";
import { useApp } from "@/lib/store";
import { TrashGlyph } from "@/features/trash/TrashGlyph";
import { useTrash } from "@/features/trash/trashStore";
import type { CleanupMethod, CleanupPlan, CleanupRecord } from "@/lib/types";
import { cn } from "@/lib/utils";
import { words } from "@/lib/platform";

const methods = (): { id: CleanupMethod; title: string; text: string }[] => [
  {
    id: "trash",
    title: `Move to ${words().trash}`,
    text: `Recoverable until you empty the ${words().trash}. Space is freed when you do.`,
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

  const execute = async () => {
    if (!plan) return;
    setRunning(true);
    setError(null);
    try {
      const result = await api.executeCleanupPlan(plan.id);
      setRecord(result);
      onFinished(result);
      // Space cleared and streaks count every cleanup, from any page.
      void useApp.getState().refreshHistory();
      void useApp.getState().refreshDisk();
    } catch (e) {
      setError(errorMessage(e));
    } finally {
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
      ? `Move ${count} item${count === 1 ? "" : "s"} to ${words().trash}`
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
                {methods().map((m) => {
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

              {loading ? (
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
                  <ul className="divide-y divide-border rounded-lg border border-border">
                    {plan.items.map((i) => (
                      <li key={i.path} className="flex items-center gap-3 px-3 py-2">
                        <div className="min-w-0 flex-1">
                          <p className="flex items-center gap-2 truncate text-xs">
                            <span className="truncate">{i.title}</span>
                            <TierBadge tier={i.tier} />
                          </p>
                          <p className="selectable mt-0.5 font-mono text-2xs break-all text-subtle-foreground">
                            {tildify(i.path, home)}
                          </p>
                        </div>
                        <span className="tabular shrink-0 text-xs">{formatBytes(i.bytes)}</span>
                      </li>
                    ))}
                  </ul>
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
                  {running ? "Working…" : destructiveLabel}
                </Button>
              </div>
            </DialogFooter>
          </>
        )}
      </DialogContent>
    </Dialog>
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
          {ok} of {record.items.length} items done.{" "}
          {record.bytesTrashed > 0
            ? `${formatBytes(record.bytesTrashed)} moved to the ${words().trash}. Empty the ${words().trash} to free the space.`
            : null}
          {record.bytesFreed > 0 ? `${formatBytes(record.bytesFreed)} freed.` : null} Sizes
          elsewhere reflect the last scan until you rescan.
        </DialogDescription>
      </DialogHeader>
      <ul className="min-h-0 flex-1 divide-y divide-border overflow-y-auto px-5 py-2">
        {record.items.map((i) => (
          <li key={i.target} className="py-2">
            <p className="flex items-center gap-2 text-xs">
              {i.status === "succeeded" ? (
                <CheckCircle2 className="size-3.5 shrink-0 text-good" aria-hidden />
              ) : (
                <XCircle className="size-3.5 shrink-0 text-critical" aria-hidden />
              )}
              <span className="selectable truncate font-mono text-2xs">
                {tildify(i.target, home)}
              </span>
              <span className="tabular ml-auto shrink-0">{formatBytes(i.bytes)}</span>
            </p>
            {i.error ? <p className="mt-0.5 pl-5 text-2xs text-critical">{i.error}</p> : null}
          </li>
        ))}
      </ul>
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
            {words().emptyTrash}…
          </Button>
        ) : null}
        <Button onClick={onClose}>Done</Button>
      </DialogFooter>
    </>
  );
}
