import { openUrl } from "@tauri-apps/plugin-opener";
import { AlertTriangle, Blocks, Loader2, MessageSquarePlus, RefreshCw, X } from "lucide-react";
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
import { formatBytes, formatCount, formatRelative } from "@/lib/format";
import { useApp } from "@/lib/store";
import type { AgentReport } from "@/lib/types";
import { cn } from "@/lib/utils";

import {
  CleanupList,
  ConnectionsList,
  PermissionsList,
  PluginsList,
  SkillsList,
} from "./AgentSections";
import { type AgentTab, useAgents, useReviewable } from "./agentsStore";
import { BrandMark } from "./logos";
import { type Confirmation } from "./shared";

/** The app bundle, when the agent has one; its own icon is the best logo. */
const AGENT_APP: Record<string, string> = {
  codex: "/Applications/Codex.app",
  cursor: "/Applications/Cursor.app",
  antigravity: "/Applications/Antigravity.app",
};

const REPO_ISSUES = "https://github.com/Devanshgoel-123/diskpilot/issues/new";
const REQUESTABLE = ["Windsurf", "GitHub Copilot", "Gemini CLI", "Cline", "Zed", "Kiro"];

export function AgentLogo({ id, size = "md" }: { id: string; size?: "sm" | "md" | "lg" }) {
  const box =
    size === "lg" ? "size-10 rounded-xl" : size === "md" ? "size-8 rounded-lg" : "size-5 rounded";
  const mark = size === "lg" ? "size-5" : size === "md" ? "size-4" : "size-3";
  const fallback = (
    <span
      className={cn(
        "flex shrink-0 items-center justify-center border border-border bg-panel-raised",
        box,
      )}
    >
      {BrandMark({ id, className: mark }) ?? (
        <span className="text-xs font-semibold text-muted-foreground">
          {id.charAt(0).toUpperCase()}
        </span>
      )}
    </span>
  );
  const app = AGENT_APP[id];
  if (!app) return fallback;
  return <AppIcon path={app} className={cn("shrink-0", box)} fallback={fallback} />;
}

interface Stats {
  risky: number;
  serversOn: number;
  unusedPlugins: number;
  inactive: number;
  cleanupCount: number;
  cleanupBytes: number;
}

function useStats(agent: AgentReport): Stats {
  const reviewable = useReviewable();
  return useMemo(() => {
    // Skills of a plugin that is turned off are not loaded, so they don't count.
    const off = new Set(agent.plugins.filter((p) => p.enabled === false).map((p) => p.name));
    const pluginSkills = new Map<string, { total: number; inactive: number }>();
    for (const s of agent.skills) {
      if (!s.plugin) continue;
      const v = pluginSkills.get(s.plugin) ?? { total: 0, inactive: 0 };
      v.total += 1;
      if (s.state === "inactive") v.inactive += 1;
      pluginSkills.set(s.plugin, v);
    }
    const cleanup = agent.cleanup.filter(
      (i) => i.recommendationId && reviewable.has(i.recommendationId),
    );
    return {
      risky: agent.permissions.filter((p) => p.risk).length,
      serversOn: agent.connections.filter((c) => c.enabled).length,
      unusedPlugins: agent.plugins.filter((p) => {
        const st = pluginSkills.get(p.name);
        return p.enabled !== false && !!st && st.inactive === st.total;
      }).length,
      inactive: agent.skills.filter((s) => s.state === "inactive" && !off.has(s.plugin ?? ""))
        .length,
      cleanupCount: cleanup.length,
      cleanupBytes: cleanup.reduce((a, i) => a + i.bytes, 0),
    };
  }, [agent, reviewable]);
}

type Tone = "good" | "warn" | "plain";

function StatTile({
  id,
  label,
  value,
  note,
  tone,
}: {
  id: AgentTab;
  label: string;
  value: string;
  note: string;
  tone: Tone;
}) {
  const tab = useAgents((s) => s.tab);
  const setTab = useAgents((s) => s.setTab);
  const active = tab === id;
  return (
    <button
      type="button"
      role="tab"
      id={`agent-tab-${id}`}
      aria-selected={active}
      aria-controls="agent-tabpanel"
      onClick={() => setTab(id)}
      className={cn(
        "min-w-0 rounded-lg border px-3 py-2.5 text-left transition-colors",
        active
          ? "border-accent bg-[color-mix(in_oklab,var(--accent)_7%,transparent)]"
          : "border-border hover:bg-hover",
      )}
    >
      <span className="block text-xs text-muted-foreground">{label}</span>
      <span className="tabular mt-0.5 block text-[20px] leading-7 font-semibold tracking-tight">
        {value}
      </span>
      <span
        className={cn(
          "mt-0.5 flex items-center gap-1.5 truncate text-xs",
          tone === "warn"
            ? "text-tier-review"
            : tone === "good"
              ? "text-tier-safe"
              : "text-subtle-foreground",
        )}
      >
        {tone === "warn" ? (
          <span className="size-1.5 shrink-0 rounded-full bg-tier-review" />
        ) : null}
        {note}
      </span>
    </button>
  );
}

function StatTiles({ agent }: { agent: AgentReport }) {
  const s = useStats(agent);
  const extensions = agent.plugins.some((p) => p.kind === "extension");
  const knowsUse = agent.skills.some((k) => k.state !== "unknown");
  return (
    <div
      role="tablist"
      aria-label={`${agent.name} sections`}
      className="grid grid-cols-2 gap-2 sm:grid-cols-3 lg:grid-cols-5"
    >
      <StatTile
        id="permissions"
        label="Permissions"
        value={formatCount(agent.permissions.length)}
        note={s.risky ? `${s.risky} need${s.risky === 1 ? "s" : ""} a look` : "Nothing risky"}
        tone={s.risky ? "warn" : "good"}
      />
      <StatTile
        id="connections"
        label="MCP servers"
        value={formatCount(agent.connections.length)}
        note={agent.connections.length ? `${s.serversOn} connected` : "None set up"}
        tone="plain"
      />
      <StatTile
        id="plugins"
        label={extensions ? "Extensions" : "Plugins"}
        value={formatCount(agent.plugins.length)}
        note={
          s.unusedPlugins
            ? `${s.unusedPlugins} unused`
            : agent.plugins.length
              ? extensions
                ? "Installed"
                : "All in use"
              : "None installed"
        }
        tone={s.unusedPlugins ? "warn" : "plain"}
      />
      <StatTile
        id="skills"
        label="Skills"
        value={formatCount(agent.skills.length)}
        note={
          s.inactive
            ? `${formatCount(s.inactive)} inactive`
            : !agent.skills.length
              ? "None installed"
              : knowsUse
                ? "All in use"
                : "Use not recorded"
        }
        tone={s.inactive ? "warn" : knowsUse && agent.skills.length ? "good" : "plain"}
      />
      <StatTile
        id="cleanup"
        label="Can free"
        value={s.cleanupCount ? formatBytes(s.cleanupBytes) : "0"}
        note={
          s.cleanupCount
            ? `${s.cleanupCount} item${s.cleanupCount === 1 ? "" : "s"} to review`
            : "Nothing to clean"
        }
        tone={s.cleanupCount ? "warn" : "good"}
      />
    </div>
  );
}

function AgentSwitcher({
  agents,
  selected,
  onSelect,
}: {
  agents: AgentReport[];
  selected: string;
  onSelect: (id: string) => void;
}) {
  return (
    <div
      role="radiogroup"
      aria-label="Agent"
      className="inline-flex flex-wrap gap-1 rounded-lg border border-border bg-panel p-1"
    >
      {agents.map((a) => (
        <button
          key={a.id}
          type="button"
          role="radio"
          aria-checked={a.id === selected}
          onClick={() => onSelect(a.id)}
          className={cn(
            "flex items-center gap-2 rounded-md px-3 py-1.5 text-[13px] transition-colors",
            a.id === selected
              ? "bg-panel-raised font-medium text-foreground shadow-sm"
              : "text-muted-foreground hover:bg-hover hover:text-foreground",
          )}
        >
          <AgentLogo id={a.id} size="sm" />
          {a.name}
        </button>
      ))}
    </div>
  );
}

function RequestAgent() {
  const version = useApp((s) => s.env?.appVersion);
  const ask = (name: string) => {
    const title = name ? `Support ${name} in AI Agents` : "Support another agent in AI Agents";
    const body = [
      `Which agent: ${name || "(name it here)"}`,
      "",
      "What would you like DiskPilot to show or clean up for it?",
      "",
      version ? `DiskPilot ${version}` : "",
    ].join("\n");
    void openUrl(
      `${REPO_ISSUES}?labels=agent-request&title=${encodeURIComponent(title)}&body=${encodeURIComponent(body)}`,
    );
  };
  return (
    <Panel className="flex flex-wrap items-center gap-x-4 gap-y-3 px-4 py-3">
      <MessageSquarePlus className="size-4 shrink-0 text-muted-foreground" aria-hidden />
      <div className="min-w-0 flex-1">
        <p className="text-[13px] font-medium">Use another coding agent?</p>
        <p className="text-xs text-muted-foreground">
          Ask for it to be supported. This opens a GitHub issue you can edit before sending.
        </p>
      </div>
      <div className="flex flex-wrap gap-1.5">
        {REQUESTABLE.map((n) => (
          <Button key={n} size="sm" variant="secondary" onClick={() => ask(n)}>
            {n}
          </Button>
        ))}
        <Button size="sm" variant="ghost" onClick={() => ask("")}>
          Other…
        </Button>
      </div>
    </Panel>
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
      <DialogContent className="w-[min(460px,calc(100vw-32px))]" showClose={!busy}>
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
  const agent = detected.find((a) => a.id === selected) ?? detected[0] ?? null;

  return (
    <>
      <PageHeader
        title="AI Agents"
        description="What your coding agents may do, what they connect to and what they have installed."
        actions={
          <div className="flex items-center gap-3">
            {report ? (
              <span className="text-xs text-subtle-foreground">
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
              className="flex items-start gap-2 rounded-lg border border-critical/40 bg-critical/10 px-3 py-2 text-[13px]"
            >
              <AlertTriangle className="mt-0.5 size-3.5 shrink-0 text-critical" />
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
                description="Reads the settings of the coding agents on this Mac. Nothing is changed."
              />
            </Panel>
          ) : !agent ? (
            <Panel>
              <EmptyState
                icon={Blocks}
                title="No coding agents found"
                description="DiskPilot supports Claude Code, Codex, Cursor and Antigravity."
              />
            </Panel>
          ) : (
            <>
              {detected.length > 1 ? (
                <AgentSwitcher agents={detected} selected={agent.id} onSelect={select} />
              ) : null}
              <Panel className="space-y-4 px-4 py-4">
                <div className="flex items-center gap-3">
                  <AgentLogo id={agent.id} size="lg" />
                  <div className="min-w-0">
                    <h2 className="text-[15px] font-semibold tracking-tight">{agent.name}</h2>
                    <p className="text-xs text-muted-foreground">
                      {[
                        agent.version ? `Version ${agent.version}` : null,
                        `${formatBytes(agent.totalBytes)} on disk`,
                      ]
                        .filter(Boolean)
                        .join(" · ")}
                    </p>
                  </div>
                </div>
                <StatTiles agent={agent} />
              </Panel>
              <Panel
                id="agent-tabpanel"
                role="tabpanel"
                aria-labelledby={`agent-tab-${tab}`}
                className="px-4 py-4"
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
              </Panel>
            </>
          )}
          {report ? <RequestAgent /> : null}
        </div>
      </div>
      <ConfirmDialog confirm={confirm} onClose={() => setConfirm(null)} />
    </>
  );
}
