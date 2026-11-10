import { ArrowUp, ChevronRight, File, Folder, FolderTree, Link2, Search } from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import { useShallow } from "zustand/react/shallow";

import { PageHeader } from "@/components/layout/PageHeader";
import { StorageTabs } from "@/features/storage/StorageTabs";
import { CategoryBadge } from "@/components/shared/CategoryBadge";
import { DetailsPanel } from "@/components/shared/DetailsPanel";
import { EmptyState } from "@/components/shared/EmptyState";
import { SizeBar } from "@/components/shared/SizeBar";
import { TierBadge } from "@/components/shared/TierBadge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Skeleton } from "@/components/ui/skeleton";
import { api, errorMessage } from "@/lib/api";
import { ALL_CATEGORIES, CATEGORY_META } from "@/lib/categories";
import { formatBytes, formatCount, formatRelative, tildify } from "@/lib/format";
import { joinPath, relativeParts } from "@/lib/paths";
import { words } from "@/lib/platform";
import { useApp } from "@/lib/store";
import type { Category, DirectoryEntry, DirectoryListing, Recommendation } from "@/lib/types";
import { cn } from "@/lib/utils";

import { AppIcon } from "@/components/shared/AppIcon";
import { CategorySelect } from "@/components/shared/CategorySelect";
import { SortHeader } from "@/components/shared/SortHeader";
import { type SortKey, sortEntries } from "./sorting";

function EntryIcon({ entry }: { entry: DirectoryEntry }) {
  const cls = "size-3.5 shrink-0";
  if (entry.kind === "directory")
    return (
      <AppIcon
        path={entry.path}
        fallback={<Folder className={cn(cls, "text-accent")} aria-hidden />}
      />
    );
  if (entry.kind === "symlink")
    return <Link2 className={cn(cls, "text-subtle-foreground")} aria-hidden />;
  return <File className={cn(cls, "text-subtle-foreground")} aria-hidden />;
}

/** Actionable recommendations by path, for the Cleanup column. */
function useCleanupIndex() {
  const recommendations = useApp((s) => s.recommendations);
  return useMemo(() => recommendations.filter((r) => r.methods.length > 0), [recommendations]);
}

/**
 * The item's own tier if DiskPilot recommends it, otherwise how much inside
 * it is recommended. Empty when there is nothing to clean up.
 */
function CleanupCell({ path, index }: { path: string; index: Recommendation[] }) {
  const own = index.find((r) => r.path === path);
  if (own) return <TierBadge tier={own.tier} />;
  const inside = index.filter((r) => r.path.startsWith(`${path}/`));
  if (!inside.length) return null;
  const bytes = inside.reduce((a, r) => a + r.estimatedReclaimBytes, 0);
  return (
    <span
      className="text-xs text-muted-foreground"
      title={`${inside.length} recommendation${inside.length === 1 ? "" : "s"} inside this folder`}
    >
      {formatBytes(bytes)} inside
    </span>
  );
}

function Breadcrumbs({
  listing,
  home,
  onNavigate,
}: {
  listing: DirectoryListing;
  home: string | undefined;
  onNavigate: (path: string) => void;
}) {
  const root = listing.scanRoot;
  // Names below the scan root; works with `/Users/me` as well as `C:\` or `D:\Data`.
  const rel = relativeParts(listing.path, root) ?? [];
  const crumbs = [{ label: tildify(root, home), path: root }];
  let acc = root;
  for (const part of rel) {
    acc = joinPath(acc, part);
    crumbs.push({ label: part, path: acc });
  }
  return (
    <nav aria-label="Folder path" className="flex min-w-0 items-center gap-0.5 overflow-hidden">
      {crumbs.map((c, i) => {
        const last = i === crumbs.length - 1;
        return (
          <span key={c.path} className="flex min-w-0 items-center gap-0.5">
            {i > 0 ? (
              <ChevronRight className="size-3 shrink-0 text-subtle-foreground" aria-hidden />
            ) : null}
            <button
              type="button"
              onClick={() => onNavigate(c.path)}
              disabled={last}
              className={cn(
                "truncate rounded px-1 py-0.5 font-mono text-xs",
                last
                  ? "text-foreground"
                  : "text-muted-foreground hover:bg-hover hover:text-foreground",
              )}
            >
              {c.label}
            </button>
          </span>
        );
      })}
    </nav>
  );
}

export function ExplorerPage() {
  const { summary, env, explorerPath, scanning, startScan, browse } = useApp(
    useShallow((s) => ({
      summary: s.summary,
      env: s.env,
      explorerPath: s.explorerPath,
      scanning: s.scanning,
      startScan: s.startScan,
      browse: s.browse,
    })),
  );
  const home = env?.homeDir;
  const requested = explorerPath ?? summary?.root ?? null;
  const [listing, setListing] = useState<DirectoryListing | null>(null);
  const [error, setError] = useState<{ path: string; message: string } | null>(null);
  const [selected, setSelected] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [category, setCategory] = useState<Category | "all">("all");
  const [sort, setSort] = useState<{ key: SortKey; desc: boolean }>({ key: "size", desc: true });

  // Fetch the listing whenever the requested folder (or the scan) changes.
  // State is only set from async callbacks.
  useEffect(() => {
    if (!summary || !requested) return;
    let live = true;
    api
      .listDirectory(requested)
      .then((l) => {
        if (!live) return;
        setListing(l);
        setError(null);
        setSelected(null);
      })
      .catch((e: unknown) => live && setError({ path: requested, message: errorMessage(e) }));
    return () => {
      live = false;
    };
  }, [summary, requested]);

  const loading = !!requested && listing?.path !== requested && error?.path !== requested;
  const load = useCallback((path: string) => browse(path), [browse]);

  const entries = useMemo(() => {
    if (!listing) return [];
    const q = query.trim().toLowerCase();
    const filtered = listing.entries.filter(
      (e) =>
        (!q || e.name.toLowerCase().includes(q)) &&
        (category === "all" || e.classification.category === category),
    );
    return sortEntries(filtered, sort.key, sort.desc);
  }, [listing, query, category, sort]);

  const cleanup = useCleanupIndex();
  const maxBytes = listing?.entries[0]?.bytes ?? 0;
  const presentCategories = useMemo(
    () =>
      ALL_CATEGORIES.filter((c) => listing?.entries.some((e) => e.classification.category === c)),
    [listing],
  );

  const toggleSort = (key: SortKey) =>
    setSort((s) => (s.key === key ? { key, desc: !s.desc } : { key, desc: key !== "name" }));

  const open = (entry: DirectoryEntry) => {
    if (entry.kind === "directory") load(entry.path);
  };

  if (!summary) {
    return (
      <>
        <PageHeader title="Storage" actions={<StorageTabs />} />
        <EmptyState
          className="flex-1 justify-center"
          icon={FolderTree}
          title={scanning ? "Scanning…" : "No scan yet"}
          description="Browse folders by size once a scan has finished. Folder sizes come from the scan; nothing is modified."
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
        description={`Folder sizes from the last scan. Open a folder with a double-click or ${words().enterKey}.`}
      />
      <div className="flex min-h-0 flex-1">
        <div className="flex min-w-0 flex-1 flex-col">
          <div className="flex shrink-0 items-center gap-2 border-b border-border px-4 py-2">
            <Button
              variant="ghost"
              size="icon"
              aria-label="Up one folder"
              disabled={!listing?.parent}
              onClick={() => listing?.parent && load(listing.parent)}
            >
              <ArrowUp />
            </Button>
            {listing ? <Breadcrumbs listing={listing} home={home} onNavigate={load} /> : null}
            <div className="ml-auto flex shrink-0 items-center gap-2">
              <div className="relative w-48">
                <Search
                  className="pointer-events-none absolute top-1/2 left-2 size-3.5 -translate-y-1/2 text-subtle-foreground"
                  aria-hidden
                />
                <Input
                  value={query}
                  onChange={(e) => setQuery(e.target.value)}
                  placeholder="Filter by name"
                  data-filter
                  className="pl-7"
                  aria-label="Filter by name"
                />
              </div>
              <CategorySelect value={category} onChange={setCategory} options={presentCategories} />
            </div>
          </div>

          {listing ? (
            <div className="flex shrink-0 items-center gap-4 border-b border-border bg-panel px-4 py-2 text-xs text-muted-foreground">
              <span>
                <span className="tabular font-medium text-foreground">
                  {formatBytes(listing.bytes)}
                </span>{" "}
                in this folder
              </span>
              <span className="tabular">{formatCount(listing.entries.length)} items</span>
              <span className="flex min-w-0 items-center gap-1.5">
                <CategoryBadge category={listing.classification.category} />
                <span className="truncate">{listing.classification.label}</span>
              </span>
            </div>
          ) : null}

          <div className="min-h-0 flex-1 overflow-auto">
            {error ? <p className="px-4 py-6 text-xs text-critical">{error.message}</p> : null}
            {loading && !listing ? (
              <div className="space-y-2 p-4">
                {Array.from({ length: 8 }, (_, i) => (
                  <Skeleton key={i} className="h-6 w-full" />
                ))}
              </div>
            ) : null}
            {listing ? (
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
                    <SortHeader
                      label="Size"
                      k="size"
                      sort={sort}
                      onSort={toggleSort}
                      className="w-40"
                      align="right"
                    />
                    <SortHeader
                      label="Files"
                      k="files"
                      sort={sort}
                      onSort={toggleSort}
                      className="w-20"
                      align="right"
                    />
                    <SortHeader
                      label="Modified"
                      k="modified"
                      sort={sort}
                      onSort={toggleSort}
                      className="w-28"
                    />
                    <th className="w-40 px-2 py-2 font-normal">Category</th>
                    <th className="w-36 px-2 py-2 pr-4 font-normal">Cleanup</th>
                  </tr>
                </thead>
                <tbody>
                  {entries.map((e) => (
                    <tr
                      key={e.path}
                      tabIndex={0}
                      aria-selected={selected === e.path}
                      onClick={() => setSelected(e.path)}
                      onDoubleClick={() => open(e)}
                      onKeyDown={(ev) => {
                        if (ev.key === "Enter") open(e);
                        if (ev.key === " ") {
                          ev.preventDefault();
                          setSelected(e.path);
                        }
                      }}
                      className={cn(
                        "h-8 border-b border-border/60 outline-none hover:bg-hover/60 focus-visible:bg-hover",
                        selected === e.path && "bg-accent-soft hover:bg-accent-soft",
                      )}
                    >
                      <td className="truncate pr-2 pl-4">
                        <span className="flex min-w-0 items-center gap-2">
                          <EntryIcon entry={e} />
                          <span className="truncate" title={e.name}>
                            {e.name}
                          </span>
                        </span>
                      </td>
                      <td className="px-2">
                        <div className="flex items-center gap-2">
                          <SizeBar
                            value={e.bytes}
                            max={maxBytes}
                            color={CATEGORY_META[e.classification.category].color}
                            className="w-16"
                          />
                          <span className="tabular ml-auto">{formatBytes(e.bytes)}</span>
                        </div>
                      </td>
                      <td className="tabular px-2 text-right text-xs text-muted-foreground">
                        {e.kind === "directory" ? formatCount(e.fileCount) : ""}
                      </td>
                      <td className="truncate px-2 text-xs text-muted-foreground">
                        {formatRelative(e.modifiedMs)}
                      </td>
                      <td className="px-2" title={e.classification.label}>
                        <CategoryBadge category={e.classification.category} />
                      </td>
                      <td className="px-2 pr-4">
                        <CleanupCell path={e.path} index={cleanup} />
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            ) : null}
            {listing && entries.length === 0 ? (
              <p className="px-4 py-6 text-xs text-muted-foreground">
                {listing.entries.length ? "No items match the filter." : "This folder is empty."}
              </p>
            ) : null}
            {listing && listing.omittedCount > 0 ? (
              <p className="px-4 py-3 text-xs text-subtle-foreground">
                {formatCount(listing.omittedCount)} smaller items (
                {formatBytes(listing.omittedBytes)}) are not listed.
              </p>
            ) : null}
          </div>
        </div>
        {selected ? <DetailsPanel path={selected} onClose={() => setSelected(null)} /> : null}
      </div>
    </>
  );
}
