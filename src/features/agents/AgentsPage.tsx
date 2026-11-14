import {
  AlertTriangle,
  Blocks,
  Cloud,
  Loader2,
  MousePointer2,
  Orbit,
  RefreshCw,
  SquareTerminal,
  X,
  type LucideIcon,
} from "lucide-react";
import { useEffect, useMemo, useState } from "react";

import { PageHeader } from "@/components/layout/PageHeader";
import { AppIcon } from "@/components/shared/AppIcon";
import { EmptyState } from "@/components/shared/EmptyState";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Panel } from "@/components/ui/panel";
import { formatBytes, formatRelative } from "@/lib/format";
import { useApp } from "@/lib/store";
import type { AgentReport } from "@/lib/types";
import { cn } from "@/lib/utils";

import { type AgentTab, useAgents, useReviewable } from "./agentsStore";
import { type Confirmation } from "./shared";
import {
  CleanupList,
  ConnectionsList,
  PermissionsList,
  PluginsList,
  SkillsList,
} from "./AgentSections";

const AGENT_ICON: Record<string, LucideIcon> = {
  "claude-code": SquareTerminal,
  codex: Cloud,
  cursor: MousePointer2,
  antigravity: Orbit,
};

/** A folder the agent owns, used to show the installed app's own icon. */
const AGENT_APP_PATH: Record<string, string> = {
  "claude-code": ".claude",
  codex: ".codex",
  cursor: ".cursor",
  antigravity: "Library/Application Support/Antigravity",
};

function AgentIcon({ id, className }: { id: string; className?: string }) {
  const home = useApp((s) => s.env?.homeDir);
  const Icon = AGENT_ICON[id] ?? Blocks;
  const path = home && AGENT_APP_PATH[id] ? `${home}/${AGENT_APP_PATH[id]}` : null;
  return (
    <AppIcon
      path={path}
      className={cn("size-7", className)}
      fallback={
        <span
          className={cn(
            "flex size-7 shrink-0 items-center justify-center rounded-md bg-panel-raised",
            className,
          )}
        >
          <Icon className="size-3.5 text-muted-foreground" strokeWidth={1.75} aria-hidden />
        </span>
      }
    />
  );
}

function counts(agent: AgentReport, reviewable: Set<string>) {
  const risky = agent.permissions.filter((p) => p.risk).length;
  const inactive = agent.skills.filter((s) => s.state === "inactive").length;
  const cleanup = agent.cleanup.filter(
    (i) => i.recommendationId && reviewable.has(i.recommendationId),
  );
  return {
    risky,
    inactive,
    cleanup,
    cleanupBytes: cleanup.reduce((a, i) => a + i.bytes, 0),
  };
}

function AgentPicker({
  agents,
  selected,
  onSelect,
}: {
  agents: AgentReport[];
  selected: string;
  onSelect: (id: string) => void;
}) {
  const reviewable = useReviewable();
  return (
    <div role="tablist" aria-label="Agent" className="grid grid-cols-2 gap-2 lg:grid-cols-4">
      {agents.map((a) => {
        const c = counts(a, reviewable);
        const active = a.id === selected;
        return (
          <button
            key={a.id}
            type="button"
            role="tab"
            aria-selected={active}
            disabled={!a.detected}
            onClick={() => onSelect(a.id)}
            className={cn(
              "flex min-w-0 items-center gap-2.5 rounded-lg border px-3 py-2.5 text-left transition-colors",
              active
                ? "border-accent bg-panel shadow-sm"
                : "border-border bg-panel/60 hover:bg-hover",
              !a.detected && "cursor-default opacity-50 hover:bg-transparent",
            )}
          >
            <AgentIcon id={a.id} />
            <span className="min-w-0 flex-1">
              <span className="block truncate text-[13px] font-medium">{a.name}</span>
              <span className="block truncate text-2xs text-subtle-foreground">
                {!a.detected
                  ? "Not found on this Mac"
                  : [a.version, formatBytes(a.totalBytes), c.risky ? `${c.risky} to look at` : null]
                      .filter(Boolean)
                      .join(" · ")}
              </span>
            </span>
            {a.detected && c.risky ? (
              <AlertTriangle
                className="size-3.5 shrink-0 text-[var(--tier-review)]"
                aria-label="Has permissions to look at"
              />
            ) : null}
          </button>
        );
      })}
    </div>
  );
}

function Tabs({ agent }: { agent: AgentReport }) {
  const tab = useAgents((s) => s.tab);
  const setTab = useAgents((s) => s.setTab);
  const reviewable = useReviewable();
  const c = counts(agent, reviewable);
  const tabs: { id: AgentTab; label: string; note?: string; attention?: boolean }[] = [
    {
      id: "permissions",
      label: "Permissions",
      note: String(agent.permissions.length),
      attention: c.risky > 0,
    },
    { id: "connections", label: "MCP servers", note: String(agent.connections.length) },
    {
      id: "plugins",
      label: agent.plugins.some((p) => p.kind === "extension") ? "Extensions" : "Plugins",
      note: String(agent.plugins.length),
    },
    {
      id: "skills",
      label: "Skills",
      note: c.inactive ? `${c.inactive} inactive` : String(agent.skills.length),
      attention: c.inactive > 0,
    },
    {
      id: "cleanup",
      label: "Clean up",
      note: c.cleanup.length ? formatBytes(c.cleanupBytes) : "0",
    },
  ];
  return (
    <div
      role="tablist"
      aria-label={`${agent.name} sections`}
      className="flex [scrollbar-width:none] gap-1 overflow-x-auto border-b border-border"
    >
      {tabs.map((t) => (
        <button
          key={t.id}
          type="button"
          role="tab"
          id={`agent-tab-${t.id}`}
          aria-selected={tab === t.id}
          aria-controls="agent-tabpanel"
          onClick={() => setTab(t.id)}
          className={cn(
            "-mb-px flex shrink-0 items-center gap-1.5 border-b-2 px-2.5 py-2 text-xs transition-colors",
            tab === t.id
              ? "border-accent font-medium text-foreground"
              : "border-transparent text-muted-foreground hover:text-foreground",
          )}
        >
          {t.label}
          <span
            className={cn(
              "tabular rounded-full px-1.5 text-2xs",
              t.attention
                ? "bg-[color-mix(in_oklab,var(--tier-review)_16%,transparent)] text-[var(--tier-review)]"
                : "bg-panel-raised text-subtle-foreground",
            )}
          >
            {t.note}
          </span>
        </button>
      ))}
    </div>
  );
}

function ConfirmDialog({
  confirm,
  onClose,
}: {
  confirm: Confirmation | null;
  onClose: () => void;
}) {
  const busy = useAgents((s) => s.busy);
  return (
    <Dialog open={!!confirm} onOpenChange={(o) => !o && !busy && onClose()}>
      <DialogContent className="w-[min(440px,calc(100vw-32px))]" showClose={!busy}>
        {confirm ? (
          <>
            <DialogHeader>
              <DialogTitle>{confirm.title}</DialogTitle>
              <DialogDescription asChild>
                <div className="space-y-2">{confirm.body}</div>
              </DialogDescription>
            </DialogHeader>
            <DialogFooter>
              <Button variant="secondary" onClick={onClose} disabled={!!busy}>
                Cancel
              </Button>
              <Button
                variant={confirm.destructive ? "destructive" : "default"}
                disabled={!!busy}
                onClick={async () => {
                  // A failed change shows its error on the page.
                  await confirm.run();
                  onClose();
                }}
              >
                {busy ? <Loader2 className="animate-spin" /> : null}
                {confirm.action}
              </Button>
            </DialogFooter>
          </>
        ) : null}
      </DialogContent>
    </Dialog>
  );
}

export function AgentsPage() {
  const { report, loading, error, selected, tab, select, load, analyze, dismissError } =
    useAgents();
  const [confirm, setConfirm] = useState<Confirmation | null>(null);

  useEffect(() => {
    void load();
  }, [load]);

  const detected = useMemo(() => report?.agents.filter((a) => a.detected) ?? [], [report]);
  const agent = report?.agents.find((a) => a.id === selected && a.detected) ?? detected[0] ?? null;

  return (
    <>
      <PageHeader
        title="AI Agents"
        description="What each coding agent may do, what it connects to and what it has installed."
        actions={
          <div className="flex items-center gap-3">
            {report ? (
              <span className="text-2xs text-subtle-foreground">
                Checked {formatRelative(report.generatedAtMs)}
              </span>
            ) : null}
            <Button variant="secondary" onClick={() => void analyze()} disabled={loading}>
              {loading ? <Loader2 className="animate-spin" /> : <RefreshCw />}
              Check again
            </Button>
          </div>
        }
      />
      <div className="min-h-0 flex-1 overflow-y-auto">
        <div className="mx-auto max-w-[1180px] space-y-4 px-6 py-5">
          {error ? (
            <div
              role="alert"
              className="flex items-start gap-2 rounded-md border border-[var(--critical)]/40 bg-[color-mix(in_oklab,var(--critical)_8%,transparent)] px-3 py-2 text-xs"
            >
              <AlertTriangle className="mt-px size-3.5 shrink-0 text-[var(--critical)]" />
              <p className="flex-1 leading-relaxed">{error}</p>
              <button
                type="button"
                onClick={dismissError}
                aria-label="Dismiss"
                className="rounded p-0.5 text-muted-foreground hover:bg-hover"
              >
                <X className="size-3.5" />
              </button>
            </div>
          ) : null}

          {!report ? (
            <Panel>
              <EmptyState
                icon={loading ? Loader2 : Blocks}
                title={loading ? "Reading agent settings…" : "Not checked yet"}
                description="Reads the settings of Claude Code, Codex, Cursor and Antigravity. Nothing is changed."
              />
            </Panel>
          ) : !agent ? (
            <Panel>
              <EmptyState
                icon={Blocks}
                title="No AI agents found"
                description="DiskPilot looks for Claude Code, Codex, Cursor and Antigravity in your home folder."
              />
            </Panel>
          ) : (
            <>
              <AgentPicker agents={report.agents} selected={agent.id} onSelect={select} />
              <Panel>
                <div className="px-4 pt-1">
                  <Tabs agent={agent} />
                </div>
                <div
                  id="agent-tabpanel"
                  role="tabpanel"
                  aria-labelledby={`agent-tab-${tab}`}
                  className="px-4 py-3"
                >
                  {tab === "permissions" ? (
                    <PermissionsList agent={agent} onConfirm={setConfirm} />
                  ) : tab === "connections" ? (
                    <ConnectionsList
                      agent={agent}
                      disconnected={report.disconnected.filter((d) => d.agent === agent.id)}
                      onConfirm={setConfirm}
                    />
                  ) : tab === "plugins" ? (
                    <PluginsList agent={agent} onConfirm={setConfirm} />
                  ) : tab === "skills" ? (
                    <SkillsList agent={agent} onConfirm={setConfirm} />
                  ) : (
                    <CleanupList agent={agent} />
                  )}
                </div>
              </Panel>
            </>
          )}
        </div>
      </div>
      <ConfirmDialog confirm={confirm} onClose={() => setConfirm(null)} />
    </>
  );
}
