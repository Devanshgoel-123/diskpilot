import { FileStack, Search } from "lucide-react";
import { useMemo, useRef, useState } from "react";
import { useShallow } from "zustand/react/shallow";
import { Pager } from "@/components/shared/Pager";
import { usePage, useScrollTopOnPage } from "@/lib/usePage";

import { PageHeader } from "@/components/layout/PageHeader";
import { StorageTabs } from "@/features/storage/StorageTabs";
import { CategoryBadge } from "@/components/shared/CategoryBadge";
import { CategorySelect } from "@/components/shared/CategorySelect";
import { DetailsPanel } from "@/components/shared/DetailsPanel";
import { EmptyState } from "@/components/shared/EmptyState";
import { SizeBar } from "@/components/shared/SizeBar";
import { SortHeader } from "@/components/shared/SortHeader";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { ALL_CATEGORIES, CATEGORY_META } from "@/lib/categories";
import { basename, formatBytes, formatCount, formatRelative, tildify } from "@/lib/format";
import { useApp } from "@/lib/store";
import type { Category, Classification } from "@/lib/types";
import { cn } from "@/lib/utils";

type View = "files" | "folders";
type SortKey = "size" | "name" | "modified";

interface Row {
  path: string;
  name: string;
  bytes: number;
  modifiedMs: number | null;
  fileCount: number | null;
  classification: Classification;
}

function parentOf(path: string): string {
  const i = path.lastIndexOf("/");
  return i > 0 ? path.slice(0, i) : "/";
}

export function LargeFilesPage() {
  const { summary, env, scanning, startScan } = useApp(
    useShallow((s) => ({
      summary: s.summary,
      env: s.env,
      scanning: s.scanning,
      startScan: s.startScan,
    })),
  );
  const openInExplorer = useApp((s) => s.openInExplorer);
  const home = env?.homeDir;
  const [view, setView] = useState<View>("files");
  const [query, setQuery] = useState("");
  const [category, setCategory] = useState<Category | "all">("all");
  const [sort, setSort] = useState<{ key: SortKey; desc: boolean }>({ key: "size", desc: true });
  const [selected, setSelected] = useState<string | null>(null);

  const rows: Row[] = useMemo(() => {
    if (!summary) return [];
    if (view === "files") {
      return summary.largestFiles.map((f) => ({
        path: f.path,
        name: f.name,
        bytes: f.bytes,
        modifiedMs: f.modifiedMs,
        fileCount: null,
        classification: f.classification,
      }));
    }
    return summary.heavyDirectories.map((d) => ({
      path: d.path,
      name: basename(d.path),
      bytes: d.bytes,
      modifiedMs: null,
      fileCount: d.fileCount,
      classification: d.classification,
    }));
  }, [summary, view]);

  const visible = useMemo(() => {
    const q = query.trim().toLowerCase();
    const filtered = rows.filter(
      (r) =>
        (!q || r.path.toLowerCase().includes(q)) &&
        (category === "all" || r.classification.category === category),
    );
    const dir = sort.desc ? -1 : 1;
    return filtered.sort((a, b) => {
      if (sort.key === "name") return dir * a.name.localeCompare(b.name);
      if (sort.key === "modified") return dir * ((a.modifiedMs ?? 0) - (b.modifiedMs ?? 0));
      return dir * (a.bytes - b.bytes);
    });
  }, [rows, query, category, sort]);
  const page = usePage(visible, 100, `${view}|${query}|${category}|${sort.key}|${sort.desc}`);
  const listRef = useRef<HTMLDivElement>(null);
  useScrollTopOnPage(listRef, page.page);

  const presentCategories = useMemo(
    () => ALL_CATEGORIES.filter((c) => rows.some((r) => r.classification.category === c)),
    [rows],
  );
  const max = rows[0]?.bytes ?? 0;
  const total = visible.reduce((a, r) => a + r.bytes, 0);

  const toggleSort = (key: SortKey) =>
    setSort((s) => (s.key === key ? { key, desc: !s.desc } : { key, desc: key !== "name" }));

  if (!summary) {
    return (
      <>
        <PageHeader title="Storage" actions={<StorageTabs />} />
        <EmptyState
          className="flex-1 justify-center"
          icon={FileStack}
          title={scanning ? "Scanning…" : "No scan yet"}
          description="The largest files and folders appear here after a scan."
          action={scanning ? null : <Button onClick={() => void startScan()}>Start scan</Button>}
        />
      </>
    );
  }

  return (
    <>
      <PageHeader
        title="Storage"
        actions={<StorageTabs />}
        description="Large does not mean disposable. DiskPilot never marks a file safe to delete because of its size."
      />
      <div className="flex min-h-0 flex-1">
        <div className="flex min-w-0 flex-1 flex-col">
          <div className="flex shrink-0 items-center gap-2 border-b border-border px-4 py-2">
            <div
              role="tablist"
              aria-label="Show"
              className="flex rounded-md border border-border-strong p-0.5"
            >
              {(["files", "folders"] as const).map((v) => (
                <button
                  key={v}
                  type="button"
                  role="tab"
                  aria-selected={view === v}
                  onClick={() => {
                    setView(v);
                    setSelected(null);
                  }}
                  className={cn(
                    "h-6 rounded px-2.5 text-xs",
                    view === v
                      ? "bg-hover font-medium text-foreground"
                      : "text-muted-foreground hover:text-foreground",
                  )}
                >
                  {v === "files" ? "Files" : "Folders"}
                </button>
              ))}
            </div>
            <p className="tabular text-xs text-muted-foreground">
              {formatCount(visible.length)} items · {formatBytes(total)}
            </p>
            <div className="ml-auto flex items-center gap-2">
              <div className="relative w-56">
                <Search
                  className="pointer-events-none absolute top-1/2 left-2 size-3.5 -translate-y-1/2 text-subtle-foreground"
                  aria-hidden
                />
                <Input
                  value={query}
                  onChange={(e) => setQuery(e.target.value)}
                  placeholder="Search name or path"
                  data-filter
                  className="pl-7"
                  aria-label="Search name or path"
                />
              </div>
              <CategorySelect value={category} onChange={setCategory} options={presentCategories} />
            </div>
          </div>
          {view === "folders" ? (
            <p className="shrink-0 border-b border-border bg-panel px-4 py-2 text-xs text-muted-foreground">
              Folders that hold most of their data in their own files rather than in subfolders:
              where storage actually accumulates.
            </p>
          ) : null}
          <div ref={listRef} className="min-h-0 flex-1 overflow-auto">
            <table className="w-full table-fixed border-collapse text-[13px]">
              <thead className="sticky top-0 z-10 bg-background">
                <tr className="border-b border-border text-left text-xs text-muted-foreground">
                  <SortHeader
                    label="Name"
                    k="name"
                    sort={sort}
                    onSort={toggleSort}
                    className="pl-4"
                  />
                  <th className="px-2 py-2 font-normal">Location</th>
                  <SortHeader
                    label="Size"
                    k="size"
                    sort={sort}
                    onSort={toggleSort}
                    className="w-40"
                    align="right"
                  />
                  {view === "files" ? (
                    <SortHeader
                      label="Modified"
                      k="modified"
                      sort={sort}
                      onSort={toggleSort}
                      className="w-28"
                    />
                  ) : (
                    <th className="w-24 px-2 py-2 text-right font-normal">Files</th>
                  )}
                  <th className="w-44 px-2 py-2 pr-4 font-normal">Category</th>
                </tr>
              </thead>
              <tbody>
                {page.items.map((r) => (
                  <tr
                    key={r.path}
                    tabIndex={0}
                    aria-selected={selected === r.path}
                    onClick={() => setSelected(r.path)}
                    // Open a folder in Storage > Folders; for a file, the folder that holds it.
                    onDoubleClick={() =>
                      openInExplorer(view === "files" ? parentOf(r.path) : r.path)
                    }
                    onKeyDown={(e) => {
                      if (e.key === "Enter")
                        openInExplorer(view === "files" ? parentOf(r.path) : r.path);
                    }}
                    title={
                      view === "files" ? "Double-click to show its folder" : "Double-click to open"
                    }
                    className={cn(
                      "h-8 border-b border-border/60 outline-none hover:bg-hover/60 focus-visible:bg-hover",
                      selected === r.path && "bg-accent-soft hover:bg-accent-soft",
                    )}
                  >
                    <td className="truncate pr-2 pl-4" title={r.name}>
                      {r.name}
                    </td>
                    <td
                      className="truncate px-2 font-mono text-2xs text-subtle-foreground"
                      title={r.path}
                    >
                      {tildify(view === "files" ? parentOf(r.path) : r.path, home)}
                    </td>
                    <td className="px-2">
                      <div className="flex items-center gap-2">
                        <SizeBar
                          value={r.bytes}
                          max={max}
                          color={CATEGORY_META[r.classification.category].color}
                          className="w-16"
                        />
                        <span className="tabular ml-auto">{formatBytes(r.bytes)}</span>
                      </div>
                    </td>
                    {view === "files" ? (
                      <td className="truncate px-2 text-xs text-muted-foreground">
                        {formatRelative(r.modifiedMs)}
                      </td>
                    ) : (
                      <td className="tabular px-2 text-right text-xs text-muted-foreground">
                        {formatCount(r.fileCount)}
                      </td>
                    )}
                    <td className="px-2 pr-4" title={r.classification.label}>
                      <CategoryBadge category={r.classification.category} />
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
            {visible.length === 0 ? (
              <p className="px-4 py-6 text-xs text-muted-foreground">No items match the filter.</p>
            ) : (
              <div className="flex flex-wrap items-center gap-3 px-4 py-3">
                <p className="min-w-0 flex-1 text-2xs text-subtle-foreground">
                  {view === "files"
                    ? `The ${formatCount(summary.largestFiles.length)} largest files found by the scan.`
                    : `The ${formatCount(summary.heavyDirectories.length)} largest such folders found by the scan.`}
                </p>
                <Pager page={page} label="items" />
              </div>
            )}
          </div>
        </div>
        {selected ? <DetailsPanel path={selected} onClose={() => setSelected(null)} /> : null}
      </div>
    </>
  );
}
