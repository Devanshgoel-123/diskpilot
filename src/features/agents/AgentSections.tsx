import { AlertTriangle, ChevronDown, Loader2, Plug, Undo2 } from "lucide-react";
import { type ReactNode, useMemo, useState } from "react";

import { Button } from "@/components/ui/button";
import { api } from "@/lib/api";
import { formatBytes, formatRelative, tildify } from "@/lib/format";
import { useApp } from "@/lib/store";
import type {
  AgentPermission,
  AgentPlugin,
  AgentReport,
  AgentSkill,
  DisconnectedServer,
  PermissionKind,
} from "@/lib/types";
import { cn } from "@/lib/utils";

import { useAgents, useReviewable } from "./agentsStore";
import { type Confirmation, Tint } from "./shared";

const REVIEW = "var(--tier-review)";
const SAFE = "var(--tier-safe)";
const KEEP = "var(--tier-keep)";
const COLLAPSED_ROWS = 6;

function Group({
  title,
  hint,
  count,
  children,
}: {
  title: string;
  hint?: string;
  count?: number;
  children: ReactNode;
}) {
  return (
    <section className="space-y-1.5">
      <div className="flex items-baseline gap-2">
        <h3 className="text-xs font-medium">{title}</h3>
        {count !== undefined ? (
          <span className="tabular text-2xs text-subtle-foreground">{count}</span>
        ) : null}
        {hint ? <p className="truncate text-2xs text-subtle-foreground">{hint}</p> : null}
      </div>
      {children}
    </section>
  );
}

/** A bordered list that shows the first few rows until expanded. */
function RowList<T>({
  items,
  render,
  limit = COLLAPSED_ROWS,
}: {
  items: T[];
  render: (item: T, index: number) => ReactNode;
  limit?: number;
}) {
  const [all, setAll] = useState(false);
  const shown = all ? items : items.slice(0, limit);
  return (
    <div className="overflow-hidden rounded-md border border-border">
      <ul className="divide-y divide-border">{shown.map(render)}</ul>
      {items.length > limit ? (
        <button
          type="button"
          onClick={() => setAll((a) => !a)}
          aria-expanded={all}
          className="flex w-full items-center justify-center gap-1 border-t border-border py-1.5 text-2xs text-muted-foreground hover:bg-hover hover:text-foreground"
        >
          {all ? "Show fewer" : `Show all ${items.length}`}
          <ChevronDown className={cn("size-3 transition-transform", all && "rotate-180")} />
        </button>
      ) : null}
    </div>
  );
}

function Row({
  title,
  sub,
  meta,
  action,
  attention,
}: {
  title: ReactNode;
  sub?: ReactNode;
  meta?: ReactNode;
  action?: ReactNode;
  attention?: boolean;
}) {
  return (
    <li
      className={cn(
        "flex min-h-10 items-center gap-3 px-3 py-1.5",
        attention && "bg-[color-mix(in_oklab,var(--tier-review)_6%,transparent)]",
      )}
    >
      <div className="min-w-0 flex-1">
        <div className="flex min-w-0 items-center gap-2 text-xs">{title}</div>
        {sub ? <div className="truncate text-2xs text-subtle-foreground">{sub}</div> : null}
      </div>
      {meta ? (
        <div className="hidden shrink-0 text-right text-2xs text-subtle-foreground sm:block">
          {meta}
        </div>
      ) : null}
      {action ? <div className="flex shrink-0 items-center gap-1">{action}</div> : null}
    </li>
  );
}

function Empty({ children }: { children: ReactNode }) {
  return (
    <p className="rounded-md border border-dashed border-border px-3 py-4 text-center text-xs text-subtle-foreground">
      {children}
    </p>
  );
}

function Footnote({ children }: { children: ReactNode }) {
  return <p className="pt-1 text-2xs leading-relaxed text-subtle-foreground">{children}</p>;
}

function BusyButton({
  busyKey,
  children,
  ...props
}: React.ComponentProps<typeof Button> & { busyKey: string }) {
  const busy = useAgents((s) => s.busy);
  return (
    <Button size="sm" variant="ghost" disabled={!!busy} {...props}>
      {busy === busyKey ? <Loader2 className="animate-spin" /> : null}
      {children}
    </Button>
  );
}

function Mono({ children, title }: { children: ReactNode; title?: string }) {
  return (
    <span className="min-w-0 truncate font-mono text-[11.5px]" title={title}>
      {children}
    </span>
  );
}

// ---- Permissions ---------------------------------------------------------------

const PERMISSION_GROUPS: { kind: PermissionKind; title: string; hint: string }[] = [
  { kind: "mode", title: "How it works", hint: "Approval and sandbox settings" },
  { kind: "allow", title: "Allowed without asking", hint: "Runs these without a prompt" },
  { kind: "ask", title: "Always asks first", hint: "" },
  { kind: "deny", title: "Blocked", hint: "Never allowed" },
  { kind: "trust", title: "Trusted folders", hint: "" },
  { kind: "hook", title: "Runs by itself", hint: "Hooks run commands on their own" },
];

export function PermissionsList({
  agent,
  onConfirm,
}: {
  agent: AgentReport;
  onConfirm: (c: Confirmation) => void;
}) {
  const change = useAgents((s) => s.change);
  const risky = agent.permissions.filter((p) => p.risk).length;

  if (!agent.permissions.length) {
    return (
      <Empty>
        No permission settings found. {agent.name} uses its defaults and asks before acting.
      </Empty>
    );
  }

  const remove = (p: AgentPermission) =>
    onConfirm({
      title: "Remove this rule?",
      destructive: true,
      action: "Remove rule",
      body: (
        <>
          <code className="block rounded bg-panel-raised px-2 py-1 font-mono text-[11.5px] break-all text-foreground">
            {p.value}
          </code>
          <p>
            {p.kind === "allow"
              ? `${agent.name} will ask again before doing this.`
              : `${agent.name} will no longer apply this rule.`}{" "}
            It is removed from {p.source}; a copy of the file is kept in DiskPilot&apos;s data
            folder.
          </p>
        </>
      ),
      run: () => change(`perm:${p.id}`, () => api.agentRemovePermission(agent.id, p.id as string)),
    });

  return (
    <div className="space-y-4">
      {risky ? (
        <p className="flex items-center gap-1.5 text-xs text-[var(--tier-review)]">
          <AlertTriangle className="size-3.5" />
          {risky === 1 ? "1 setting lets" : `${risky} settings let`} {agent.name} act without
          asking. They are listed first.
        </p>
      ) : null}
      {PERMISSION_GROUPS.map((g) => {
        const items = agent.permissions
          .filter((p) => p.kind === g.kind)
          .sort((a, b) => Number(!!b.risk) - Number(!!a.risk));
        if (!items.length) return null;
        return (
          <Group key={g.kind} title={g.title} hint={g.hint} count={items.length}>
            <RowList
              items={items}
              limit={g.kind === "allow" ? 8 : COLLAPSED_ROWS}
              render={(p, i) => (
                <Row
                  key={`${p.source}-${p.label}-${p.value}-${i}`}
                  attention={!!p.risk}
                  title={
                    g.kind === "mode" || g.kind === "hook" || g.kind === "trust" ? (
                      <>
                        <span className="shrink-0 text-muted-foreground">{p.label}</span>
                        <Mono title={p.value}>{p.value}</Mono>
                      </>
                    ) : (
                      <Mono title={p.value}>{p.value}</Mono>
                    )
                  }
                  sub={p.risk ?? p.source}
                  action={
                    p.id ? (
                      <BusyButton
                        busyKey={`perm:${p.id}`}
                        onClick={() => remove(p)}
                        aria-label={`Remove rule ${p.value}`}
                      >
                        Remove
                      </BusyButton>
                    ) : null
                  }
                />
              )}
            />
          </Group>
        );
      })}
      <Footnote>
        Rules in each project&apos;s own settings are not shown. Passwords and tokens inside rules
        are hidden.
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
    <div className="space-y-4">
      {agent.connections.length ? (
        <Group title="Connected" count={agent.connections.length}>
          <RowList
            items={agent.connections}
            render={(c) => (
              <Row
                key={c.id}
                title={
                  <>
                    <Plug className="size-3 shrink-0 text-muted-foreground" aria-hidden />
                    <span className="truncate font-medium">{c.name}</span>
                    {c.enabled ? (
                      <Tint color={SAFE}>On</Tint>
                    ) : (
                      <Tint color={KEEP}>{canToggle ? "Off" : "Off in this project"}</Tint>
                    )}
                  </>
                }
                sub={[c.transport, c.target, c.scope].filter(Boolean).join(" · ")}
                action={
                  canToggle && !c.enabled ? (
                    <BusyButton
                      busyKey={`conn:${c.id}`}
                      onClick={() =>
                        void change(`conn:${c.id}`, () =>
                          api.agentSetConnection(agent.id, c.id, true),
                        )
                      }
                    >
                      Reconnect
                    </BusyButton>
                  ) : (
                    <BusyButton
                      busyKey={`conn:${c.id}`}
                      aria-label={`Disconnect ${c.name}`}
                      onClick={() =>
                        onConfirm({
                          title: `Disconnect ${c.name}?`,
                          action: "Disconnect",
                          body: (
                            <>
                              <p>
                                {agent.name} stops using this server from its next session.
                                {canToggle
                                  ? " It stays in config.toml, turned off, so you can turn it back on here."
                                  : " DiskPilot keeps its settings, including any keys, so you can reconnect it here exactly as it was."}
                              </p>
                              <p>
                                Changes {c.source}. A copy of the file is kept in DiskPilot&apos;s
                                data folder.
                              </p>
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
                    </BusyButton>
                  )
                }
              />
            )}
          />
        </Group>
      ) : (
        <Empty>No MCP servers are set up for {agent.name}.</Empty>
      )}

      {disconnected.length ? (
        <Group title="Disconnected by DiskPilot" count={disconnected.length}>
          <RowList
            items={disconnected}
            render={(d) => (
              <Row
                key={d.id}
                title={<span className="truncate font-medium">{d.name}</span>}
                sub={`${d.scope} · disconnected ${formatRelative(d.atMs)}`}
                action={
                  <>
                    <BusyButton
                      busyKey={`re:${d.id}`}
                      onClick={() => void change(`re:${d.id}`, () => api.agentReconnect(d.id))}
                    >
                      <Undo2 />
                      Reconnect
                    </BusyButton>
                    <BusyButton
                      busyKey={`forget:${d.id}`}
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
                    </BusyButton>
                  </>
                }
              />
            )}
          />
        </Group>
      ) : null}
      <Footnote>
        Servers in each project&apos;s own config and connectors added in the agent&apos;s app are
        managed there. Keys and arguments are never shown.
      </Footnote>
    </div>
  );
}

// ---- Plugins -----------------------------------------------------------------

const MONTH_MS = 30 * 86_400_000;

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

function useTurnOff(agent: AgentReport, onConfirm: (c: Confirmation) => void) {
  const change = useAgents((s) => s.change);
  return (p: AgentPlugin, stats: PluginSkillStats | undefined) =>
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
}

export function PluginsList({
  agent,
  onConfirm,
}: {
  agent: AgentReport;
  onConfirm: (c: Confirmation) => void;
}) {
  const change = useAgents((s) => s.change);
  const extensions = agent.plugins.some((p) => p.kind === "extension");
  const stats = usePluginStats(agent);
  const turnOff = useTurnOff(agent, onConfirm);

  if (!agent.plugins.length) {
    return <Empty>No plugins or extensions installed for {agent.name}.</Empty>;
  }

  return (
    <div className="space-y-2">
      <RowList
        items={agent.plugins}
        limit={10}
        render={(p) => {
          const st = stats.get(p.name);
          const unused = !!st && st.inactive === st.total;
          return (
            <Row
              key={p.id}
              attention={unused && p.enabled === true}
              title={
                <>
                  <span className="truncate font-medium">{p.name}</span>
                  {p.version ? (
                    <span className="shrink-0 font-mono text-2xs text-subtle-foreground">
                      {p.version}
                    </span>
                  ) : null}
                  {p.enabled === false ? <Tint color={KEEP}>Off</Tint> : null}
                  {unused && p.enabled ? <Tint color={REVIEW}>Unused</Tint> : null}
                </>
              }
              sub={[
                p.publisher,
                st
                  ? `${st.used} of ${st.total} skill${st.total === 1 ? "" : "s"} used in the last 30 days`
                  : null,
              ]
                .filter(Boolean)
                .join(" · ")}
              meta={
                <>
                  <span className="tabular block">{formatBytes(p.bytes)}</span>
                  {p.lastUsedMs ? <span>used {formatRelative(p.lastUsedMs)}</span> : null}
                </>
              }
              action={
                p.enabled === null ? null : p.enabled ? (
                  <BusyButton busyKey={`plugin:${p.id}`} onClick={() => turnOff(p, st)}>
                    Turn off
                  </BusyButton>
                ) : (
                  <BusyButton
                    busyKey={`plugin:${p.id}`}
                    onClick={() =>
                      void change(`plugin:${p.id}`, () => api.agentSetPlugin(agent.id, p.id, true))
                    }
                  >
                    Turn on
                  </BusyButton>
                )
              }
            />
          );
        }}
      />
      <Footnote>
        {extensions
          ? `Turn extensions on or off in ${agent.name}. Leftover extension folders are listed under Clean up.`
          : "Old versions of plugins are listed under Clean up."}
      </Footnote>
    </div>
  );
}

// ---- Skills ------------------------------------------------------------------

type SkillFilter = "inactive" | "user" | "plugin" | "builtIn";

function skillMeta(s: AgentSkill) {
  if (s.state === "unknown") return "No record of use";
  if (!s.lastUsedMs) return "Not used";
  return `Used ${formatRelative(s.lastUsedMs)}`;
}

function SkillRow({ s, showState }: { s: AgentSkill; showState: boolean }) {
  const home = useApp((st) => st.env?.homeDir);
  return (
    <Row
      key={s.id}
      title={
        <>
          <span className="truncate font-medium">{s.name}</span>
          {showState && s.state === "inactive" ? <Tint color={REVIEW}>Inactive</Tint> : null}
          {s.origin === "plugin" && s.plugin ? (
            <span className="shrink-0 text-2xs text-subtle-foreground">{s.plugin}</span>
          ) : null}
        </>
      }
      sub={s.description || tildify(s.path, home)}
      meta={
        <>
          <span className="block">{skillMeta(s)}</span>
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
  const turnOff = useTurnOff(agent, onConfirm);
  const groups = useMemo(() => {
    const by = (f: (s: AgentSkill) => boolean) =>
      agent.skills.filter(f).sort((a, b) => a.name.localeCompare(b.name));
    return {
      inactive: by((s) => s.state === "inactive"),
      user: by((s) => s.origin === "user"),
      plugin: by((s) => s.origin === "plugin"),
      builtIn: by((s) => s.origin === "builtIn"),
    };
  }, [agent.skills]);
  const filters: { id: SkillFilter; label: string }[] = (
    [
      { id: "inactive", label: "Inactive" },
      { id: "user", label: "Yours" },
      { id: "plugin", label: "From plugins" },
      { id: "builtIn", label: "Built in" },
    ] as const
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
        .filter((x): x is { p: AgentPlugin; st: PluginSkillStats } => !!x.st && x.st.inactive > 0)
        .sort((a, b) => b.st.inactive / b.st.total - a.st.inactive / a.st.total),
    [agent.plugins, stats],
  );

  if (!agent.skills.length) {
    return <Empty>No skills installed for {agent.name}.</Empty>;
  }

  return (
    <div className="space-y-3">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div
          role="tablist"
          aria-label="Show skills"
          className="flex rounded-md border border-border-strong p-0.5"
        >
          {filters.map((f) => (
            <button
              key={f.id}
              type="button"
              role="tab"
              aria-selected={filter === f.id}
              onClick={() => setPicked(f.id)}
              className={cn(
                "h-6 rounded px-2.5 text-xs",
                filter === f.id
                  ? "bg-panel-raised font-medium text-foreground shadow-sm"
                  : "text-muted-foreground hover:text-foreground",
              )}
            >
              {f.label}
              <span className="tabular ml-1.5 text-2xs text-subtle-foreground">
                {groups[f.id].length}
              </span>
            </button>
          ))}
        </div>
        {removable.length ? (
          <Button
            size="sm"
            variant="secondary"
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
            <Group title="Your skills" count={ownInactive.length} hint="Can be moved to the Trash">
              <RowList
                items={ownInactive}
                limit={10}
                render={(s) => <SkillRow key={s.id} s={s} showState={false} />}
              />
            </Group>
          ) : null}
          {pluginRows.length ? (
            <Group
              title="From plugins"
              count={pluginRows.reduce((a, x) => a + x.st.inactive, 0)}
              hint="Removed with their plugin"
            >
              <RowList
                items={pluginRows}
                limit={8}
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
                        <BusyButton busyKey={`plugin:${p.id}`} onClick={() => turnOff(p, st)}>
                          Turn off plugin
                        </BusyButton>
                      ) : null
                    }
                  />
                )}
              />
            </Group>
          ) : null}
        </>
      ) : (
        <RowList
          key={filter}
          items={groups[filter]}
          limit={10}
          render={(s) => <SkillRow key={s.id} s={s} showState />}
        />
      )}
      <Footnote>
        {agent.usageNote} A skill counts as inactive after 30 days without use; skills installed in
        the last 14 days never do. Removing one moves its folder to the Trash after you review it.
      </Footnote>
    </div>
  );
}

// ---- Clean up ----------------------------------------------------------------

export function CleanupList({ agent }: { agent: AgentReport }) {
  const home = useApp((s) => s.env?.homeDir);
  const reviewInRecommendations = useApp((s) => s.reviewInRecommendations);
  const reviewable = useReviewable();
  const items = agent.cleanup.filter(
    (i) => i.recommendationId && reviewable.has(i.recommendationId),
  );
  if (!items.length) {
    return <Empty>Nothing of {agent.name}&apos;s needs cleaning up.</Empty>;
  }
  const total = items.reduce((a, i) => a + i.bytes, 0);
  return (
    <div className="space-y-3">
      <div className="flex items-center justify-between gap-2">
        <p className="text-xs text-muted-foreground">
          Caches, logs, old versions and inactive skills.{" "}
          <span className="tabular font-medium text-foreground">{formatBytes(total)}</span> in{" "}
          {items.length} item{items.length === 1 ? "" : "s"}.
        </p>
        <Button
          size="sm"
          onClick={() => reviewInRecommendations(items.map((i) => i.recommendationId as string))}
        >
          Review all
        </Button>
      </div>
      <RowList
        items={items}
        limit={10}
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
                {i.lastUsedMs ? <span>{formatRelative(i.lastUsedMs)}</span> : null}
              </>
            }
            action={
              <Button
                size="sm"
                variant="ghost"
                onClick={() => reviewInRecommendations([i.recommendationId as string])}
              >
                Review
              </Button>
            }
          />
        )}
      />
      <Footnote>
        Everything goes through the usual review, and is moved to the Trash so you can undo it from
        History.
      </Footnote>
    </div>
  );
}
