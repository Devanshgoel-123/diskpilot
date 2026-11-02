import {
  Activity,
  AlertTriangle,
  Box,
  Container,
  Database,
  Info,
  Layers,
  Lightbulb,
  Loader2,
  PlugZap,
  RefreshCw,
  Stethoscope,
  Trash2,
} from "lucide-react";
import { type CSSProperties, useCallback, useEffect, useState } from "react";

import { PageHeader } from "@/components/layout/PageHeader";
import { EmptyState } from "@/components/shared/EmptyState";
import { Button } from "@/components/ui/button";
import { Panel, PanelHeader } from "@/components/ui/panel";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { api, errorMessage } from "@/lib/api";
import { formatBytes, formatCount, formatPercent } from "@/lib/format";
import { useApp } from "@/lib/store";
import type {
  DiagnosticFinding,
  DockerAction,
  DockerOverview,
  FindingLevel,
  UsageRow,
} from "@/lib/types";
import { cn } from "@/lib/utils";

import { DockerActionDialog } from "./DockerActionDialog";
import { BuildCacheTable, ContainersTable, ImagesTable, VolumesTable } from "./DockerTables";
import { InspectPanel, type InspectTarget } from "./InspectPanel";

type Tab = "containers" | "images" | "volumes" | "buildCache";

const KINDS: {
  key: Tab;
  usage: keyof DockerOverview["usage"];
  label: string;
  color: string;
  icon: typeof Box;
}[] = [
  {
    key: "images",
    usage: "images",
    label: "Images",
    color: "var(--cat-application-data)",
    icon: Layers,
  },
  {
    key: "containers",
    usage: "containers",
    label: "Containers",
    color: "var(--cat-docker-storage)",
    icon: Container,
  },
  {
    key: "volumes",
    usage: "volumes",
    label: "Volumes",
    color: "var(--cat-user-data)",
    icon: Database,
  },
  {
    key: "buildCache",
    usage: "buildCache",
    label: "Build cache",
    color: "var(--cat-build-artifacts)",
    icon: Box,
  },
];

const FINDING_STYLE: Record<FindingLevel, { icon: typeof Info; color: string }> = {
  info: { icon: Info, color: "var(--accent)" },
  suggestion: { icon: Lightbulb, color: "var(--tier-safe)" },
  warning: { icon: AlertTriangle, color: "var(--tier-review)" },
};

function UsageTile({
  label,
  row,
  color,
  icon: Icon,
  active,
  onClick,
}: {
  label: string;
  row: UsageRow;
  color: string;
  icon: typeof Box;
  active: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      aria-pressed={active}
      style={{ "--tint": color } as CSSProperties}
      className={cn(
        "rounded-lg border bg-panel px-4 py-3 text-left transition-colors hover:bg-panel-raised",
        active ? "tinted" : "border-border",
      )}
    >
      <p className="flex items-center justify-between text-xs text-muted-foreground">
        {label}
        <Icon className="size-3.5" style={{ color }} aria-hidden />
      </p>
      <p className="tabular mt-1 text-lg font-semibold tracking-tight">
        {formatBytes(row.sizeBytes)}
      </p>
      <p className="tabular mt-0.5 text-2xs text-subtle-foreground">
        {formatCount(row.totalCount)} total · {formatCount(row.active)} active ·{" "}
        {formatBytes(row.reclaimableBytes)} reclaimable
      </p>
    </button>
  );
}

export function DockerPage() {
  const refreshHistory = useApp((s) => s.refreshHistory);
  const [overview, setOverview] = useState<DockerOverview | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [tab, setTab] = useState<Tab>("containers");
  const [inspect, setInspect] = useState<InspectTarget | null>(null);
  const [action, setAction] = useState<DockerAction | null>(null);
  const [findings, setFindings] = useState<DiagnosticFinding[] | null>(null);
  const [diagnosing, setDiagnosing] = useState(false);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      setOverview(await api.dockerOverview());
      setError(null);
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    let live = true;
    api
      .dockerOverview()
      .then((o) => {
        if (!live) return;
        setOverview(o);
        setError(null);
      })
      .catch((e: unknown) => live && setError(errorMessage(e)))
      .finally(() => live && setLoading(false));
    return () => {
      live = false;
    };
  }, []);

  const diagnose = async () => {
    setDiagnosing(true);
    try {
      setFindings(await api.dockerDiagnostics());
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setDiagnosing(false);
    }
  };

  const status = overview?.status;
  const running = status?.state === "running";
  const usage = overview?.usage;
  const total = usage ? KINDS.reduce((a, k) => a + usage[k.usage].sizeBytes, 0) : 0;

  const headerActions = (
    <>
      {running ? (
        <Button variant="secondary" onClick={() => void diagnose()} disabled={diagnosing}>
          {diagnosing ? <Loader2 className="animate-spin" /> : <Stethoscope />}
          Diagnostics
        </Button>
      ) : null}
      <Button
        variant="ghost"
        size="icon"
        aria-label="Refresh"
        onClick={() => void load()}
        disabled={loading}
      >
        <RefreshCw className={cn(loading && "animate-spin")} />
      </Button>
    </>
  );

  return (
    <>
      <PageHeader
        title="Docker"
        description={
          running
            ? `${status?.platform ?? "Docker Engine"} · engine ${status?.serverVersion ?? "?"}`
            : "Docker disk usage and safe resource management."
        }
        actions={headerActions}
      />
      <div className="flex min-h-0 flex-1">
        <div className="min-h-0 min-w-0 flex-1 overflow-y-auto">
          <div className="mx-auto max-w-[1180px] space-y-4 px-6 py-5">
            {error ? <p className="text-xs text-critical">{error}</p> : null}

            {loading && !overview ? (
              <p className="flex items-center gap-2 text-xs text-muted-foreground">
                <Loader2 className="size-3.5 animate-spin" aria-hidden /> Asking Docker for its disk
                usage…
              </p>
            ) : null}

            {overview && !running ? (
              <Panel>
                <EmptyState
                  icon={PlugZap}
                  title={
                    status?.state === "notInstalled"
                      ? "Docker is not installed"
                      : "Docker is not running"
                  }
                  description={
                    status?.state === "notInstalled"
                      ? "DiskPilot looks for the docker CLI in the usual Docker Desktop, Homebrew, OrbStack and Colima locations."
                      : (status?.message ?? "Start Docker Desktop, then refresh.")
                  }
                  action={
                    status?.state === "notRunning" ? (
                      <Button variant="secondary" onClick={() => void load()}>
                        <RefreshCw />
                        Check again
                      </Button>
                    ) : null
                  }
                />
              </Panel>
            ) : null}

            {overview && running && usage ? (
              <>
                <div className="grid grid-cols-2 gap-3 lg:grid-cols-4">
                  {KINDS.map((k) => (
                    <UsageTile
                      key={k.key}
                      label={k.label}
                      row={usage[k.usage]}
                      color={k.color}
                      icon={k.icon}
                      active={tab === k.key}
                      onClick={() => setTab(k.key)}
                    />
                  ))}
                </div>

                <Panel className="px-4 py-3">
                  <p className="flex items-baseline justify-between text-xs text-muted-foreground">
                    <span className="flex items-center gap-1.5">
                      <Activity className="size-3.5" aria-hidden />
                      Usage inside Docker
                    </span>
                    <span className="tabular text-foreground">{formatBytes(total)}</span>
                  </p>
                  <div className="mt-2 flex h-2.5 gap-[2px] overflow-hidden rounded-full bg-track">
                    {KINDS.map((k) =>
                      usage[k.usage].sizeBytes > 0 ? (
                        <Tooltip key={k.key}>
                          <TooltipTrigger asChild>
                            <div
                              className="h-full first:rounded-l-full last:rounded-r-full"
                              style={{
                                width: `${(usage[k.usage].sizeBytes / Math.max(total, 1)) * 100}%`,
                                background: k.color,
                                minWidth: 3,
                              }}
                            />
                          </TooltipTrigger>
                          <TooltipContent>
                            {k.label} · {formatBytes(usage[k.usage].sizeBytes)} (
                            {formatPercent(usage[k.usage].sizeBytes, total)})
                          </TooltipContent>
                        </Tooltip>
                      ) : null,
                    )}
                  </div>
                  <ul className="mt-2 flex flex-wrap gap-x-4 gap-y-1">
                    {KINDS.map((k) => (
                      <li
                        key={k.key}
                        className="flex items-center gap-1.5 text-xs text-muted-foreground"
                      >
                        <span
                          className="size-2 rounded-[3px]"
                          style={{ background: k.color }}
                          aria-hidden
                        />
                        {k.label}
                        <span className="tabular text-foreground">
                          {formatBytes(usage[k.usage].sizeBytes)}
                        </span>
                      </li>
                    ))}
                  </ul>
                </Panel>

                {findings ? (
                  <Panel>
                    <PanelHeader
                      title="Diagnostics"
                      description="Read-only analysis of Docker's disk usage."
                      actions={
                        <Button variant="ghost" size="sm" onClick={() => setFindings(null)}>
                          Hide
                        </Button>
                      }
                    />
                    <ul className="divide-y divide-border border-t border-border">
                      {findings.map((f) => {
                        const s = FINDING_STYLE[f.level];
                        return (
                          <li key={f.title} className="flex gap-3 px-4 py-2.5">
                            <s.icon
                              className="mt-0.5 size-3.5 shrink-0"
                              style={{ color: s.color }}
                              aria-hidden
                            />
                            <div className="min-w-0 flex-1">
                              <p className="text-[13px]">{f.title}</p>
                              <p className="mt-0.5 text-xs leading-relaxed text-muted-foreground">
                                {f.detail}
                              </p>
                            </div>
                            {f.bytes != null ? (
                              <span className="tabular shrink-0 text-xs">
                                {formatBytes(f.bytes)}
                              </span>
                            ) : null}
                          </li>
                        );
                      })}
                    </ul>
                  </Panel>
                ) : null}

                <Panel>
                  <div className="flex items-center justify-between gap-3 border-b border-border px-2 pt-1">
                    <div role="tablist" className="flex">
                      {KINDS.map((k) => {
                        const count =
                          k.key === "buildCache"
                            ? overview.buildCache.length
                            : overview[k.key].length;
                        return (
                          <button
                            key={k.key}
                            role="tab"
                            type="button"
                            aria-selected={tab === k.key}
                            onClick={() => setTab(k.key)}
                            className={cn(
                              "-mb-px flex items-center gap-1.5 border-b-2 px-3 py-2 text-xs",
                              tab === k.key
                                ? "border-accent font-medium text-foreground"
                                : "border-transparent text-muted-foreground hover:text-foreground",
                            )}
                          >
                            {k.label}
                            <span className="tabular text-2xs text-subtle-foreground">{count}</span>
                          </button>
                        );
                      })}
                    </div>
                    {tab === "buildCache" ? (
                      <Button
                        variant="secondary"
                        size="sm"
                        className="mr-2"
                        disabled={!overview.buildCache.some((b) => !b.inUse)}
                        onClick={() => setAction({ type: "pruneBuildCache" })}
                      >
                        <Trash2 />
                        Remove unused build cache…
                      </Button>
                    ) : null}
                  </div>
                  {tab === "volumes" ? (
                    <p className="flex items-center gap-1.5 border-b border-border px-4 py-2 text-xs text-muted-foreground">
                      <Info className="size-3.5 shrink-0" aria-hidden />
                      Volumes often hold databases. DiskPilot shows and inspects them but never
                      removes them.
                    </p>
                  ) : null}
                  {tab === "containers" ? (
                    <ContainersTable
                      containers={overview.containers}
                      onAction={setAction}
                      onInspect={setInspect}
                    />
                  ) : tab === "images" ? (
                    <ImagesTable
                      images={overview.images}
                      onAction={setAction}
                      onInspect={setInspect}
                    />
                  ) : tab === "volumes" ? (
                    <VolumesTable volumes={overview.volumes} onInspect={setInspect} />
                  ) : (
                    <BuildCacheTable entries={overview.buildCache} />
                  )}
                </Panel>
              </>
            ) : null}
          </div>
        </div>
        {inspect ? <InspectPanel target={inspect} onClose={() => setInspect(null)} /> : null}
      </div>

      {action ? (
        <DockerActionDialog
          action={action}
          onClose={() => setAction(null)}
          onDone={() => {
            void load();
            void refreshHistory();
            if (findings) void diagnose();
          }}
        />
      ) : null}
    </>
  );
}
