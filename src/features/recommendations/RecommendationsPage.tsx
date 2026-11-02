import { ListChecks, ShieldCheck, Sparkles } from "lucide-react";
import { useEffect, useMemo, useState } from "react";

import { PageHeader } from "@/components/layout/PageHeader";
import { EmptyState } from "@/components/shared/EmptyState";
import { Button } from "@/components/ui/button";
import { Panel } from "@/components/ui/panel";
import { summarizeSelection } from "@/lib/analysis";
import { formatBytes, formatCount } from "@/lib/format";
import { useApp } from "@/lib/store";
import { TIER_META, TIER_ORDER } from "@/lib/tiers";
import type { Tier } from "@/lib/types";
import { cn } from "@/lib/utils";

import { CleanupDialog } from "./CleanupDialog";
import { RecommendationRow } from "./RecommendationRow";

type Filter = Tier | "all";

export function RecommendationsPage() {
  const { summary, recommendations, scanning, startScan, refreshRecommendations, refreshHistory } =
    useApp();
  const [filter, setFilter] = useState<Filter>("all");
  const consumePreselect = useApp((s) => s.consumePreselect);
  // Items handed over from another page (e.g. Developer Tools) start selected.
  const [selected, setSelected] = useState<Set<string>>(
    () => new Set(useApp.getState().preselect ?? []),
  );
  useEffect(() => {
    consumePreselect();
  }, [consumePreselect]);
  const [expanded, setExpanded] = useState<string | null>(null);
  const [dialog, setDialog] = useState<{ key: number; ids: string[] } | null>(null);

  const byTier = useMemo(() => {
    const groups: Record<Tier, typeof recommendations> = {
      likelySafe: [],
      reviewFirst: [],
      keep: [],
    };
    for (const r of recommendations) groups[r.tier].push(r);
    return groups;
  }, [recommendations]);

  // Drop selections that no longer exist (e.g. after a cleanup).
  const liveSelection = useMemo(() => {
    const ids = new Set(recommendations.map((r) => r.id));
    return new Set([...selected].filter((id) => ids.has(id)));
  }, [selected, recommendations]);
  const selection = summarizeSelection(recommendations, liveSelection);
  const max = Math.max(0, ...recommendations.map((r) => r.bytes));

  const toggle = (id: string) =>
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });

  const selectAllSafe = () =>
    setSelected(new Set(byTier.likelySafe.filter((r) => r.methods.length).map((r) => r.id)));

  if (!summary) {
    return (
      <>
        <PageHeader title="Cleanup" />
        <EmptyState
          className="flex-1 justify-center"
          icon={Sparkles}
          title={scanning ? "Scanning…" : "No scan yet"}
          description="Recommendations are built from a scan. Each one explains what was found, why it may be safe, and what could go wrong."
          action={scanning ? null : <Button onClick={() => void startScan()}>Start scan</Button>}
        />
      </>
    );
  }

  const tiers = filter === "all" ? TIER_ORDER : [filter];

  return (
    <>
      <PageHeader
        title="Cleanup"
        description="Explained suggestions. Nothing is removed until you review and confirm it."
      />
      <div className="relative min-h-0 flex-1 overflow-y-auto">
        <div className="mx-auto max-w-[1120px] space-y-6 px-6 pt-6 pb-24">
          <div
            role="tablist"
            aria-label="Filter by tier"
            className="inline-flex rounded-lg bg-panel-raised p-0.5"
          >
            {(["all", ...TIER_ORDER] as Filter[]).map((t) => {
              const items = t === "all" ? recommendations : byTier[t];
              const bytes = items.reduce((a, r) => a + r.bytes, 0);
              const active = filter === t;
              return (
                <button
                  key={t}
                  type="button"
                  role="tab"
                  aria-selected={active}
                  onClick={() => setFilter(t)}
                  className={cn(
                    "flex items-baseline gap-2 rounded-md px-3 py-1.5 text-[13px] transition-colors duration-[120ms]",
                    active
                      ? "bg-panel text-foreground shadow-[0_0_0_0.5px_var(--border-strong),0_1px_2px_rgb(0_0_0/0.08)]"
                      : "text-muted-foreground hover:text-foreground",
                  )}
                >
                  {t !== "all" ? (
                    <span
                      className="size-2 self-center rounded-full"
                      style={{ background: TIER_META[t].color }}
                      aria-hidden
                    />
                  ) : null}
                  <span className="font-medium">{t === "all" ? "All" : TIER_META[t].label}</span>
                  <span className="tabular text-xs text-subtle-foreground">
                    {t === "all" ? formatCount(items.length) : formatBytes(bytes)}
                  </span>
                </button>
              );
            })}
          </div>

          <div className="flex items-center justify-between gap-3">
            <p className="flex items-center gap-1.5 text-xs text-muted-foreground">
              <ShieldCheck className="size-3.5 text-good" aria-hidden />
              Trash first. Every path is re-checked by the safety layer before anything happens.
            </p>
            <div className="flex items-center gap-2">
              {filter !== "all" ? (
                <Button variant="ghost" size="sm" onClick={() => setFilter("all")}>
                  Show all
                </Button>
              ) : null}
              <Button
                variant="secondary"
                size="sm"
                onClick={selectAllSafe}
                disabled={!byTier.likelySafe.length}
              >
                <ListChecks />
                Select all likely safe
              </Button>
            </div>
          </div>

          {recommendations.length === 0 ? (
            <Panel>
              <EmptyState
                icon={Sparkles}
                title="Nothing to recommend"
                description="The last scan found no caches, build output or other known items above the size thresholds."
              />
            </Panel>
          ) : (
            tiers.map((tier) =>
              byTier[tier].length ? (
                <section key={tier} aria-label={TIER_META[tier].label}>
                  <header className="flex items-center gap-2 border-b border-border pb-2">
                    {(() => {
                      const Icon = TIER_META[tier].icon;
                      return (
                        <Icon
                          className="size-3.5"
                          style={{ color: TIER_META[tier].color }}
                          aria-hidden
                        />
                      );
                    })()}
                    <h2 className="text-[13px] font-semibold">{TIER_META[tier].label}</h2>
                    <span className="text-xs text-subtle-foreground">
                      {TIER_META[tier].description}
                    </span>
                  </header>
                  <ul>
                    {byTier[tier].map((r) => (
                      <RecommendationRow
                        key={r.id}
                        rec={r}
                        max={max}
                        selected={liveSelection.has(r.id)}
                        expanded={expanded === r.id}
                        onToggleSelect={() => toggle(r.id)}
                        onToggleExpand={() => setExpanded((e) => (e === r.id ? null : r.id))}
                      />
                    ))}
                  </ul>
                </section>
              ) : null,
            )
          )}
        </div>

        {selection.count > 0 ? (
          <div className="pointer-events-none sticky bottom-0 flex justify-center px-6 pb-4">
            <div className="animate-in pointer-events-auto flex items-center gap-4 rounded-lg bg-panel-raised px-4 py-2.5 shadow-float">
              <p className="text-[13px]">
                <span className="tabular font-medium">{selection.count}</span> selected,{" "}
                <span className="tabular font-medium">{formatBytes(selection.bytes)}</span>
              </p>
              <Button variant="ghost" size="sm" onClick={() => setSelected(new Set())}>
                Clear
              </Button>
              <Button onClick={() => setDialog({ key: Date.now(), ids: [...liveSelection] })}>
                Review cleanup…
              </Button>
            </div>
          </div>
        ) : null}
      </div>

      {dialog ? (
        <CleanupDialog
          key={dialog.key}
          open
          ids={dialog.ids}
          allowedMethods={selection.methods.length ? selection.methods : ["trash"]}
          onOpenChange={(open) => {
            if (!open) setDialog(null);
          }}
          onFinished={() => {
            setSelected(new Set());
            void refreshRecommendations();
            void refreshHistory();
          }}
        />
      ) : null}
    </>
  );
}
