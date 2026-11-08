import { ChevronDown, Info, Loader2, Lock, RotateCcw } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";

import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "@/components/ui/dialog";
import { api, errorMessage, onTrashProgress } from "@/lib/api";
import { formatBytes, formatCount } from "@/lib/format";
import { useApp } from "@/lib/store";
import type { TrashPreview, TrashProgress, TrashResult } from "@/lib/types";
import { cn } from "@/lib/utils";
import { isWindows, words } from "@/lib/platform";

import "./trash.css";
import { entryIcon } from "./fileKind";
import { TrashGlyph } from "./TrashGlyph";
import { type StageMood, TrashStage } from "./TrashStage";
import { STAGE_CHIPS, useTrash } from "./trashStore";
import { prefersReducedMotion, useCountUp } from "./useCountUp";

type Phase = "loading" | "denied" | "empty" | "confirm" | "working" | "result" | "error";

const PREVIEW_ROWS = 4;
/** How the engine words an entry it left because the user stopped (trash.rs). */
const STOPPED_PREFIX = "Not removed: emptying was stopped";

const plural = (n: number, one: string, many = `${one}s`) =>
  `${formatCount(n)} ${n === 1 ? one : many}`;

/**
 * Empty Trash, start to finish: the Trash is listed as a single-use preview,
 * the user confirms, and the engine permanently removes exactly those entries
 * while reporting progress. Nothing is removed before "Empty Trash" is
 * clicked, and the result shown is the engine's own record.
 */
export function EmptyTrashDialog() {
  const session = useTrash((s) => s.session);
  return <TrashFlow key={session} />;
}

function TrashFlow() {
  const { open, setOpen, refresh } = useTrash();
  const disk = useApp((s) => s.disk);

  const [phase, setPhase] = useState<Phase>("loading");
  const [preview, setPreview] = useState<TrashPreview | null>(null);
  const [progress, setProgress] = useState<TrashProgress | null>(null);
  const [result, setResult] = useState<TrashResult | null>(null);
  const [error, setError] = useState<{ title: string; message: string } | null>(null);
  const [fallen, setFallen] = useState(0);
  const [stopping, setStopping] = useState(false);
  const [settled, setSettled] = useState(false);
  const [showAll, setShowAll] = useState(false);
  const executed = useRef(false);
  const cancelRef = useRef<HTMLButtonElement>(null);

  const [attempt, setAttempt] = useState(0);
  useEffect(() => {
    if (!open) return;
    let live = true;
    api
      .previewTrash()
      .then((p) => {
        if (!live) return void api.discardTrashPreview(p.id).catch(() => {});
        setPreview(p);
        setPhase(p.access === "denied" ? "denied" : p.entries.length === 0 ? "empty" : "confirm");
      })
      .catch((e: unknown) => {
        if (!live) return;
        setError({
          title: `DiskPilot couldn't read the ${words().trash}`,
          message: errorMessage(e),
        });
        setPhase("error");
      });
    return () => {
      live = false;
    };
  }, [open, attempt]);

  /** List the Trash again from scratch (after a result, an error or granting access). */
  const load = () => {
    setPhase("loading");
    setPreview(null);
    setProgress(null);
    setResult(null);
    setError(null);
    setFallen(0);
    setStopping(false);
    setSettled(false);
    setShowAll(false);
    executed.current = false;
    setAttempt((n) => n + 1);
  };

  // Progress events for this preview only.
  useEffect(() => {
    if (!open || !preview) return;
    const id = preview.id;
    const off = onTrashProgress((p) => p.previewId === id && setProgress(p));
    return () => void off.then((f) => f());
  }, [open, preview]);

  const chips = Math.min(STAGE_CHIPS, preview?.entries.length ?? 0);
  // Cards drop one at a time, paced so each landing reads, but never ahead of
  // what the engine has actually finished.
  const handled = result ? chips : Math.min(progress?.entriesDone ?? 0, chips);
  useEffect(() => {
    if (fallen >= handled) return;
    const t = setTimeout(
      () => setFallen((f) => f + 1),
      prefersReducedMotion() ? 0 : fallen === 0 ? 120 : 150,
    );
    return () => clearTimeout(t);
  }, [fallen, handled]);

  // Once the last card has landed the lid closes, then the result shows.
  useEffect(() => {
    if (phase !== "working" || !result || fallen < chips) return;
    const reduced = prefersReducedMotion();
    const lid = setTimeout(() => setSettled(true), reduced ? 0 : 560);
    const show = setTimeout(() => setPhase("result"), reduced ? 0 : 1080);
    return () => {
      clearTimeout(lid);
      clearTimeout(show);
    };
  }, [phase, result, fallen, chips]);

  const start = async () => {
    if (!preview) return;
    executed.current = true;
    setPhase("working");
    setProgress({
      previewId: preview.id,
      entriesDone: 0,
      entriesTotal: preview.entries.length,
      bytesRemoved: 0,
      filesRemoved: 0,
      current: preview.entries[0]?.name ?? null,
      notRemoved: [],
    });
    try {
      const r = await api.emptyTrash(preview.id);
      setResult(r);
      void refresh();
      const app = useApp.getState();
      void app.refreshDisk();
      void app.refreshHistory();
    } catch (e) {
      setError({ title: `The ${words().trash} was not emptied`, message: errorMessage(e) });
      setPhase("error");
    }
  };

  const stop = () => {
    setStopping(true);
    void api.cancelEmptyTrash();
  };

  const busy = phase === "working";
  const close = (next: boolean) => {
    if (busy || next) return;
    if (preview && !executed.current) void api.discardTrashPreview(preview.id).catch(() => {});
    setOpen(false);
    void refresh();
  };

  const notRemoved = useMemo(() => {
    if (result) {
      return new Set(
        result.record.items.flatMap((it, i) => (it.status === "succeeded" ? [] : [i])),
      );
    }
    return new Set(progress?.notRemoved ?? []);
  }, [result, progress]);

  const outcome = result ? outcomeOf(result) : null;
  const mood: StageMood =
    phase === "working"
      ? "working"
      : phase === "result"
        ? outcome === "done"
          ? "done"
          : "attention"
        : phase === "denied"
          ? "locked"
          : phase === "empty" || phase === "loading"
            ? "empty"
            : phase === "error"
              ? "attention"
              : "confirm";
  const lidOpen = phase === "working" && !settled;

  return (
    <Dialog open={open} onOpenChange={close}>
      <DialogContent
        showClose={!busy}
        overlayClassName="trash-overlay"
        className="trash-dialog w-[min(480px,calc(100vw-32px))] overflow-hidden"
        onEscapeKeyDown={(e) => busy && e.preventDefault()}
        onInteractOutside={(e) => busy && e.preventDefault()}
        onOpenAutoFocus={(e) => {
          e.preventDefault();
          cancelRef.current?.focus();
        }}
      >
        <TrashStage
          mood={mood}
          entries={phase === "empty" || phase === "denied" ? [] : (preview?.entries ?? [])}
          fallen={phase === "confirm" ? 0 : fallen}
          notRemoved={notRemoved}
          lidOpen={lidOpen}
        />

        <div key={phase} className="tr-swap min-h-0 flex-1 overflow-y-auto px-6 pt-5 pb-5">
          {phase === "loading" ? <Loading /> : null}
          {phase === "confirm" && preview ? (
            <Confirm preview={preview} showAll={showAll} onShowAll={() => setShowAll(true)} />
          ) : null}
          {phase === "working" && preview ? (
            <Working preview={preview} progress={progress} stopping={stopping} />
          ) : null}
          {phase === "result" && result && preview ? (
            <Result result={result} preview={preview} available={disk?.availableBytes ?? null} />
          ) : null}
          {phase === "denied" ? <Denied /> : null}
          {phase === "empty" ? <Empty /> : null}
          {phase === "error" && error ? <ErrorView {...error} /> : null}
        </div>

        <footer className="flex items-center justify-end gap-2 border-t border-border px-6 py-3">
          {phase === "confirm" ? (
            <>
              <Button ref={cancelRef} variant="secondary" onClick={() => close(false)}>
                Cancel
              </Button>
              <Button variant="destructive" className="group" onClick={() => void start()}>
                <TrashGlyph />
                {words().emptyTrash}
              </Button>
            </>
          ) : phase === "working" ? (
            <Button ref={cancelRef} variant="secondary" onClick={stop} disabled={stopping}>
              {stopping ? "Stopping…" : "Stop"}
            </Button>
          ) : phase === "denied" ? (
            <>
              <Button variant="ghost" onClick={() => load()}>
                <RotateCcw />
                Check again
              </Button>
              <Button ref={cancelRef} variant="secondary" onClick={() => close(false)}>
                Close
              </Button>
              {isWindows() ? null : (
                <Button onClick={() => void api.openFullDiskAccessSettings()}>
                  Open System Settings
                </Button>
              )}
            </>
          ) : phase === "error" ? (
            <>
              <Button ref={cancelRef} variant="secondary" onClick={() => close(false)}>
                Close
              </Button>
              <Button onClick={() => load()}>
                <RotateCcw />
                Review again
              </Button>
            </>
          ) : phase === "result" && outcome !== "done" ? (
            <>
              <Button variant="secondary" onClick={() => load()}>
                <RotateCcw />
                Review what's left
              </Button>
              <Button ref={cancelRef} onClick={() => close(false)}>
                Done
              </Button>
            </>
          ) : (
            <Button ref={cancelRef} onClick={() => close(false)} disabled={phase === "loading"}>
              Done
            </Button>
          )}
        </footer>

        <p className="sr-only" role="status" aria-live="polite">
          {phase === "working"
            ? `Emptying the ${words().trash}.`
            : phase === "result" && result
              ? `${resultTitle(result)} ${formatBytes(result.record.bytesFreed)} reclaimed.`
              : ""}
        </p>
      </DialogContent>
    </Dialog>
  );
}

type Outcome = "done" | "partial" | "stopped" | "failed";

function outcomeOf(r: TrashResult): Outcome {
  if (r.record.status === "succeeded") return "done";
  if (r.cancelled) return "stopped";
  return r.record.status === "partial" ? "partial" : "failed";
}

function resultTitle(r: TrashResult): string {
  switch (outcomeOf(r)) {
    case "done":
      return "All clear.";
    case "stopped":
      return "Stopped.";
    case "partial":
      return "Some items couldn't be removed.";
    case "failed":
      return "Nothing could be removed.";
  }
}

// ---- Bodies ----------------------------------------------------------------

function Loading() {
  return (
    <>
      <DialogTitle className="text-[17px]">Looking in the {words().trash}…</DialogTitle>
      <DialogDescription className="mt-1 flex items-center gap-2 text-[13px]">
        <Loader2 className="size-3.5 animate-spin" aria-hidden />
        Measuring every item so you can see exactly what will be deleted.
      </DialogDescription>
    </>
  );
}

function Stat({ value, label, strong }: { value: string; label: string; strong?: boolean }) {
  return (
    <div className="min-w-0">
      <p
        className={cn(
          "tabular text-[28px] leading-8 font-semibold tracking-[-0.02em]",
          !strong && "text-muted-foreground",
        )}
      >
        {value}
      </p>
      <p className="mt-0.5 text-xs text-muted-foreground">{label}</p>
    </div>
  );
}

function Confirm({
  preview,
  showAll,
  onShowAll,
}: {
  preview: TrashPreview;
  showAll: boolean;
  onShowAll: () => void;
}) {
  const n = preview.entries.length;
  const rows = showAll ? preview.entries : preview.entries.slice(0, PREVIEW_ROWS);
  const rest = preview.entries.slice(rows.length);
  const restBytes = rest.reduce((s, e) => s + e.bytes, 0);
  return (
    <>
      <DialogTitle className="text-[17px]">Empty the {words().trash}?</DialogTitle>
      <DialogDescription className="mt-1 text-[13px] leading-relaxed">
        {n === 1 ? "This item will be" : `These ${formatCount(n)} items will be`} deleted
        permanently. This can't be undone.
      </DialogDescription>

      <div className="mt-4 grid grid-cols-2 gap-4">
        <Stat value={formatBytes(preview.totalBytes)} label="of storage to reclaim" strong />
        <Stat
          value={formatCount(preview.totalFiles)}
          label={`${preview.totalFiles === 1 ? "file" : "files"} in ${plural(n, "item")}`}
        />
      </div>

      <ul
        aria-label={`Items in the ${words().trash}`}
        className={cn(
          "mt-4 divide-y divide-border rounded-lg border border-border",
          showAll && "max-h-52 overflow-y-auto",
        )}
      >
        {rows.map((e) => {
          const Icon = entryIcon(e);
          return (
            <li key={e.name} className="flex items-center gap-2.5 px-3 py-2">
              <Icon className="size-3.5 shrink-0 text-muted-foreground" aria-hidden />
              <span className="selectable min-w-0 flex-1 truncate text-[13px]" title={e.name}>
                {e.name}
              </span>
              {e.isDir ? (
                <span className="tabular shrink-0 text-xs text-subtle-foreground">
                  {plural(e.files, "file")}
                </span>
              ) : null}
              <span className="tabular w-16 shrink-0 text-right text-xs">
                {formatBytes(e.bytes)}
              </span>
            </li>
          );
        })}
        {rest.length ? (
          <li>
            <button
              type="button"
              onClick={onShowAll}
              className="flex w-full items-center gap-2.5 px-3 py-2 text-left text-xs text-muted-foreground transition-colors hover:bg-hover hover:text-foreground"
            >
              <ChevronDown className="size-3.5" aria-hidden />
              <span className="flex-1">{plural(rest.length, "more item")}</span>
              <span className="tabular w-16 text-right">{formatBytes(restBytes)}</span>
            </button>
          </li>
        ) : null}
      </ul>

      <p className="mt-3 flex gap-2 text-xs leading-relaxed text-muted-foreground">
        <Info className="mt-0.5 size-3.5 shrink-0" aria-hidden />
        Only the items listed here are deleted. Anything you move to the {words().trash} after this
        is left alone.
      </p>
    </>
  );
}

function Working({
  preview,
  progress,
  stopping,
}: {
  preview: TrashPreview;
  progress: TrashProgress | null;
  stopping: boolean;
}) {
  const bytes = useCountUp(progress?.bytesRemoved ?? 0, 420);
  const fraction =
    preview.totalBytes > 0
      ? Math.min(1, (progress?.bytesRemoved ?? 0) / preview.totalBytes)
      : (progress?.entriesDone ?? 0) / Math.max(1, preview.entries.length);
  return (
    <>
      <DialogTitle className="text-[17px]">
        {stopping ? "Stopping after the current file…" : `Emptying the ${words().trash}…`}
      </DialogTitle>
      <DialogDescription className="mt-1 text-[13px]">
        Deleting one file at a time. Stopping leaves anything not yet deleted in the {words().trash}
        .
      </DialogDescription>

      <p className="mt-4 flex items-baseline gap-2">
        <span className="tabular text-[28px] leading-8 font-semibold tracking-[-0.02em]">
          {formatBytes(bytes)}
        </span>
        <span className="tabular text-xs text-muted-foreground">
          of {formatBytes(preview.totalBytes)} reclaimed
        </span>
      </p>

      <div
        role="progressbar"
        aria-label={`Emptying the ${words().trash}`}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={Math.round(fraction * 100)}
        className="mt-3 h-1 overflow-hidden rounded-full bg-track"
      >
        <div
          className="tr-progress h-full rounded-full bg-accent"
          style={{ transform: `scaleX(${fraction})` }}
        />
      </div>

      <p className="mt-2 flex items-center justify-between gap-3 text-xs text-muted-foreground">
        <span className="min-w-0 truncate">
          {progress?.current ? `Deleting “${progress.current}”` : "Finishing…"}
        </span>
        <span className="tabular shrink-0">
          {formatCount(progress?.filesRemoved ?? 0)} of {formatCount(preview.totalFiles)} files
        </span>
      </p>
    </>
  );
}

function Result({
  result,
  preview,
  available,
}: {
  result: TrashResult;
  preview: TrashPreview;
  available: number | null;
}) {
  const outcome = outcomeOf(result);
  const bytes = useCountUp(result.record.bytesFreed, 900);
  const files = useCountUp(result.filesRemoved, 900);
  const issues = result.record.items
    .map((it, i) => ({ ...it, name: preview.entries[i]?.name ?? it.target }))
    .filter((it) => it.status !== "succeeded");
  const left = issues.length;
  // Items left because the user stopped are covered by the sentence above;
  // the list is for the ones that need attention.
  const problems = issues.filter((it) => !it.error?.startsWith(STOPPED_PREFIX));

  return (
    <>
      <DialogTitle className="text-[17px]">{resultTitle(result)}</DialogTitle>
      <p className="mt-3 flex items-baseline gap-2">
        <span className="tabular text-[32px] leading-9 font-semibold tracking-[-0.025em]">
          {formatBytes(bytes)}
        </span>
        <span className="text-[13px] text-muted-foreground">of storage reclaimed</span>
      </p>
      <DialogDescription className="mt-1 text-[13px] leading-relaxed">
        {plural(Math.round(files), "file")} {outcome === "done" ? "successfully " : ""}removed.
        {outcome !== "done"
          ? ` ${plural(left, "item")} ${left === 1 ? "is" : "are"} still in the ${words().trash}.`
          : ""}
        {available != null && outcome === "done"
          ? ` ${formatBytes(available)} is now available on this disk.`
          : ""}
      </DialogDescription>

      {problems.length ? (
        <ul
          aria-label="Items not removed"
          className="mt-4 max-h-44 divide-y divide-border overflow-y-auto rounded-lg border border-border"
        >
          {problems.map((it) => (
            <li key={it.target} className="px-3 py-2">
              <p className="flex items-center gap-2 text-[13px]">
                <span className="selectable min-w-0 flex-1 truncate" title={it.name}>
                  {it.name}
                </span>
                {it.bytes > 0 ? (
                  <span className="tabular shrink-0 text-xs text-muted-foreground">
                    {formatBytes(it.bytes)} removed
                  </span>
                ) : null}
              </p>
              <p className="mt-0.5 text-xs leading-snug text-muted-foreground">{it.error}</p>
            </li>
          ))}
        </ul>
      ) : null}

      {result.permissionDenied ? (
        <p className="mt-3 flex gap-2 text-xs leading-relaxed text-muted-foreground">
          <Lock className="mt-0.5 size-3.5 shrink-0" aria-hidden />
          {isWindows()
            ? "Some files are in use, locked, or belong to another user or to Windows. DiskPilot leaves them as they are. Close the apps using them and try again, or empty the Recycle Bin from File Explorer, which asks for permission if it is needed."
            : "Some files are locked, or belong to another user or to macOS. DiskPilot leaves them as they are. Finder can remove them: open the Trash in Finder and choose Empty, and it will ask for your password if one is needed."}
        </p>
      ) : null}
    </>
  );
}

function Denied() {
  return (
    <>
      <DialogTitle className="text-[17px]">
        DiskPilot can&apos;t see inside the {words().trash}
      </DialogTitle>
      <DialogDescription className="mt-1 text-[13px] leading-relaxed">
        {isWindows()
          ? "Windows did not let DiskPilot list the Recycle Bin. Choose Check again, or empty it from File Explorer or the desktop."
          : "macOS keeps the Trash private. To empty it here, turn on DiskPilot in System Settings, Privacy & Security, Full Disk Access, then choose Check again. You can also empty the Trash from Finder."}
      </DialogDescription>
    </>
  );
}

function Empty() {
  return (
    <>
      <DialogTitle className="text-[17px]">The {words().trash} is empty</DialogTitle>
      <DialogDescription className="mt-1 text-[13px] leading-relaxed">
        There's nothing to delete. Items you clean up in DiskPilot go to the {words().trash} first
        and will show up here.
      </DialogDescription>
    </>
  );
}

function ErrorView({ title, message }: { title: string; message: string }) {
  return (
    <>
      <DialogTitle className="text-[17px]">{title}</DialogTitle>
      <DialogDescription className="selectable mt-1 text-[13px] leading-relaxed">
        {message}
      </DialogDescription>
    </>
  );
}
