import { Blocks, ChevronRight, Loader2, RefreshCw, Wrench } from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import { useShallow } from "zustand/react/shallow";

import { PageHeader } from "@/components/layout/PageHeader";

import { EmptyState } from "@/components/shared/EmptyState";
import { Button } from "@/components/ui/button";
import { Panel } from "@/components/ui/panel";
import { formatBytes, formatCount, formatRelative } from "@/lib/format";
import { words } from "@/lib/platform";
import { useApp } from "@/lib/store";
import { WindowsSections } from "@/features/windows/sections";

import { AGENT_TOOL_IDS } from "./agentTools";
import { ToolCard } from "./ToolCard";

export function DeveloperToolsPage() {
  const { devtools, analyzingDevTools, analyzeDevTools, recommendations, reviewInRecommendations } =
    useApp(
      useShallow((s) => ({
        devtools: s.devtools,
        analyzingDevTools: s.analyzingDevTools,
        analyzeDevTools: s.analyzeDevTools,
        recommendations: s.recommendations,
        reviewInRecommendations: s.reviewInRecommendations,
      })),
    );

  useEffect(() => {
    if (!devtools) void analyzeDevTools();
  }, [devtools, analyzeDevTools]);

  const reviewable = useMemo(
    () => new Set(recommendations.filter((r) => r.methods.length).map((r) => r.id)),
    [recommendations],
  );
  const tools = useMemo(
    () => devtools?.tools.filter((t) => !AGENT_TOOL_IDS.has(t.id)) ?? [],
    [devtools],
  );
  const detected = useMemo(() => tools.filter((t) => t.detected), [tools]);
  const missing = useMemo(() => tools.filter((t) => !t.detected), [tools]);
  const navigate = useApp((s) => s.navigate);
  const reviewIds = useMemo(
    () =>
      detected
        .flatMap((t) => t.sections.flatMap((s) => s.items))
        .map((i) => i.recommendationId)
        .filter((id): id is string => !!id && reviewable.has(id)),
    [detected, reviewable],
  );
  // Same items as the "Review" buttons, so the totals always agree.
  const bytesOf = useCallback(
    (ids: Iterable<string>) => {
      const set = new Set(ids);
      return recommendations
        .filter((r) => set.has(r.id))
        .reduce((a, r) => a + r.estimatedReclaimBytes, 0);
    },
    [recommendations],
  );
  const reviewBytes = useMemo(() => bytesOf(reviewIds), [bytesOf, reviewIds]);

  // Pick items across tools, untick the ones to keep, then review the rest
  // together. Ids that stop being reviewable (a new analysis) drop out.
  const [picked, setPicked] = useState<Set<string>>(new Set());
  const selected = useMemo(
    () => new Set([...picked].filter((id) => reviewable.has(id))),
    [picked, reviewable],
  );
  const select = useCallback(
    (ids: string[], on: boolean) =>
      setPicked((prev) => {
        const next = new Set(prev);
        for (const id of ids) {
          if (on) next.add(id);
          else next.delete(id);
        }
        return next;
      }),
    [],
  );
  const allPicked = reviewIds.length > 0 && reviewIds.every((id) => selected.has(id));
  const reviewSelected = () => {
    reviewInRecommendations([...selected]);
    setPicked(new Set());
  };

  return (
    <>
      <PageHeader
        title="Developer Tools"
        description="Per-tool analysis of editors, runtimes, toolchains and build data, including caches, logs and leftover updates."
        actions={
          <Button
            variant="secondary"
            onClick={() => void analyzeDevTools()}
            disabled={analyzingDevTools}
          >
            {analyzingDevTools ? <Loader2 className="animate-spin" /> : <RefreshCw />}
            Analyze again
          </Button>
        }
      />
      <div className="min-h-0 flex-1 overflow-y-auto">
        <div className="page-content space-y-4">
          {!devtools ? (
            <Panel>
              <EmptyState
                icon={analyzingDevTools ? Loader2 : Wrench}
                title={analyzingDevTools ? "Analyzing developer tools…" : "No analysis yet"}
                description="Reads each tool's folders to find storage for deleted projects, superseded versions and inactive dependencies. Nothing is changed."
              />
            </Panel>
          ) : (
            <>
              <div className="flex flex-wrap items-end justify-between gap-4">
                <div className="flex flex-wrap gap-x-10 gap-y-3">
                  <div>
                    <p className="text-xs text-muted-foreground">Tools found</p>
                    <p className="tabular mt-0.5 text-[20px] leading-7 font-semibold tracking-tight">
                      {detected.length}
                    </p>
                  </div>
                  <div>
                    <p className="text-xs text-muted-foreground">Storage used</p>
                    <p className="tabular mt-0.5 text-[20px] leading-7 font-semibold tracking-tight">
                      {formatBytes(detected.reduce((a, t) => a + t.totalBytes, 0))}
                    </p>
                  </div>
                  <div>
                    <p className="text-xs text-muted-foreground">Worth reviewing</p>
                    <p className="tabular mt-0.5 text-[20px] leading-7 font-semibold tracking-tight">
                      {formatBytes(reviewBytes)}
                    </p>
                  </div>
                </div>
                <div className="flex items-center gap-3">
                  <p className="text-2xs text-subtle-foreground">
                    Analyzed {formatRelative(devtools.generatedAtMs)}
                  </p>
                  {reviewIds.length ? (
                    <>
                      <Button variant="ghost" onClick={() => select(reviewIds, !allPicked)}>
                        {allPicked ? "Clear selection" : "Select all"}
                      </Button>
                      <Button onClick={() => reviewInRecommendations(reviewIds)}>
                        Review {reviewIds.length} items
                      </Button>
                    </>
                  ) : null}
                </div>
              </div>

              <div className="grid grid-cols-1 gap-4 md:grid-cols-2">
                {detected.map((t) => (
                  <ToolCard
                    key={t.id}
                    tool={t}
                    reviewable={reviewable}
                    onReview={reviewInRecommendations}
                    selected={selected}
                    onSelect={select}
                  />
                ))}
              </div>

              <button
                type="button"
                onClick={() => navigate("agents")}
                className="flex w-full items-center gap-3 rounded-lg border border-border bg-panel px-4 py-3 text-left hover:bg-hover"
              >
                <Blocks className="size-4 shrink-0 text-accent" aria-hidden />
                <span className="min-w-0 flex-1">
                  <span className="block text-[13px] font-medium">
                    Claude Code and Codex are on the AI Agents page
                  </span>
                  <span className="block text-xs text-muted-foreground">
                    Their caches, old versions, conversation data and the check for keys in
                    transcripts moved there, next to their permissions and skills.
                  </span>
                </span>
                <ChevronRight className="size-4 shrink-0 text-muted-foreground" aria-hidden />
              </button>

              <WindowsSections page="developer" />

              {missing.length ? (
                <p className="text-xs text-subtle-foreground">
                  Not found on this {words().computer}: {missing.map((t) => t.name).join(", ")}.
                </p>
              ) : null}
              <p className="text-2xs leading-relaxed text-subtle-foreground">
                Sizes come from the last scan where available and are measured directly otherwise.
                Project activity is based on when dependency manifests last changed. Runtimes,
                toolchains and simulators are never removed by DiskPilot; use the commands shown.
                Conversation history, sign-in and settings are listed for reference only, and other
                apps&apos; databases are read, never modified.
              </p>
            </>
          )}
        </div>
        {selected.size > 0 ? (
          <div className="pointer-events-none sticky bottom-0 flex justify-center px-6 pb-4">
            <div className="animate-in pointer-events-auto flex items-center gap-4 rounded-lg bg-panel-raised px-4 py-2.5 shadow-float">
              <p className="text-[13px]">
                <span className="tabular font-medium">{formatCount(selected.size)}</span> selected,{" "}
                <span className="tabular font-medium">{formatBytes(bytesOf(selected))}</span>
              </p>
              <Button variant="ghost" size="sm" onClick={() => setPicked(new Set())}>
                Clear selection
              </Button>
              <Button onClick={reviewSelected}>Review {formatCount(selected.size)} selected</Button>
            </div>
          </div>
        ) : null}
      </div>
    </>
  );
}
