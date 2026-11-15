import { AlertTriangle, Loader2, Plug, Undo2 } from "lucide-react";
import { type ReactNode, useMemo, useState } from "react";

import { Pager } from "@/components/shared/Pager";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import { api } from "@/lib/api";
import { formatBytes, formatCount, formatRelative, tildify } from "@/lib/format";
import { useApp } from "@/lib/store";
import type {
  AgentPermission,
  AgentPlugin,
  AgentReport,
  AgentSkill,
  DisconnectedServer,
} from "@/lib/types";
import { usePage } from "@/lib/usePage";
import { cn } from "@/lib/utils";

import { ToolCard } from "@/features/developer/ToolCard";

import { useAgents, useReviewable } from "./agentsStore";
import { isOneTime, modeValue, ruleArea, ruleText } from "./permissionText";
import { type Confirmation, Tint } from "./shared";

const REVIEW = "var(--tier-review)";
const SAFE = "var(--tier-safe)";
const KEEP = "var(--tier-keep)";
const PAGE_SIZE = 10;
const MONTH_MS = 30 * 86_400_000;

// ---- Building blocks -------------------------------------------------------------

function Section({
  title,
  description,
  count,
  action,
  children,
}: {
  title: string;
  description?: ReactNode;
  count?: number;
  action?: ReactNode;
  children: ReactNode;
}) {
  return (
    <section className="space-y-2">
      <div className="flex flex-wrap items-end justify-between gap-x-4 gap-y-2">
        <div className="min-w-0">
          <h3 className="text-[13px] font-semibold">
            {title}
            {count !== undefined ? (
              <span className="tabular ml-2 font-normal text-subtle-foreground">
                {formatCount(count)}
              </span>
            ) : null}
          </h3>
          {description ? (
            <p className="mt-0.5 max-w-[680px] text-xs leading-relaxed text-muted-foreground">
              {description}
            </p>
          ) : null}
        </div>
        {action}
      </div>
      {children}
    </section>
  );
}

/** A bordered list, a page at a time. */
function PagedList<T>({
  items,
  render,
  label,
  size = PAGE_SIZE,
}: {
  items: T[];
  render: (item: T, index: number) => ReactNode;
  label: string;
  size?: number;
}) {
  const page = usePage(items, size, items.length);
  return (
    <div className="overflow-hidden rounded-lg border border-border">
      <ul className="divide-y divide-border">{page.items.map(render)}</ul>
      <Pager page={page} label={label} className="border-t border-border px-3 py-1.5" />
    </div>
  );
}

function Row({
  title,
  sub,
  meta,
  action,
  tone,
}: {
  title: ReactNode;
  sub?: ReactNode;
  meta?: ReactNode;
  action?: ReactNode;
  tone?: "warn";
}) {
  return (
    <li
      className={cn(
        "flex min-h-12 items-center gap-4 px-3 py-2",
        tone === "warn" && "bg-review-soft",
      )}
    >
      <div className="min-w-0 flex-1">
        <div className="flex min-w-0 items-center gap-2 text-[13px]">{title}</div>
        {sub ? <div className="mt-0.5 truncate text-xs text-muted-foreground">{sub}</div> : null}
      </div>
      {meta ? (
        <div className="hidden shrink-0 text-right text-xs text-muted-foreground sm:block">
          {meta}
        </div>
      ) : null}
      {action ? <div className="flex shrink-0 items-center gap-1.5">{action}</div> : null}
    </li>
  );
}

function Empty({ children }: { children: ReactNode }) {
  return (
    <p className="rounded-lg border border-dashed border-border px-3 py-6 text-center text-[13px] text-muted-foreground">
      {children}
    </p>
  );
}

function Footnote({ children }: { children: ReactNode }) {
  return <p className="text-xs leading-relaxed text-subtle-foreground">{children}</p>;
}

function ActionButton({
  busyKey,
  children,
  variant = "secondary",
  ...props
}: React.ComponentProps<typeof Button> & { busyKey: string }) {
  const busy = useAgents((s) => s.busy);
  return (
    <Button size="sm" variant={variant} disabled={!!busy} {...props}>
      {busy === busyKey ? <Loader2 className="animate-spin" /> : null}
      {children}
    </Button>
  );
}

function Command({ children, title }: { children: ReactNode; title?: string }) {
  return (
    <code className="min-w-0 truncate font-mono text-xs text-foreground" title={title}>
      {children}
    </code>
  );
}

function Area({ children }: { children: ReactNode }) {
  return (
    <span className="shrink-0 rounded bg-panel-raised px-1.5 py-px text-[11px] font-medium text-muted-foreground">
      {children}
    </span>
  );
}

// ---- Permissions ---------------------------------------------------------------

export function PermissionsList({
  agent,
  onConfirm,
}: {
  agent: AgentReport;
  onConfirm: (c: Confirmation) => void;
}) {
  const change = useAgents((s) => s.change);
  const perms = agent.permissions;
  const risky = perms.filter((p) => p.risk);
  const settings = perms.filter((p) => p.kind === "mode" && !p.risk);
  const allowed = perms.filter((p) => p.kind === "allow" && !p.risk);
  const patterns = allowed.filter((p) => !isOneTime(p));
  const oneTime = allowed.filter(isOneTime);
  const asks = perms.filter((p) => p.kind === "ask");
  const blocked = perms.filter((p) => p.kind === "deny");
  const trusted = perms.filter((p) => p.kind === "trust");
  const hooks = perms.filter((p) => p.kind === "hook");
  const removableOneTime = oneTime.filter((p) => p.id);

  if (!perms.length) {
    return (
      <Empty>
        No permission settings found, so {agent.name} uses its defaults and asks before acting.
      </Empty>
    );
  }

  const removeOne = (p: AgentPermission) =>
    onConfirm({
      title: "Remove this rule?",
      destructive: true,
      action: "Remove",
      body: (
        <>
          <code className="block rounded-md bg-panel-raised px-2 py-1.5 font-mono text-xs break-all text-foreground">
            {p.value}
          </code>
          <p>
            {p.kind === "allow"
              ? `${agent.name} will ask you again before doing this.`
              : `${agent.name} will no longer apply this rule.`}{" "}
            A copy of {p.source} is saved first.
          </p>
        </>
      ),
      run: () =>
        change(`perm:${p.id}`, () => api.agentRemovePermissions(agent.id, [p.id as string])),
    });

  const removeOneTime = () =>
    onConfirm({
      title: `Clear ${removableOneTime.length} one-time approvals?`,
      destructive: true,
      action: "Clear approvals",
      body: (
        <p>
          {agent.name} will ask again the next time it wants to run any of these exact commands.
          Patterns and other rules stay. A copy of the settings file is saved first.
        </p>
      ),
      run: () =>
        change("perm:one-time", () =>
          api.agentRemovePermissions(
            agent.id,
            removableOneTime.map((p) => p.id as string),
          ),
        ),
    });

  // Name the file only when rules come from more than one.
  const manyFiles = new Set(perms.map((p) => p.source)).size > 1;
  const ruleRow = (p: AgentPermission, i: number) => (
    <Row
      key={`${p.id ?? p.value}-${i}`}
      tone={p.risk ? "warn" : undefined}
      title={
        <>
          <Area>{ruleArea(p.value)}</Area>
          <Command title={p.value}>{ruleText(p.value)}</Command>
        </>
      }
      sub={p.risk ?? (manyFiles ? p.source : undefined)}
      action={
        p.id ? (
          <ActionButton
            busyKey={`perm:${p.id}`}
            variant="ghost"
            onClick={() => removeOne(p)}
            aria-label={`Remove rule ${p.value}`}
          >
            Remove
          </ActionButton>
        ) : null
      }
    />
  );

  return (
    <div className="space-y-6">
      {risky.length ? (
        <Section
          title="Needs a look"
          count={risky.length}
          description={`These let ${agent.name} act without asking you first.`}
        >
          <ul className="divide-y divide-border overflow-hidden rounded-lg border border-border">
            {risky.map((p, i) => (
              <li key={`${p.value}-${i}`} className="flex min-h-12 items-center gap-3 px-3 py-2.5">
                <AlertTriangle className="size-4 shrink-0 text-tier-review" aria-hidden />
                <div className="min-w-0 flex-1">
                  <p className="flex min-w-0 items-center gap-2 text-[13px]">
                    {p.kind === "mode" ? (
                      <>
                        <span className="font-medium">{p.label}</span>
                        <span className="text-muted-foreground">{modeValue(p.value)}</span>
                      </>
                    ) : (
                      <>
                        <Area>{ruleArea(p.value)}</Area>
                        <Command title={p.value}>{ruleText(p.value)}</Command>
                      </>
                    )}
                  </p>
                  <p className="mt-0.5 text-xs text-muted-foreground">
                    {p.risk}
                    {p.id ? null : ` Change it in ${p.source}.`}
                  </p>
                </div>
                {p.id ? (
                  <ActionButton
                    busyKey={`perm:${p.id}`}
                    onClick={() => removeOne(p)}
                    aria-label={`Remove rule ${p.value}`}
                  >
                    Remove…
                  </ActionButton>
                ) : null}
              </li>
            ))}
          </ul>
        </Section>
      ) : null}

      {settings.length ? (
        <Section title="Settings" description="How cautious the agent is overall.">
          <dl className="grid gap-2 sm:grid-cols-2 lg:grid-cols-3">
            {settings.map((p, i) => (
              <div key={`${p.label}-${i}`} className="rounded-lg border border-border px-3 py-2">
                <dt className="text-xs text-muted-foreground">{p.label}</dt>
                <dd className="mt-0.5 text-[13px] font-medium" title={p.value}>
                  {modeValue(p.value)}
                </dd>
              </div>
            ))}
          </dl>
        </Section>
      ) : null}

      {patterns.length ? (
        <Section
          title="Always allowed"
          count={patterns.length}
          description={`Kinds of actions ${agent.name} may take without asking. “…” stands for anything.`}
        >
          <PagedList items={patterns} label="rules" render={ruleRow} />
        </Section>
      ) : null}

      {oneTime.length ? (
        <Section
          title="Commands you approved once"
          count={oneTime.length}
          description={`Each time you chose “Yes, don't ask again”, ${agent.name} saved that exact command and now runs it without asking. Most were for one task and can be cleared.`}
          action={
            removableOneTime.length > 1 ? (
              <ActionButton busyKey="perm:one-time" onClick={removeOneTime}>
                Clear all {removableOneTime.length}
              </ActionButton>
            ) : null
          }
        >
          <PagedList items={oneTime} label="approved commands" render={ruleRow} />
        </Section>
      ) : null}

      {asks.length ? (
        <Section title="Always asks first" count={asks.length}>
          <PagedList items={asks} label="rules" render={ruleRow} />
        </Section>
      ) : null}

      {blocked.length ? (
        <Section title="Never allowed" count={blocked.length}>
          <PagedList items={blocked} label="rules" render={ruleRow} />
        </Section>
      ) : null}

      {trusted.length ? (
        <Section title="Trusted folders" count={trusted.length}>
          <PagedList
            items={trusted}
            label="folders"
            render={(p, i) => <Row key={`${p.value}-${i}`} title={p.value} sub={p.label} />}
          />
        </Section>
      ) : null}

      {hooks.length ? (
        <Section
          title="Runs by itself"
          count={hooks.length}
          description="Hooks are commands the agent runs on its own at certain moments."
        >
          <PagedList
            items={hooks}
            label="hooks"
            render={(p, i) => <Row key={`${p.label}-${i}`} title={p.label} sub={p.value} />}
          />
        </Section>
      ) : null}

      <Footnote>
        From {Array.from(new Set(perms.map((p) => p.source))).join(", ")}. Rules in each
        project&apos;s own settings are not shown. Passwords and tokens are hidden.
      </Footnote>
    </div>
  );
}

// ---- MCP servers ---------------------------------------------------------------

export function ConnectionsList({
  agent,
  disconnected,
  onConfirm,
}: {
  agent: AgentReport;
  disconnected: DisconnectedServer[];
  onConfirm: (c: Confirmation) => void;
}) {
  const change = useAgents((s) => s.change);
  const canToggle = agent.id === "codex";

  return (
    <div className="space-y-6">
      <Section
        title="MCP servers"
        count={agent.connections.length}
        description={`Tools and data sources ${agent.name} can reach. Disconnect the ones you no longer use.`}
      >
        {agent.connections.length ? (
          <PagedList
            items={agent.connections}
            label="servers"
            render={(c) => (
              <Row
                key={c.id}
                title={
                  <>
                    <Plug className="size-3.5 shrink-0 text-muted-foreground" aria-hidden />
                    <span className="truncate font-medium">{c.name}</span>
                    {c.enabled ? (
                      <Tint color={SAFE}>Connected</Tint>
                    ) : (
                      <Tint color={KEEP}>{canToggle ? "Off" : "Off in this project"}</Tint>
                    )}
                  </>
                }
                sub={[c.transport, c.target, c.scope].filter(Boolean).join(" · ")}
                action={
                  canToggle && !c.enabled ? (
                    <ActionButton
                      busyKey={`conn:${c.id}`}
                      variant="positive"
                      onClick={() =>
                        void change(`conn:${c.id}`, () =>
                          api.agentSetConnection(agent.id, c.id, true),
                        )
                      }
                    >
                      Reconnect
                    </ActionButton>
                  ) : (
                    <ActionButton
                      busyKey={`conn:${c.id}`}
                      aria-label={`Disconnect ${c.name}`}
                      onClick={() =>
                        onConfirm({
                          title: `Disconnect ${c.name}?`,
                          action: "Disconnect",
                          destructive: true,
                          body: (
                            <>
                              <p>
                                {agent.name} stops using this server from its next session.
                                {canToggle
                                  ? " It stays in config.toml, turned off, so you can reconnect it here."
                                  : " DiskPilot keeps its settings, including any keys, so you can reconnect it here exactly as it was."}
                              </p>
                              <p>A copy of {c.source} is saved first.</p>
                            </>
                          ),
                          run: () =>
                            change(`conn:${c.id}`, () =>
                              api.agentSetConnection(agent.id, c.id, false),
                            ),
                        })
                      }
                    >
                      Disconnect
                    </ActionButton>
                  )
                }
              />
            )}
          />
        ) : (
          <Empty>No MCP servers are set up for {agent.name}.</Empty>
        )}
      </Section>

      {disconnected.length ? (
        <Section
          title="Disconnected by DiskPilot"
          count={disconnected.length}
          description="Reconnect puts the server back exactly as it was."
        >
          <PagedList
            items={disconnected}
            label="servers"
            render={(d) => (
              <Row
                key={d.id}
                title={<span className="truncate font-medium">{d.name}</span>}
                sub={`${d.scope} · disconnected ${formatRelative(d.atMs)}`}
                action={
                  <>
                    <ActionButton
                      busyKey={`forget:${d.id}`}
                      variant="ghost"
                      onClick={() =>
                        onConfirm({
                          title: `Forget ${d.name}?`,
                          destructive: true,
                          action: "Forget",
                          body: (
                            <p>
                              DiskPilot deletes its copy of this server&apos;s settings. To use it
                              again you will need to add it to {agent.name} yourself.
                            </p>
                          ),
                          run: () =>
                            change(`forget:${d.id}`, () => api.agentForgetDisconnected(d.id)),
                        })
                      }
                    >
                      Forget
                    </ActionButton>
                    <ActionButton
                      busyKey={`re:${d.id}`}
                      variant="positive"
                      onClick={() => void change(`re:${d.id}`, () => api.agentReconnect(d.id))}
                    >
                      <Undo2 />
                      Reconnect
                    </ActionButton>
                  </>
                }
              />
            )}
          />
        </Section>
      ) : null}
      <Footnote>
        Servers in each project&apos;s own config, and connectors added in the agent&apos;s app, are
        managed there. Keys and arguments are never shown.
      </Footnote>
    </div>
  );
}

// ---- Plugins -----------------------------------------------------------------

interface PluginSkillStats {
  total: number;
  inactive: number;
  /** Skills actually used in the last 30 days. */
  used: number;
}

function usePluginStats(agent: AgentReport) {
  // "Now" is when the agents were checked, so the numbers match the report.
  const now = useAgents((s) => s.report?.generatedAtMs ?? 0);
  return useMemo(() => {
    const m = new Map<string, PluginSkillStats>();
    for (const s of agent.skills) {
      if (!s.plugin) continue;
      const v = m.get(s.plugin) ?? { total: 0, inactive: 0, used: 0 };
      v.total += 1;
      if (s.state === "inactive") v.inactive += 1;
      if (s.lastUsedMs && now - s.lastUsedMs < MONTH_MS) v.used += 1;
      m.set(s.plugin, v);
    }
    return m;
  }, [agent.skills, now]);
}

function useSetPlugin(agent: AgentReport, onConfirm: (c: Confirmation) => void) {
  const change = useAgents((s) => s.change);
  return (p: AgentPlugin, on: boolean, stats?: PluginSkillStats) => {
    if (on) {
      void change(`plugin:${p.id}`, () => api.agentSetPlugin(agent.id, p.id, true));
      return;
    }
    onConfirm({
      title: `Turn off ${p.name}?`,
      action: "Turn off",
      body: (
        <p>
          {agent.name} stops loading this plugin
          {stats ? ` and its ${stats.total} skill${stats.total === 1 ? "" : "s"}` : ""} from its
          next session. It stays installed, so you can turn it back on here.
        </p>
      ),
      run: () => change(`plugin:${p.id}`, () => api.agentSetPlugin(agent.id, p.id, false)),
    });
  };
}

export function PluginsList({
  agent,
  onConfirm,
}: {
  agent: AgentReport;
  onConfirm: (c: Confirmation) => void;
}) {
  const busy = useAgents((s) => s.busy);
  const extensions = agent.plugins.some((p) => p.kind === "extension");
  const stats = usePluginStats(agent);
  const setPlugin = useSetPlugin(agent, onConfirm);
  // Unused plugins first: they are the ones worth a decision.
  const plugins = useMemo(
    () =>
      [...agent.plugins].sort((a, b) => {
        const ua = stats.get(a.name);
        const ub = stats.get(b.name);
        const unusedA = !!ua && ua.inactive === ua.total && a.enabled !== false;
        const unusedB = !!ub && ub.inactive === ub.total && b.enabled !== false;
        return Number(unusedB) - Number(unusedA) || a.name.localeCompare(b.name);
      }),
    [agent.plugins, stats],
  );

  if (!agent.plugins.length) {
    return <Empty>No plugins or extensions installed for {agent.name}.</Empty>;
  }

  return (
    <div className="space-y-6">
      <Section
        title={extensions ? "Extensions" : "Plugins"}
        count={agent.plugins.length}
        description={
          extensions
            ? `Installed in ${agent.name}. Turn them on or off in ${agent.name} itself; leftover extension folders are under Can free.`
            : "Turning a plugin off stops loading it and its skills, without uninstalling it."
        }
      >
        <PagedList
          items={plugins}
          label={extensions ? "extensions" : "plugins"}
          render={(p) => {
            const st = stats.get(p.name);
            const unused = !!st && st.inactive === st.total && p.enabled !== false;
            return (
              <Row
                key={p.id}
                tone={unused ? "warn" : undefined}
                title={
                  <>
                    <span className="truncate font-medium">{p.name}</span>
                    {p.version ? (
                      <span className="tabular shrink-0 text-xs text-subtle-foreground">
                        {p.version}
                      </span>
                    ) : null}
                    {unused ? <Tint color={REVIEW}>Unused</Tint> : null}
                    {p.enabled === false ? <Tint color={KEEP}>Off</Tint> : null}
                  </>
                }
                sub={[
                  p.publisher,
                  st
                    ? `${st.used} of ${st.total} skill${st.total === 1 ? "" : "s"} used this month`
                    : null,
                ]
                  .filter(Boolean)
                  .join(" · ")}
                meta={
                  <>
                    <span className="tabular block text-foreground">{formatBytes(p.bytes)}</span>
                    <span>{p.lastUsedMs ? `Used ${formatRelative(p.lastUsedMs)}` : ""}</span>
                  </>
                }
                action={
                  p.enabled === null ? null : (
                    <Switch
                      checked={p.enabled}
                      disabled={!!busy}
                      label={`${p.name} plugin`}
                      onChange={(on) => setPlugin(p, on, st)}
                    />
                  )
                }
              />
            );
          }}
        />
      </Section>
    </div>
  );
}

// ---- Skills ------------------------------------------------------------------

type SkillFilter = "inactive" | "user" | "plugin" | "builtIn";

function skillUse(s: AgentSkill) {
  if (s.state === "unknown") return "Use not recorded";
  if (!s.lastUsedMs) return "Never used";
  return `Used ${formatRelative(s.lastUsedMs)}`;
}

function SkillRow({ s }: { s: AgentSkill }) {
  const home = useApp((st) => st.env?.homeDir);
  return (
    <Row
      title={
        <>
          <span className="truncate font-medium">{s.name}</span>
          {s.state === "inactive" ? <Tint color={REVIEW}>Inactive</Tint> : null}
          {s.origin === "plugin" && s.plugin ? (
            <span className="shrink-0 text-xs text-subtle-foreground">from {s.plugin}</span>
          ) : null}
          {s.origin === "builtIn" ? (
            <span className="shrink-0 text-xs text-subtle-foreground">built in</span>
          ) : null}
        </>
      }
      sub={s.description || tildify(s.path, home)}
      meta={
        <>
          <span className="block">{skillUse(s)}</span>
          <span className="tabular">{formatBytes(s.bytes)}</span>
        </>
      }
    />
  );
}

export function SkillsList({
  agent,
  onConfirm,
}: {
  agent: AgentReport;
  onConfirm: (c: Confirmation) => void;
}) {
  const reviewInRecommendations = useApp((s) => s.reviewInRecommendations);
  const reviewable = useReviewable();
  const stats = usePluginStats(agent);
  const setPlugin = useSetPlugin(agent, onConfirm);
  const groups = useMemo(() => {
    // Skills of a plugin that is turned off are not loaded, so they don't count.
    const off = new Set(agent.plugins.filter((p) => p.enabled === false).map((p) => p.name));
    const by = (f: (s: AgentSkill) => boolean) =>
      agent.skills.filter(f).sort((a, b) => a.name.localeCompare(b.name));
    return {
      inactive: by((s) => s.state === "inactive" && !off.has(s.plugin ?? "")),
      user: by((s) => s.origin === "user"),
      plugin: by((s) => s.origin === "plugin"),
      builtIn: by((s) => s.origin === "builtIn"),
    };
  }, [agent.skills, agent.plugins]);
  const filters = (
    [
      { id: "inactive", label: "Inactive" },
      { id: "user", label: "Yours" },
      { id: "plugin", label: "From plugins" },
      { id: "builtIn", label: "Built in" },
    ] as { id: SkillFilter; label: string }[]
  ).filter((f) => groups[f.id].length > 0);
  const [picked, setPicked] = useState<SkillFilter | null>(null);
  const filter = picked && groups[picked].length ? picked : (filters[0]?.id ?? "user");
  const removable = groups.inactive.filter(
    (s) => s.recommendationId && reviewable.has(s.recommendationId),
  );
  // Inactive skills that come with a plugin can only go with the plugin,
  // so they are shown one row per plugin.
  const ownInactive = groups.inactive.filter((s) => s.origin !== "plugin");
  const pluginRows = useMemo(
    () =>
      agent.plugins
        .map((p) => ({ p, st: stats.get(p.name) }))
        .filter(
          (x): x is { p: AgentPlugin; st: PluginSkillStats } =>
            !!x.st && x.st.inactive > 0 && x.p.enabled !== false,
        )
        .sort((a, b) => b.st.inactive / b.st.total - a.st.inactive / a.st.total),
    [agent.plugins, stats],
  );

  if (!agent.skills.length) {
    return <Empty>No skills installed for {agent.name}.</Empty>;
  }

  return (
    <div className="space-y-6">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div
          role="tablist"
          aria-label="Show skills"
          className="flex rounded-lg border border-border-strong p-0.5"
        >
          {filters.map((f) => (
            <button
              key={f.id}
              type="button"
              role="tab"
              aria-selected={filter === f.id}
              onClick={() => setPicked(f.id)}
              className={cn(
                "h-7 rounded-md px-3 text-[13px]",
                filter === f.id
                  ? "bg-panel-raised font-medium text-foreground shadow-sm"
                  : "text-muted-foreground hover:text-foreground",
              )}
            >
              {f.label}
              <span className="tabular ml-1.5 text-xs text-subtle-foreground">
                {formatCount(groups[f.id].length)}
              </span>
            </button>
          ))}
        </div>
        {removable.length ? (
          <Button
            size="sm"
            onClick={() =>
              reviewInRecommendations(removable.map((s) => s.recommendationId as string))
            }
          >
            Review {removable.length} inactive skill{removable.length === 1 ? "" : "s"} ·{" "}
            {formatBytes(removable.reduce((a, s) => a + s.bytes, 0))}
          </Button>
        ) : null}
      </div>

      {filter === "inactive" ? (
        <>
          {ownInactive.length ? (
            <Section
              title="Your skills"
              count={ownInactive.length}
              description="Not used in 30 days. Review them to move their folders to the Trash."
            >
              <PagedList
                items={ownInactive}
                label="skills"
                render={(s) => <SkillRow key={s.id} s={s} />}
              />
            </Section>
          ) : null}
          {pluginRows.length ? (
            <Section
              title="Skills from plugins"
              count={pluginRows.reduce((a, x) => a + x.st.inactive, 0)}
              description="These come with a plugin and go with it. Turn off a plugin you don't use."
            >
              <PagedList
                items={pluginRows}
                label="plugins"
                render={({ p, st }) => (
                  <Row
                    key={p.id}
                    title={
                      <>
                        <span className="truncate font-medium">{p.name}</span>
                        {p.enabled === false ? <Tint color={KEEP}>Off</Tint> : null}
                      </>
                    }
                    sub={
                      st.inactive === st.total
                        ? `None of its ${st.total} skill${st.total === 1 ? "" : "s"} used in 30 days`
                        : `${st.inactive} of ${st.total} skills not used in 30 days`
                    }
                    action={
                      p.enabled === true && st.inactive === st.total ? (
                        <ActionButton
                          busyKey={`plugin:${p.id}`}
                          onClick={() => setPlugin(p, false, st)}
                        >
                          Turn off plugin
                        </ActionButton>
                      ) : null
                    }
                  />
                )}
              />
            </Section>
          ) : null}
        </>
      ) : (
        <PagedList
          key={filter}
          items={groups[filter]}
          label="skills"
          render={(s) => <SkillRow key={s.id} s={s} />}
        />
      )}
      <Footnote>
        {agent.usageNote} A skill counts as inactive after 30 days without use; skills installed in
        the last 14 days never do. Reviewed skills are moved to the Trash, not deleted, so you can
        put them back.
      </Footnote>
    </div>
  );
}

// ---- Clean up ----------------------------------------------------------------

export function CleanupList({ agent }: { agent: AgentReport }) {
  const home = useApp((s) => s.env?.homeDir);
  const reviewInRecommendations = useApp((s) => s.reviewInRecommendations);
  const reviewable = useReviewable();
  // Everything the agent stores, from the Developer Tools analysis.
  const stored = useApp((s) => s.devtools?.tools.find((t) => t.id === agent.id && t.detected));
  const items = agent.cleanup.filter(
    (i) => i.recommendationId && reviewable.has(i.recommendationId),
  );
  const total = items.reduce((a, i) => a + i.bytes, 0);
  return (
    <div className="space-y-6">
      {!items.length ? (
        <Empty>Nothing of {agent.name}&apos;s needs cleaning up.</Empty>
      ) : (
        <Section
          title={`${formatBytes(total)} can be freed`}
          description="Caches, logs, old versions and inactive skills. Each one goes through the usual review and is moved to the Trash, so you can undo it from History."
          action={
            <Button
              size="sm"
              onClick={() =>
                reviewInRecommendations(items.map((i) => i.recommendationId as string))
              }
            >
              Review all {items.length}
            </Button>
          }
        >
          <PagedList
            items={items}
            label="items"
            render={(i, idx) => (
              <Row
                key={`${i.path ?? i.label}-${idx}`}
                title={<span className="truncate">{i.label}</span>}
                sub={i.path ? tildify(i.path, home) : i.detail}
                meta={
                  <>
                    <span className="tabular block font-medium text-foreground">
                      {formatBytes(i.bytes)}
                    </span>
                    <span>{i.lastUsedMs ? formatRelative(i.lastUsedMs) : ""}</span>
                  </>
                }
                action={
                  <Button
                    size="sm"
                    variant="secondary"
                    onClick={() => reviewInRecommendations([i.recommendationId as string])}
                  >
                    Review
                  </Button>
                }
              />
            )}
          />
        </Section>
      )}
      {stored ? (
        <Section
          title={`Everything ${agent.name} stores`}
          description="Versions, conversation data, caches and settings, with what each one is. History and settings are listed for reference and never suggested for removal."
        >
          <ToolCard tool={stored} reviewable={reviewable} onReview={reviewInRecommendations} />
        </Section>
      ) : null}
    </div>
  );
}
