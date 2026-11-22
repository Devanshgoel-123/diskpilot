import {
  Archive,
  Eye,
  FileText,
  Film,
  Image as ImageIcon,
  type LucideIcon,
  Music,
  Package,
  Search,
  Shapes,
} from "lucide-react";
import { useCallback, useMemo, useRef, useState } from "react";
import { useShallow } from "zustand/react/shallow";

import { PageHeader } from "@/components/layout/PageHeader";
import { EmptyState } from "@/components/shared/EmptyState";
import { FilePreview } from "@/components/shared/FilePreview";
import { FileThumbnail } from "@/components/shared/FileThumbnail";
import { Pager } from "@/components/shared/Pager";
import { usePage, useScrollTopOnPage } from "@/lib/usePage";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Skeleton } from "@/components/ui/skeleton";
import { CleanupDialog } from "@/features/recommendations/CleanupDialog";
import { StorageTabs } from "@/features/storage/StorageTabs";
import { api } from "@/lib/api";
import { formatBytes, formatCount, formatRelative, tildify } from "@/lib/format";
import { listedNote } from "./listedNote";
import { useKindFiles } from "./useKindFiles";
import { useApp } from "@/lib/store";
import type { CleanupRecord, FileEntry, FileKind } from "@/lib/types";
import { cn } from "@/lib/utils";
import { parentOf, relativeParts } from "@/lib/paths";
import { isWindows, words } from "@/lib/platform";

const KINDS: Record<FileKind, { label: string; icon: LucideIcon }> = {
  videos: { label: "Videos", icon: Film },
  images: { label: "Images", icon: ImageIcon },
  audio: { label: "Audio", icon: Music },
  archives: { label: "Archives", icon: Archive },
  installers: { label: "Installers and disk images", icon: Package },
  documents: { label: "Documents", icon: FileText },
};

const PAGE_SIZE = 100;
const DAY = 24 * 60 * 60 * 1000;
const AGES = [
  { id: "any", label: "Any age", olderThan: 0 },
  { id: "6m", label: "Not changed in 6 months", olderThan: 182 * DAY },
  { id: "1y", label: "Not changed in a year", olderThan: 365 * DAY },
] as const;
type AgeId = (typeof AGES)[number]["id"];

/** The first folder under home, e.g. "Downloads", or "" for files at home. */
function topFolder(path: string, home: string | undefined): string {
  const rel = home ? relativeParts(path, home) : null;
  return rel && rel.length > 1 ? (rel[0] ?? "") : "";
}

/**
 * Personal files grouped by type, to review, preview and move to the Trash.
 * Keyed by scan, so a new scan starts with nothing selected.
 */
export function FileKindsPage() {
  const scan = useApp((s) => s.summary?.startedAtMs ?? 0);
  return <FileKindsView key={scan} />;
}

function FileKindsView() {
  const { summary, env, scanning, startScan } = useApp(
    useShallow((s) => ({
      summary: s.summary,
      env: s.env,
      scanning: s.scanning,
      startScan: s.startScan,
    })),
  );
  const home = env?.homeDir;
  const groups = useMemo(() => summary?.fileKinds ?? [], [summary]);
  const [picked, setPicked] = useState<FileKind | null>(null);
  const kind = picked && groups.some((g) => g.kind === picked) ? picked : (groups[0]?.kind ?? null);
  const [query, setQuery] = useState("");
  const [age, setAge] = useState<AgeId>("any");
  const [folder, setFolder] = useState("");
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [gone, setGone] = useState<Set<string>>(new Set());
  const [preview, setPreview] = useState<number | null>(null);
  const [dialog, setDialog] = useState(false);
  // Captured when the page first renders, so the age filter is stable.
  const [now] = useState(() => Date.now());

  const group = groups.find((g) => g.kind === kind) ?? null;
  // Files of the types shown so far; the current type's arrive a moment
  // after it is picked.
  const loaded = useKindFiles(summary, kind);
  const groupFiles = kind ? loaded.get(kind) : undefined;
  const files: FileEntry[] = useMemo(
    () => (groupFiles ?? []).filter((f) => !gone.has(f.path)),
    [groupFiles, gone],
  );
  const folders = useMemo(
    () => [...new Set(files.map((f) => topFolder(f.path, home)).filter(Boolean))].sort(),
    [files, home],
  );

  const visible = useMemo(() => {
    const q = query.trim().toLowerCase();
    const olderThan = AGES.find((a) => a.id === age)?.olderThan ?? 0;
    return files.filter(
      (f) =>
        (!q || f.path.toLowerCase().includes(q)) &&
        (!folder || topFolder(f.path, home) === folder) &&
        (!olderThan || (f.modifiedMs !== null && now - f.modifiedMs > olderThan)),
    );
  }, [files, query, age, folder, home, now]);

  // 100 rows a page: a page renders at once, and the rest wait until asked for.
  const page = usePage(visible, PAGE_SIZE, `${kind}|${query}|${age}|${folder}`);
  const listRef = useRef<HTMLDivElement>(null);
  useScrollTopOnPage(listRef, page.page);

  const sizes = useMemo(
    () => new Map([...loaded.values()].flatMap((fs) => fs.map((f) => [f.path, f.bytes] as const))),
    [loaded],
  );
  // Only files the current scan still lists and that were not removed.
  const chosen = useMemo(
    () => [...selected].filter((p) => sizes.has(p) && !gone.has(p)),
    [selected, sizes, gone],
  );
  // Totals per type, less what was moved to the Trash from this page.
  const removed = useMemo(() => {
    const out = new Map<FileKind, { bytes: number; count: number }>();
    for (const g of groups) {
      const r = (loaded.get(g.kind) ?? []).filter((f) => gone.has(f.path));
      out.set(g.kind, { bytes: r.reduce((a, f) => a + f.bytes, 0), count: r.length });
    }
    return out;
  }, [groups, loaded, gone]);
  const shownPaths = useMemo(() => new Set(visible.map((f) => f.path)), [visible]);
  const hiddenChosen = chosen.filter((p) => !shownPaths.has(p)).length;
  const chosenBytes = chosen.reduce((a, p) => a + (sizes.get(p) ?? 0), 0);
  const allVisibleChosen = visible.length > 0 && visible.every((f) => selected.has(f.path));

  const toggle = (path: string) =>
    setSelected((s) => {
      const next = new Set(s);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });
  const toggleVisible = () =>
    setSelected((s) => {
      const next = new Set(s);
      for (const f of visible) {
        if (allVisibleChosen) next.delete(f.path);
        else next.add(f.path);
      }
      return next;
    });

  const createPlan = useCallback(() => api.planFileKindCleanup(chosen), [chosen]);
  const finished = (record: CleanupRecord) => {
    const done = record.items.filter((i) => i.status === "succeeded").map((i) => i.target);
    setGone((g) => new Set([...g, ...done]));
    const gone = new Set(done);
    setSelected((s) => new Set([...s].filter((p) => !gone.has(p))));
  };

  const header = (
    <PageHeader
      title="Storage"
      actions={<StorageTabs />}
      description={`Your own files by type, outside apps, ${isWindows() ? "" : "~/Library and "}hidden folders. DiskPilot never marks these safe to delete: you decide.`}
    />
  );

  if (!summary) {
    return (
      <>
        {header}
        <EmptyState
          className="flex-1 justify-center"
          icon={Shapes}
          title={scanning ? "Scanning…" : "No scan yet"}
          description="Videos, images, installers and other files appear here, grouped by type, after a scan."
          action={scanning ? null : <Button onClick={() => void startScan()}>Start scan</Button>}
        />
      </>
    );
  }

  if (groups.length === 0) {
    return (
      <>
        {header}
        <EmptyState
          className="flex-1 justify-center"
          icon={Shapes}
          title="Scan again to group files by type"
          description="This scan was made by an older version of DiskPilot, or found no videos, images, audio, archives, installers or documents."
          action={scanning ? null : <Button onClick={() => void startScan()}>Scan again</Button>}
        />
      </>
    );
  }

  return (
    <>
      {header}
      <div className="flex min-h-0 flex-1">
        <nav
          aria-label="File types"
          className="flex w-60 shrink-0 flex-col gap-0.5 border-r border-border p-2"
        >
          {groups.map((g) => {
            const meta = KINDS[g.kind];
            const Icon = meta.icon;
            return (
              <button
                key={g.kind}
                type="button"
                aria-current={g.kind === kind ? "true" : undefined}
                onClick={() => {
                  setPicked(g.kind);
                  setFolder("");
                  setPreview(null);
                }}
                className={cn(
                  "flex items-center gap-2.5 rounded-md px-2.5 py-2 text-left hover:bg-hover",
                  g.kind === kind && "bg-accent-soft hover:bg-accent-soft",
                )}
              >
                <Icon className="size-4 shrink-0 text-muted-foreground" aria-hidden />
                <span className="min-w-0 flex-1">
                  <span className="block truncate text-[13px] font-medium">{meta.label}</span>
                  <span className="tabular block text-2xs text-muted-foreground">
                    {formatCount(g.fileCount - (removed.get(g.kind)?.count ?? 0))} files
                  </span>
                </span>
                <span className="tabular text-xs">
                  {formatBytes(g.bytes - (removed.get(g.kind)?.bytes ?? 0))}
                </span>
              </button>
            );
          })}
        </nav>
        <div className="flex min-w-0 flex-1 flex-col">
          <div className="flex shrink-0 flex-wrap items-center gap-2 border-b border-border px-4 py-2">
            <div className="relative w-56">
              <Search
                className="pointer-events-none absolute top-1/2 left-2 size-3.5 -translate-y-1/2 text-subtle-foreground"
                aria-hidden
              />
              <Input
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                placeholder="Filter by name or path"
                data-filter
                className="pl-7"
                aria-label="Filter by name or path"
              />
            </div>
            <select
              value={folder}
              onChange={(e) => setFolder(e.target.value)}
              aria-label="Folder"
              className="h-7 rounded-md border border-border-strong bg-panel px-1.5 text-xs"
            >
              <option value="">All folders</option>
              {folders.map((f) => (
                <option key={f} value={f}>
                  {f}
                </option>
              ))}
            </select>
            <select
              value={age}
              onChange={(e) => setAge(e.target.value as AgeId)}
              aria-label="Age"
              className="h-7 rounded-md border border-border-strong bg-panel px-1.5 text-xs"
            >
              {AGES.map((a) => (
                <option key={a.id} value={a.id}>
                  {a.label}
                </option>
              ))}
            </select>
            <Button
              variant="ghost"
              className="ml-auto"
              disabled={!visible.length}
              onClick={toggleVisible}
            >
              {allVisibleChosen ? "Clear selection" : `Select all ${formatCount(visible.length)}`}
            </Button>
          </div>
          {group ? (
            <div className="flex shrink-0 items-center gap-3 border-b border-border/60 px-4 py-1.5">
              <p className="min-w-0 flex-1 text-2xs text-muted-foreground">
                {listedNote(group.listed, group.fileCount)}
              </p>
              <Pager page={page} label="files" />
            </div>
          ) : null}
          <div ref={listRef} className="min-h-0 flex-1 overflow-auto">
            {visible.length ? (
              <ul aria-label={group ? KINDS[group.kind].label : undefined}>
                {page.items.map((f, j) => {
                  const i = page.start + j;
                  return (
                    <li key={f.path} className="h-10">
                      <div
                        tabIndex={0}
                        onKeyDown={(e) => {
                          if (e.key === " " && e.target === e.currentTarget) {
                            e.preventDefault();
                            setPreview(i);
                          }
                        }}
                        className="flex h-full items-center gap-3 border-b border-border/60 px-4 outline-none hover:bg-hover/60 focus-visible:bg-hover"
                      >
                        <input
                          type="checkbox"
                          checked={selected.has(f.path)}
                          onChange={() => toggle(f.path)}
                          aria-label={`Select ${f.name}`}
                          className="size-3.5 shrink-0 accent-[var(--accent)]"
                        />
                        <button
                          type="button"
                          tabIndex={-1}
                          onClick={() => setPreview(i)}
                          aria-label={`Preview ${f.name}`}
                          className="shrink-0"
                        >
                          <FileThumbnail path={f.path} size={28} />
                        </button>
                        <span className="min-w-0 flex-1">
                          <span className="block truncate text-[13px]" title={f.name}>
                            {f.name}
                          </span>
                          <span
                            className="block truncate font-mono text-2xs text-subtle-foreground"
                            title={f.path}
                          >
                            {tildify(parentOf(f.path), home)}
                          </span>
                        </span>
                        <span className="w-28 shrink-0 text-2xs text-subtle-foreground">
                          {formatRelative(f.modifiedMs)}
                        </span>
                        <span className="tabular w-20 shrink-0 text-right text-xs">
                          {formatBytes(f.bytes)}
                        </span>
                        <button
                          type="button"
                          onClick={() => setPreview(i)}
                          title="Preview (Space)"
                          aria-label={`Preview ${f.name}`}
                          className="shrink-0 rounded p-1 text-muted-foreground hover:bg-hover hover:text-foreground"
                        >
                          <Eye className="size-3.5" aria-hidden />
                        </button>
                      </div>
                    </li>
                  );
                })}
              </ul>
            ) : null}
            <Pager page={page} label="files" className="px-4 py-3" />
            {group && !groupFiles ? (
              <div className="space-y-2 p-4" aria-label="Loading files">
                {Array.from({ length: 8 }, (_, i) => (
                  <Skeleton key={i} className="h-7 w-full" />
                ))}
              </div>
            ) : visible.length === 0 ? (
              <p className="px-4 py-6 text-xs text-muted-foreground">No files match the filter.</p>
            ) : null}
          </div>
          <div className="flex shrink-0 items-center gap-3 border-t border-border bg-panel px-4 py-2.5">
            <p className="text-xs text-muted-foreground">
              {chosen.length
                ? `${formatCount(chosen.length)} selected${
                    hiddenChosen
                      ? ` (${formatCount(hiddenChosen)} not shown with this type and filters)`
                      : ""
                  } · about ${formatBytes(chosenBytes)} freed once the ${words().trash} is emptied`
                : "Select files you no longer need. Press Space on a row to preview it."}
            </p>
            <Button
              className="ml-auto"
              disabled={chosen.length === 0}
              onClick={() => setDialog(true)}
            >
              Review {chosen.length || ""} file{chosen.length === 1 ? "" : "s"}…
            </Button>
          </div>
        </div>
      </div>
      <FilePreview
        files={visible.map((f) => ({
          path: f.path,
          bytes: f.bytes,
          modifiedMs: f.modifiedMs,
          selected: selected.has(f.path),
        }))}
        index={preview}
        onIndexChange={setPreview}
        onClose={() => setPreview(null)}
        onToggleSelect={toggle}
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
