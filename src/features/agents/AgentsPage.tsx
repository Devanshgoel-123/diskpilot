import { openUrl } from "@tauri-apps/plugin-opener";
import {
  AlertTriangle,
  Blocks,
  Bot,
  Loader2,
  MessageSquarePlus,
  RefreshCw,
  ShieldCheck,
  X,
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
import { formatBytes, formatCount, formatRelative } from "@/lib/format";
import { useApp } from "@/lib/store";
import type { AgentReport } from "@/lib/types";
import { isWindows, words } from "@/lib/platform";
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
import { TranscriptSecrets } from "@/features/developer/TranscriptSecrets";

/** The app bundle on macOS, when the agent has one; its own icon is the best logo. */
const AGENT_APP: Record<string, string> = {
  codex: "/Applications/Codex.app",
  cursor: "/Applications/Cursor.app",
  antigravity: "/Applications/Antigravity.app",
  "claude-desktop": "/Applications/Claude.app",
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
      {BrandMark({ id, className: mark }) ??
        (id === "ai-agents" ? (
          <Bot className={cn(mark, "text-muted-foreground")} strokeWidth={1.75} aria-hidden />
        ) : null) ?? (
          <span className="text-xs font-semibold text-muted-foreground">
            {id.charAt(0).toUpperCase()}
          </span>
        )}
    </span>
  );
  const app = isWindows() ? undefined : AGENT_APP[id];
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

/** One tab: name and count always; a short note only when it needs a look. */
function SectionTab({
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
      title={note}
      className={cn(
        "-mb-px flex shrink-0 items-center gap-1.5 border-b-2 py-2 text-[13px] transition-colors duration-[120ms]",
        active
          ? "border-accent font-medium text-foreground"
          : "border-transparent text-muted-foreground hover:text-foreground",
      )}
    >
      {label}
      <span className="tabular text-xs font-normal text-subtle-foreground">{value}</span>
      {tone === "warn" ? (
        <span className="flex items-center gap-1 text-xs font-normal text-tier-review">
          <span className="size-[5px] rounded-full bg-tier-review" aria-hidden />
          {note}
        </span>
      ) : (
        <span className="sr-only">{note}</span>
      )}
    </button>
  );
}

function SectionTabs({ agent }: { agent: AgentReport }) {
  const s = useStats(agent);
  const extensions = agent.plugins.some((p) => p.kind === "extension");
  const knowsUse = agent.skills.some((k) => k.state !== "unknown");
  return (
    <div
      role="tablist"
      aria-label={`${agent.name} sections`}
      className="flex gap-5 overflow-x-auto border-b border-border"
    >
      <SectionTab
        id="permissions"
        label="Permissions"
        value={formatCount(agent.permissions.length)}
        note={s.risky ? `${s.risky} need${s.risky === 1 ? "s" : ""} a look` : "Nothing risky"}
        tone={s.risky ? "warn" : "good"}
      />
      <SectionTab
        id="connections"
        label="MCP servers"
        value={
          agent.connections.length
            ? `${formatCount(s.serversOn)} of ${formatCount(agent.connections.length)} on`
            : "0"
        }
        note={agent.connections.length ? `${s.serversOn} connected` : "None set up"}
        tone="plain"
      />
      <SectionTab
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
      <SectionTab
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
      <SectionTab
        id="cleanup"
        label="Can free"
        value={s.cleanupCount ? formatBytes(s.cleanupBytes) : "0"}
        note={
          s.cleanupCount
            ? `${s.cleanupCount} item${s.cleanupCount === 1 ? "" : "s"} to review`
            : "Nothing to clean"
        }
        tone="plain"
      />
    </div>
  );
}

/** The one sentence a person should read first about this agent. */
function Posture({ agent }: { agent: AgentReport }) {
  const risky = agent.permissions.filter((p) => p.risk).length;
  if (!risky) {
    return (
      <p className="flex items-center gap-2 text-xs text-muted-foreground">
        <ShieldCheck className="size-3.5 shrink-0 text-tier-safe" aria-hidden />
        No setting lets {agent.name} act without asking you first.
      </p>
    );
  }
  return (
    <div className="flex gap-2.5 rounded-lg bg-review-soft px-3.5 py-3">
      <AlertTriangle className="mt-px size-4 shrink-0 text-tier-review" aria-hidden />
      <div className="min-w-0">
        <p className="text-[13px] font-semibold">
          {risky === 1 ? "One setting lets" : `${formatCount(risky)} settings let`} {agent.name} act
          without asking you first.
        </p>
        <p className="mt-0.5 text-xs text-muted-foreground">
          They&apos;re listed first under Permissions. Removing a rule edits one settings file and
          saves a copy of it beforehand.
        </p>
      </div>
    </div>
  );
}

/** The Claude app, Copilot and other agent CLIs, from the Developer Tools analysis. */
function RailItem({
  id,
  name,
  detail,
  on,
  risky,
  onSelect,
}: {
  id: string;
  name: string;
  detail: string;
  on: boolean;
  risky?: number;
  onSelect: () => void;
}) {
  return (
    <button
      type="button"
      role="radio"
      aria-checked={on}
      onClick={onSelect}
      className={cn(
        "flex w-full items-center gap-2.5 rounded-md px-2 py-2 text-left transition-colors duration-[120ms]",
        on ? "bg-selected" : "hover:bg-hover",
      )}
    >
      <AgentLogo id={id} size="sm" />
      <span className="min-w-0 flex-1">
        <span className="block truncate text-[13px] font-medium">{name}</span>
        <span className="block truncate font-mono text-2xs text-subtle-foreground">{detail}</span>
      </span>
      {risky ? (
        <span
          className="tabular flex items-center gap-1 text-xs text-tier-review"
          title={`${risky} need${risky === 1 ? "s" : ""} a look`}
        >
          <span className="size-[5px] rounded-full bg-tier-review" aria-hidden />
          {risky}
        </span>
      ) : null}
    </button>
  );
}

/**
 * Every agent on this computer, with how many of its settings need a look. Stays
 * in view while the selected one scrolls.
 */
function AgentRail({
  agents,
  selected,
  onSelect,
}: {
  agents: AgentReport[];
  selected: string;
  onSelect: (id: string) => void;
}) {
  const missing = ["Claude Code", "Codex", "Cursor", "Antigravity"].filter(
    (n) => !agents.some((a) => a.name === n),
  );
  const riskyTotal = agents.reduce((n, a) => n + a.permissions.filter((p) => p.risk).length, 0);
  const riskyAgents = agents.filter((a) => a.permissions.some((p) => p.risk)).length;
  return (
    <nav aria-label="Agents" className="space-y-4 md:sticky md:top-0 md:self-start">
      <div role="radiogroup" aria-label="Agent" className="space-y-4">
        <div className="space-y-1">
          <p className="px-2 text-2xs font-semibold text-subtle-foreground">Coding agents</p>
          <div className="space-y-px">
            {agents.map((a) => (
              <RailItem
                key={a.id}
                id={a.id}
                name={a.name}
                detail={[a.version, formatBytes(a.totalBytes)].filter(Boolean).join(" · ")}
                on={a.id === selected}
                risky={a.permissions.filter((p) => p.risk).length}
                onSelect={() => onSelect(a.id)}
              />
            ))}
          </div>
          {missing.length ? (
            <p className="px-2 pt-1 text-2xs text-subtle-foreground">
              Not installed: {missing.join(", ")}
            </p>
          ) : null}
        </div>
      </div>
      {riskyTotal ? (
        <p className="mx-2 border-t border-border pt-3 text-xs text-muted-foreground">
          <span className="tabular font-medium text-tier-review">{formatCount(riskyTotal)}</span>{" "}
          {riskyTotal === 1 ? "setting lets" : "settings let"} agents act without asking, across{" "}
          {riskyAgents} of {agents.length} agents.
        </p>
      ) : null}
    </nav>
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

  // Storage details come from the Developer Tools analysis; run it if it hasn't.
  const devtoolsReady = useApp((s) => !!s.devtools);
  const analyzeDevTools = useApp((s) => s.analyzeDevTools);
  useEffect(() => {
    if (!devtoolsReady) void analyzeDevTools();
  }, [devtoolsReady, analyzeDevTools]);

  const detected = useMemo(() => report?.agents.filter((a) => a.detected) ?? [], [report]);
  const agent = detected.find((a) => a.id === selected) ?? detected[0] ?? null;
  const showRail = detected.length > 1;

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
        <div className="page-content space-y-4">
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
                description={`Reads the settings of the coding agents on this ${words().computer}. Nothing is changed.`}
              />
            </Panel>
          ) : (
            <div className={cn("grid gap-8", showRail && "md:grid-cols-[208px_minmax(0,1fr)]")}>
              {showRail ? (
                <AgentRail agents={detected} selected={agent?.id ?? ""} onSelect={select} />
              ) : null}
              <div className="min-w-0 space-y-8">
                {agent ? (
                  <section aria-label={agent.name} className="space-y-4">
                    <div className="flex items-center gap-3">
                      <AgentLogo id={agent.id} size="md" />
                      <div className="min-w-0">
                        <h2 className="text-[17px] leading-[22px] font-semibold tracking-tight">
                          {agent.name}
                        </h2>
                        <p className="truncate font-mono text-2xs text-subtle-foreground">
                          {[
                            agent.version,
                            `${formatBytes(agent.totalBytes)} on disk`,
                            agent.permissions[0]?.source,
                          ]
                            .filter(Boolean)
                            .join(" · ")}
                        </p>
                      </div>
                    </div>
                    <Posture agent={agent} />
                    <SectionTabs agent={agent} />
                    <div
                      id="agent-tabpanel"
                      role="tabpanel"
                      aria-labelledby={`agent-tab-${tab}`}
                      className="pt-1"
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
                  </section>
                ) : (
                  <Panel>
                    <EmptyState
                      icon={Blocks}
                      title="No coding agents found"
                      description="DiskPilot supports Claude Code, Codex, Cursor and Antigravity."
                    />
                  </Panel>
                )}
                <div className="space-y-3">
                  <TranscriptSecrets />
                  <RequestAgent />
                </div>
              </div>
            </div>
          )}
        </div>
      </div>
      <ConfirmDialog confirm={confirm} onClose={() => setConfirm(null)} />
    </>
  );
}
