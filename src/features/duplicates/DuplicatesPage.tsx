import { ChevronDown, Copy, Eye, FolderOpen, Layers, Loader2, Search, Square } from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";

import { PageHeader } from "@/components/layout/PageHeader";
import { EmptyState } from "@/components/shared/EmptyState";
import { FilePreview } from "@/components/shared/FilePreview";
import { FileThumbnail } from "@/components/shared/FileThumbnail";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { CleanupDialog } from "@/features/recommendations/CleanupDialog";
import { clearThumbnails } from "@/lib/thumbnails";
import { api, errorMessage, onDuplicatesProgress } from "@/lib/api";
import { formatBytes, formatCount, formatRelative, tildify } from "@/lib/format";
import { useApp } from "@/lib/store";
import type {
  CleanupRecord,
  DuplicateGroup,
  DuplicateProgress,
  DuplicateReport,
} from "@/lib/types";
import { cn } from "@/lib/utils";
import { basename, parentOf, separatorOf } from "@/lib/paths";
import { isWindows, words } from "@/lib/platform";

const MIN_SIZES = [
  { bytes: 1024 * 1024, label: "1 MB" },
  { bytes: 10 * 1024 * 1024, label: "10 MB" },
  { bytes: 100 * 1024 * 1024, label: "100 MB" },
];

/** The copy the user keeps in each group: their choice, else the suggestion. */
function keptPath(group: DuplicateGroup, keep: Record<string, string>): string {
  return keep[group.id] ?? group.files.find((f) => f.keep)?.path ?? group.files[0]?.path ?? "";
}

function Progress({ progress }: { progress: DuplicateProgress | null }) {
  const home = useApp((s) => s.env?.homeDir);
  const comparing = progress?.phase === "comparing" && progress.bytesToHash > 0;
  const pct = comparing ? Math.min(100, (progress.bytesHashed / progress.bytesToHash) * 100) : 0;
  return (
    <div className="mx-auto w-full max-w-md px-6 py-16" aria-live="polite">
      <p className="text-[13px] font-medium">
        {comparing ? "Comparing files with the same size…" : "Looking through your files…"}
      </p>
      <div className="mt-2 h-[3px] overflow-hidden rounded-full bg-track">
        {comparing ? (
          <div
            className="h-full rounded-full bg-accent transition-[width] duration-150"
            style={{ width: `${pct}%` }}
          />
        ) : (
          <div className="scan-indeterminate h-full w-1/3 rounded-full bg-accent" />
        )}
      </div>
      <p className="tabular mt-1.5 text-xs text-muted-foreground">
        {formatCount(progress?.filesExamined ?? 0)} files
        {comparing
          ? ` · ${formatBytes(progress.bytesHashed)} of ${formatBytes(progress.bytesToHash)} read`
          : ""}
      </p>
      <p className="mt-0.5 truncate font-mono text-2xs text-subtle-foreground">
        {progress?.current ? tildify(progress.current, home) : "Starting"}
      </p>
    </div>
  );
}

function GroupRow({
  group,
  kept,
  selected,
  open,
  onToggleOpen,
  onKeep,
  onToggle,
  onToggleGroup,
  onPreview,
}: {
  group: DuplicateGroup;
  kept: string;
  selected: Set<string>;
  open: boolean;
  onToggleOpen: () => void;
  onKeep: (path: string) => void;
  onToggle: (path: string) => void;
  onToggleGroup: () => void;
  /** Open the large preview at this copy. */
  onPreview: (index: number) => void;
}) {
  const home = useApp((s) => s.env?.homeDir);
  const extras = group.files.filter((f) => f.path !== kept);
  const chosen = extras.filter((f) => selected.has(f.path)).length;
  return (
    <li className="border-b border-border">
      <div className="flex items-center gap-3 px-4 py-2">
        <input
          type="checkbox"
          checked={chosen === extras.length}
          ref={(el) => {
            if (el) el.indeterminate = chosen > 0 && chosen < extras.length;
          }}
          onChange={onToggleGroup}
          aria-label={`Select extra copies of ${group.name}`}
          className="size-3.5 shrink-0 accent-[var(--accent)]"
        />
        <button
          type="button"
          onClick={() =>
            onPreview(
              Math.max(
                0,
                group.files.findIndex((f) => f.path === kept),
              ),
            )
          }
          aria-label={`Preview ${group.name}`}
          title="Preview (Space)"
          className="shrink-0 rounded-[4px] focus-visible:outline-2 focus-visible:outline-[var(--accent)]"
        >
          <FileThumbnail path={kept} size={32} />
        </button>
        <button
          type="button"
          onClick={onToggleOpen}
          onKeyDown={(e) => {
            if (e.key === " ") {
              e.preventDefault();
              onPreview(
                Math.max(
                  0,
                  group.files.findIndex((f) => f.path === kept),
                ),
              );
            }
          }}
          aria-expanded={open}
          className="flex min-w-0 flex-1 items-center gap-2 text-left"
        >
          <ChevronDown
            className={cn(
              "size-3.5 shrink-0 text-subtle-foreground transition-transform duration-150",
              !open && "-rotate-90",
            )}
            aria-hidden
          />
          <span className="min-w-0 flex-1">
            <span className="block truncate text-[13px]">{group.name}</span>
            <span className="block truncate text-2xs text-subtle-foreground">
              {group.files.length} copies of {formatBytes(group.bytes)}
              {group.sharesStorage
                ? isWindows()
                  ? " · some share storage"
                  : " · some share storage (APFS clones)"
                : ""}
            </span>
          </span>
        </button>
        {group.sharesStorage && group.reclaimableBytes === 0 ? (
          <span
            className="shrink-0 text-xs text-subtle-foreground"
            title="These copies share the same storage, so removing one frees no space"
          >
            No space to free
          </span>
        ) : (
          <span className="tabular shrink-0 text-xs" title="Space freed by keeping one copy">
            {formatBytes(group.reclaimableBytes)}
          </span>
        )}
      </div>
      {open ? (
        <ul className="pr-4 pb-2 pl-[52px]">
          {group.files.map((f, i) => {
            const isKept = f.path === kept;
            return (
              <li
                key={f.path}
                tabIndex={0}
                onKeyDown={(e) => {
                  if (e.key === " " && e.target === e.currentTarget) {
                    e.preventDefault();
                    onPreview(i);
                  }
                }}
                className="flex items-center gap-3 rounded-md px-2 py-1 hover:bg-hover focus-visible:outline-2 focus-visible:outline-[var(--accent)]"
              >
                <input
                  type="checkbox"
                  checked={selected.has(f.path)}
                  disabled={isKept}
                  onChange={() => onToggle(f.path)}
                  aria-label={`Move ${tildify(f.path, home)} to ${words().trash}`}
                  className="size-3.5 shrink-0 accent-[var(--accent)] disabled:opacity-30"
                />
                <button
                  type="button"
                  tabIndex={-1}
                  onClick={() => onPreview(i)}
                  aria-label={`Preview ${tildify(f.path, home)}`}
                  className="shrink-0"
                >
                  <FileThumbnail path={f.path} size={24} />
                </button>
                <span className="selectable min-w-0 flex-1 truncate font-mono text-2xs text-muted-foreground">
                  {tildify(parentOf(f.path), home)}
                  {separatorOf(f.path)}
                  <span className="text-foreground">{basename(f.path)}</span>
                </span>
                <span className="w-24 shrink-0 text-2xs text-subtle-foreground">
                  {formatRelative(f.modifiedMs)}
                </span>
                {isKept ? (
                  <span className="w-16 shrink-0 text-right text-2xs font-medium whitespace-nowrap text-[var(--tier-safe)]">
                    Keep
                  </span>
                ) : (
                  <button
                    type="button"
                    onClick={() => onKeep(f.path)}
                    className="w-16 shrink-0 text-right text-2xs whitespace-nowrap text-muted-foreground hover:text-foreground"
                  >
                    Keep this
                  </button>
                )}
                <button
                  type="button"
                  onClick={() => onPreview(i)}
                  aria-label="Preview"
                  title="Preview (Space)"
                  className="shrink-0 text-subtle-foreground hover:text-foreground"
                >
                  <Eye className="size-3.5" aria-hidden />
                </button>
                <button
                  type="button"
                  onClick={() => void api.revealInFinder(f.path)}
                  aria-label={words().showInFileManager}
                  title={words().showInFileManager}
                  className="shrink-0 text-subtle-foreground hover:text-foreground"
                >
                  <FolderOpen className="size-3.5" aria-hidden />
                </button>
              </li>
            );
          })}
        </ul>
      ) : null}
    </li>
  );
}

export function DuplicatesPage() {
  const env = useApp((s) => s.env);
  const summary = useApp((s) => s.summary);
  const home = env?.homeDir;
  const [report, setReport] = useState<DuplicateReport | null>(null);
  const [running, setRunning] = useState(false);
  const [progress, setProgress] = useState<DuplicateProgress | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [minBytes, setMinBytes] = useState(1024 * 1024);
  const [query, setQuery] = useState("");
  const [keep, setKeep] = useState<Record<string, string>>({});
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [open, setOpen] = useState<Set<string>>(new Set());
  const [dialog, setDialog] = useState(false);
  const [preview, setPreview] = useState<{ group: DuplicateGroup; index: number } | null>(null);

  useEffect(() => {
    void api.getDuplicates().then((r) => r && setReport(r));
    const un = onDuplicatesProgress(setProgress);
    return () => void un.then((f) => f());
  }, []);

  const search = async () => {
    setRunning(true);
    setError(null);
    setProgress(null);
    try {
      const r = await api.findDuplicates(minBytes);
      clearThumbnails();
      setReport(r);
      setKeep({});
      setSelected(new Set());
      setOpen(new Set(r.groups.slice(0, 3).map((g) => g.id)));
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setRunning(false);
    }
  };

  const groups = useMemo(() => {
    const q = query.trim().toLowerCase();
    const all = report?.groups ?? [];
    return q ? all.filter((g) => g.files.some((f) => f.path.toLowerCase().includes(q))) : all;
  }, [report, query]);

  const chosen = useMemo(() => [...selected], [selected]);
  const chosenBytes = useMemo(() => {
    let total = 0;
    for (const g of report?.groups ?? []) {
      if (g.sharesStorage) continue;
      total += g.files.filter((f) => selected.has(f.path)).length * g.bytes;
    }
    return total;
  }, [report, selected]);

  const toggle = (path: string) =>
    setSelected((s) => {
      const next = new Set(s);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });

  const toggleGroup = (g: DuplicateGroup) =>
    setSelected((s) => {
      const extras = g.files.map((f) => f.path).filter((p) => p !== keptPath(g, keep));
      const next = new Set(s);
      const all = extras.every((p) => next.has(p));
      for (const p of extras) {
        if (all) next.delete(p);
        else next.add(p);
      }
      return next;
    });

  const selectAllExtras = () =>
    setSelected(
      new Set(
        groups.flatMap((g) => g.files.map((f) => f.path).filter((p) => p !== keptPath(g, keep))),
      ),
    );

  const setKept = (g: DuplicateGroup, path: string) => {
    setKeep((k) => ({ ...k, [g.id]: path }));
    setSelected((s) => {
      const next = new Set(s);
      next.delete(path);
      return next;
    });
  };

  const createPlan = useCallback(() => api.planDuplicateCleanup(chosen), [chosen]);

  const finished = (record: CleanupRecord) => {
    const gone = new Set(record.items.filter((i) => i.status === "succeeded").map((i) => i.target));
    setSelected((s) => new Set([...s].filter((p) => !gone.has(p))));
    void api.getDuplicates().then((r) => r && setReport(r));
  };

  const root = report?.root ?? summary?.root ?? home ?? "";
  const actions = running ? (
    <Button variant="outline" onClick={() => void api.cancelFindDuplicates()}>
      <Square className="fill-current" aria-hidden /> Stop
    </Button>
  ) : (
    <>
      <label className="flex items-center gap-1.5 text-xs text-muted-foreground">
        At least
        <select
          value={minBytes}
          onChange={(e) => setMinBytes(Number(e.target.value))}
          className="h-7 rounded-md border border-border-strong bg-panel px-1.5 text-xs text-foreground"
        >
          {MIN_SIZES.map((s) => (
            <option key={s.bytes} value={s.bytes}>
              {s.label}
            </option>
          ))}
        </select>
      </label>
      <Button variant={report ? "outline" : "default"} onClick={() => void search()}>
        <Layers aria-hidden /> {report ? "Search again" : "Find duplicates"}
      </Button>
    </>
  );

  return (
    <>
      <PageHeader
        title="Duplicates"
        description={`Files with identical contents in ${tildify(root, home)}. You choose which copy stays.`}
        actions={actions}
      />
      {error ? (
        <p role="alert" className="border-b border-border px-6 py-2 text-xs text-critical">
          {error}
        </p>
      ) : null}
      {running ? (
        <Progress progress={progress} />
      ) : !report ? (
        <EmptyState
          className="flex-1 justify-center"
          icon={Copy}
          title="Find files you have more than once"
          description={
            isWindows()
              ? "DiskPilot compares contents, not names: files match only when every byte is the same. Hidden folders, dependency folders and package caches are skipped, and files kept online only by OneDrive are never downloaded."
              : "DiskPilot compares contents, not names: files match only when every byte is the same. Hidden folders, ~/Library, app bundles, dependency folders and package caches are skipped."
          }
          action={
            <Button onClick={() => void search()}>
              <Layers aria-hidden /> Find duplicates
            </Button>
          }
        />
      ) : report.groups.length === 0 ? (
        <EmptyState
          className="flex-1 justify-center"
          icon={Copy}
          title={report.cancelled ? "Search stopped" : "No duplicates found"}
          description={`Looked at ${formatCount(report.filesExamined)} files of at least ${formatBytes(report.minBytes)}.`}
        />
      ) : (
        <div className="flex min-h-0 flex-1 flex-col">
          <div className="flex shrink-0 items-center gap-3 border-b border-border px-4 py-2">
            <p className="tabular text-xs text-muted-foreground">
              {formatCount(report.groups.length)} sets · {formatBytes(report.reclaimableBytes)} in
              extra copies
              {report.cancelled ? " · search stopped early" : ""}
              {report.truncated ? " · largest sets shown" : ""}
            </p>
            <div className="relative ml-auto w-56">
              <Search
                className="pointer-events-none absolute top-1/2 left-2 size-3.5 -translate-y-1/2 text-subtle-foreground"
                aria-hidden
              />
              <Input
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                placeholder="Filter by name or path"
                className="pl-7"
                aria-label="Filter by name or path"
              />
            </div>
            <Button variant="ghost" onClick={selectAllExtras}>
              Select extra copies
            </Button>
          </div>
          <ul className="min-h-0 flex-1 overflow-auto">
            {groups.map((g) => (
              <GroupRow
                key={g.id}
                group={g}
                kept={keptPath(g, keep)}
                selected={selected}
                open={open.has(g.id)}
                onToggleOpen={() =>
                  setOpen((o) => {
                    const next = new Set(o);
                    if (next.has(g.id)) next.delete(g.id);
                    else next.add(g.id);
                    return next;
                  })
                }
                onKeep={(p) => setKept(g, p)}
                onToggle={toggle}
                onToggleGroup={() => toggleGroup(g)}
                onPreview={(index) => setPreview({ group: g, index })}
              />
            ))}
          </ul>
          <div className="flex shrink-0 items-center gap-3 border-t border-border bg-panel px-4 py-2.5">
            <p className="text-xs text-muted-foreground">
              {chosen.length
                ? `${formatCount(chosen.length)} ${chosen.length === 1 ? "copy" : "copies"} selected · about ${formatBytes(chosenBytes)} freed once the ${words().trash} is emptied`
                : "Select the copies you no longer need. One copy in every set always stays."}
            </p>
            <Button
              className="ml-auto"
              disabled={chosen.length === 0}
              onClick={() => setDialog(true)}
            >
              {running ? <Loader2 className="animate-spin" aria-hidden /> : null}
              Review {chosen.length || ""} cop{chosen.length === 1 ? "y" : "ies"}…
            </Button>
          </div>
        </div>
      )}
      <FilePreview
        files={(preview?.group.files ?? []).map((f) => ({
          path: f.path,
          bytes: preview?.group.bytes,
          modifiedMs: f.modifiedMs,
          note:
            f.path === (preview ? keptPath(preview.group, keep) : "")
              ? "Keeping this copy"
              : selected.has(f.path)
                ? `Selected to move to ${words().trash}`
                : undefined,
        }))}
        index={preview?.index ?? null}
        onIndexChange={(index) => setPreview((p) => (p ? { ...p, index } : p))}
        onClose={() => setPreview(null)}
      />
      {dialog ? (
        <CleanupDialog
          open={dialog}
          ids={chosen}
          allowedMethods={["trash"]}
          createPlan={createPlan}
          onOpenChange={setDialog}
          onFinished={finished}
        />
      ) : null}
    </>
  );
}
