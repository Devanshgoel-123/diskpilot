import { Blocks, ChevronRight, Loader2, RefreshCw, Wrench } from "lucide-react";
import { useEffect, useMemo } from "react";
import { useShallow } from "zustand/react/shallow";

import { PageHeader } from "@/components/layout/PageHeader";

import { EmptyState } from "@/components/shared/EmptyState";
import { Button } from "@/components/ui/button";
import { Panel } from "@/components/ui/panel";
import { formatBytes, formatRelative } from "@/lib/format";
import { useApp } from "@/lib/store";

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
  const reviewBytes = useMemo(() => {
    const ids = new Set(reviewIds);
    return recommendations
      .filter((r) => ids.has(r.id))
      .reduce((a, r) => a + r.estimatedReclaimBytes, 0);
  }, [reviewIds, recommendations]);

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
        <div className="mx-auto max-w-[1180px] space-y-4 px-6 py-5">
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
                    <Button onClick={() => reviewInRecommendations(reviewIds)}>
                      Review {reviewIds.length} items
                    </Button>
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
                    Claude Code, Codex and other AI agents are on the AI Agents page
                  </span>
                  <span className="block text-xs text-muted-foreground">
                    Their caches, old versions, conversation data and the check for keys in
                    transcripts moved there, next to their permissions and skills.
                  </span>
                </span>
                <ChevronRight className="size-4 shrink-0 text-muted-foreground" aria-hidden />
              </button>

              {missing.length ? (
                <p className="text-xs text-subtle-foreground">
                  Not found on this Mac: {missing.map((t) => t.name).join(", ")}.
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
      </div>
    </>
  );
}
