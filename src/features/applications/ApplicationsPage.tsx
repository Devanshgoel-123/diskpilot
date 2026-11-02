import { AppWindow, ChevronRight, FolderOpen, PackageX, Search } from "lucide-react";
import { useEffect, useMemo, useState } from "react";

import { PageHeader } from "@/components/layout/PageHeader";
import { AppIcon } from "@/components/shared/AppIcon";
import { EmptyState } from "@/components/shared/EmptyState";
import { TierBadge } from "@/components/shared/TierBadge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Section } from "@/components/ui/section";
import { Skeleton } from "@/components/ui/skeleton";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { api } from "@/lib/api";
import { formatBytes, formatRelative, tildify } from "@/lib/format";
import { useApp } from "@/lib/store";
import type { AppFootprint, AppStorageReport } from "@/lib/types";
import { cn } from "@/lib/utils";

type SortKey = "total" | "data" | "app" | "name";

const SORTS: { key: SortKey; label: string }[] = [
  { key: "total", label: "Total size" },
  { key: "data", label: "Data size" },
  { key: "app", label: "App size" },
  { key: "name", label: "Name" },
];

const total = (a: AppFootprint) => (a.appBytes ?? 0) + a.dataBytes;

function sortApps(apps: AppFootprint[], key: SortKey): AppFootprint[] {
  const sorted = [...apps];
  switch (key) {
    case "name":
      return sorted.sort((a, b) => a.name.localeCompare(b.name));
    case "data":
      return sorted.sort((a, b) => b.dataBytes - a.dataBytes);
    case "app":
      return sorted.sort((a, b) => (b.appBytes ?? 0) - (a.appBytes ?? 0));
    default:
      return sorted.sort((a, b) => total(b) - total(a));
  }
}

function Reveal({ path }: { path: string }) {
  const [failed, setFailed] = useState(false);
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button
          variant="ghost"
          size="icon"
          aria-label="Show in Finder"
          onClick={(e) => {
            e.stopPropagation();
            api.revealInFinder(path).catch(() => setFailed(true));
          }}
        >
          <FolderOpen />
        </Button>
      </TooltipTrigger>
      <TooltipContent>
        {failed ? "Only folders inside the last scan can be shown" : "Show in Finder"}
      </TooltipContent>
    </Tooltip>
  );
}

function AppRow({
  app,
  max,
  open,
  onToggle,
}: {
  app: AppFootprint;
  max: number;
  open: boolean;
  onToggle: () => void;
}) {
  const home = useApp((s) => s.env?.homeDir);
  const appShare = max ? ((app.appBytes ?? 0) / max) * 100 : 0;
  const dataShare = max ? (app.dataBytes / max) * 100 : 0;
  return (
    <li className="border-b border-border last:border-b-0">
      <button
        type="button"
        onClick={onToggle}
        aria-expanded={open}
        disabled={!app.locations.length}
        className="grid w-full grid-cols-[minmax(0,1fr)_5.5rem_5.5rem_9rem_6.5rem_1rem] items-center gap-4 px-2 py-2 text-left transition-colors duration-[120ms] hover:bg-hover disabled:hover:bg-transparent"
      >
        <span className="flex min-w-0 items-center gap-3">
          <AppIcon
            path={app.appPath}
            className="size-6"
            fallback={<AppWindow className="size-6 p-0.5 text-subtle-foreground" aria-hidden />}
          />
          <span className="min-w-0">
            <span className="block truncate text-[13px] font-medium">{app.name}</span>
            <span className="block truncate font-mono text-2xs text-subtle-foreground">
              {app.bundleId}
            </span>
          </span>
        </span>
        <span className="tabular text-right text-[13px] text-muted-foreground">
          {app.appBytes === null ? "System" : formatBytes(app.appBytes)}
        </span>
        <span className="tabular text-right text-[13px]">
          {app.dataBytes ? formatBytes(app.dataBytes) : "—"}
        </span>
        <span className="flex h-1.5 overflow-hidden rounded-full bg-track" aria-hidden>
          <span className="h-full bg-chart-other" style={{ width: `${appShare}%` }} />
          <span className="h-full bg-accent" style={{ width: `${dataShare}%` }} />
        </span>
        <span className="tabular text-right text-xs text-subtle-foreground">
          {app.dataModifiedMs ? formatRelative(app.dataModifiedMs) : ""}
        </span>
        <ChevronRight
          className={cn(
            "size-3.5 text-subtle-foreground transition-transform duration-[180ms]",
            open && "rotate-90",
            !app.locations.length && "invisible",
          )}
          aria-hidden
        />
      </button>
      {open ? (
        <ul className="animate-in mb-2 ml-11 border-l border-border pl-3">
          {app.locations.map((l) => (
            <li key={l.path} className="flex items-center gap-3 py-1">
              <span className="w-36 shrink-0 text-xs text-muted-foreground">{l.kind}</span>
              <span
                className="min-w-0 flex-1 truncate font-mono text-2xs text-subtle-foreground"
                title={l.path}
              >
                {tildify(l.path, home)}
              </span>
              <span className="tabular w-20 text-right text-xs">{formatBytes(l.bytes)}</span>
              <Reveal path={l.path} />
            </li>
          ))}
        </ul>
      ) : null}
    </li>
  );
}

export function ApplicationsPage() {
  const { summary, startScan, scanning } = useApp();
  const [report, setReport] = useState<AppStorageReport | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [sort, setSort] = useState<SortKey>("total");
  const [open, setOpen] = useState<string | null>(null);

  // Reload when a new scan finishes, since data sizes come from the scan.
  const scanId = summary?.id;
  const [attempt, setAttempt] = useState(0);
  useEffect(() => {
    let live = true;
    api
      .getAppStorage()
      .then((r) => {
        if (!live) return;
        setReport(r);
        setError(null);
      })
      .catch((e: unknown) => {
        if (live) setError(String(e));
      });
    return () => {
      live = false;
    };
  }, [scanId, attempt]);
  const load = () => setAttempt((n) => n + 1);

  const apps = useMemo(() => {
    const q = query.trim().toLowerCase();
    const list = report?.apps ?? [];
    const filtered = q
      ? list.filter((a) => a.name.toLowerCase().includes(q) || a.bundleId.toLowerCase().includes(q))
      : list;
    return sortApps(filtered, sort);
  }, [report, query, sort]);
  const max = useMemo(() => Math.max(0, ...(report?.apps ?? []).map(total)), [report]);
  const appTotal = (report?.apps ?? []).reduce((a, x) => a + (x.appBytes ?? 0), 0);
  const dataTotal = (report?.apps ?? []).reduce((a, x) => a + x.dataBytes, 0);
  const leftoverTotal = (report?.leftovers ?? []).reduce((a, x) => a + x.bytes, 0);

  return (
    <>
      <PageHeader
        title="Applications"
        description="Installed apps and the data each keeps in your Library."
      />
      <div className="min-h-0 flex-1 overflow-y-auto">
        <div className="mx-auto max-w-[1120px] space-y-8 px-6 pt-6 pb-10">
          {error ? (
            <EmptyState
              icon={AppWindow}
              title="Applications could not be read"
              description={error}
              action={
                <Button variant="secondary" onClick={load}>
                  Try again
                </Button>
              }
            />
          ) : !report ? (
            <div className="space-y-3" aria-label="Loading applications">
              <Skeleton className="h-6 w-80" />
              {Array.from({ length: 8 }, (_, i) => (
                <div key={i} className="flex items-center gap-3">
                  <Skeleton className="size-6" />
                  <Skeleton className="h-3 flex-1" />
                  <Skeleton className="h-3 w-16" />
                </div>
              ))}
            </div>
          ) : (
            <>
              <div className="flex flex-wrap items-end justify-between gap-6">
                <dl className="flex flex-wrap gap-x-10 gap-y-3">
                  <Figure label="Apps" value={String(report.apps.length)} />
                  <Figure label="In app bundles" value={formatBytes(appTotal)} />
                  <Figure
                    label="App data in Library"
                    value={report.dataScanned ? formatBytes(dataTotal) : "Not scanned"}
                  />
                  {report.leftovers.length ? (
                    <Figure label="Left by removed apps" value={formatBytes(leftoverTotal)} />
                  ) : null}
                </dl>
                <div className="flex items-center gap-2">
                  <div className="relative">
                    <Search
                      className="pointer-events-none absolute top-1/2 left-2 size-3.5 -translate-y-1/2 text-subtle-foreground"
                      aria-hidden
                    />
                    <Input
                      id="app-filter"
                      value={query}
                      onChange={(e) => setQuery(e.target.value)}
                      placeholder="Filter apps"
                      aria-label="Filter apps"
                      className="w-52 pl-7"
                    />
                  </div>
                  <select
                    id="app-sort"
                    aria-label="Sort apps"
                    value={sort}
                    onChange={(e) => setSort(e.target.value as SortKey)}
                    className="h-7 rounded-md border border-border-strong bg-panel px-2 text-[13px]"
                  >
                    {SORTS.map((s) => (
                      <option key={s.key} value={s.key}>
                        {s.label}
                      </option>
                    ))}
                  </select>
                </div>
              </div>

              {!report.dataScanned ? (
                <div className="flex items-center justify-between gap-4 rounded-lg bg-panel-raised px-4 py-3">
                  <p className="text-[13px] text-muted-foreground">
                    App sizes are shown. Scan your home folder to add each app&apos;s caches,
                    containers and support files.
                  </p>
                  <Button size="sm" disabled={scanning} onClick={() => void startScan()}>
                    {scanning ? "Scanning…" : "Scan"}
                  </Button>
                </div>
              ) : null}

              <section aria-label="Applications">
                <div className="grid grid-cols-[minmax(0,1fr)_5.5rem_5.5rem_9rem_6.5rem_1rem] gap-4 border-b border-border px-2 pb-1.5 text-xs text-subtle-foreground">
                  <span>Name</span>
                  <span className="text-right">App</span>
                  <span className="text-right">Data</span>
                  <span className="flex items-center gap-3">
                    <span className="flex items-center gap-1">
                      <span className="size-1.5 rounded-full bg-chart-other" aria-hidden />
                      App
                    </span>
                    <span className="flex items-center gap-1">
                      <span className="size-1.5 rounded-full bg-accent" aria-hidden />
                      Data
                    </span>
                  </span>
                  <span className="text-right">Data changed</span>
                  <span />
                </div>
                {apps.length ? (
                  <ul>
                    {apps.map((a) => (
                      <AppRow
                        key={a.appPath}
                        app={a}
                        max={max}
                        open={open === a.appPath}
                        onToggle={() => setOpen((o) => (o === a.appPath ? null : a.appPath))}
                      />
                    ))}
                  </ul>
                ) : (
                  <p className="px-2 py-6 text-[13px] text-muted-foreground">
                    No apps match &ldquo;{query}&rdquo;.
                  </p>
                )}
              </section>

              {report.leftovers.length ? (
                <Section
                  title="Left behind by removed apps"
                  description="Sandbox data from apps that are no longer installed. It can hold documents the app saved, so DiskPilot never removes it. Review it in Finder."
                >
                  <ul className="border-t border-border">
                    {report.leftovers.map((l) => (
                      <li
                        key={l.path}
                        className="flex items-center gap-3 border-b border-border px-2 py-2"
                      >
                        <PackageX
                          className="size-5 shrink-0 text-subtle-foreground"
                          strokeWidth={1.5}
                          aria-hidden
                        />
                        <div className="min-w-0 flex-1">
                          <p className="truncate font-mono text-xs">{l.name}</p>
                          <p className="text-2xs text-subtle-foreground">
                            {l.kind}
                            {l.modifiedMs ? `, changed ${formatRelative(l.modifiedMs)}` : ""}
                          </p>
                        </div>
                        <TierBadge tier="reviewFirst" />
                        <span className="tabular w-20 text-right text-[13px]">
                          {formatBytes(l.bytes)}
                        </span>
                        <Reveal path={l.path} />
                      </li>
                    ))}
                  </ul>
                </Section>
              ) : null}
            </>
          )}
        </div>
      </div>
    </>
  );
}

function Figure({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <dt className="text-xs text-muted-foreground">{label}</dt>
      <dd className="tabular mt-0.5 text-[20px] leading-7 font-semibold tracking-tight">{value}</dd>
    </div>
  );
}
