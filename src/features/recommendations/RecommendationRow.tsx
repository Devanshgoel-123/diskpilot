import { ChevronDown, FolderOpen, FolderTree, Terminal } from "lucide-react";
import type { ReactNode } from "react";

import { AppIcon } from "@/components/shared/AppIcon";
import { CategoryBadge } from "@/components/shared/CategoryBadge";
import { CopyButton } from "@/components/shared/CopyButton";
import { SizeBar } from "@/components/shared/SizeBar";
import { TierBadge } from "@/components/shared/TierBadge";
import { Button } from "@/components/ui/button";
import { api } from "@/lib/api";
import { CONFIDENCE_LABEL } from "@/lib/categories";
import { formatBytes, formatCount, formatRelative, tildify } from "@/lib/format";
import { useApp } from "@/lib/store";
import { TIER_META } from "@/lib/tiers";
import type { Recommendation } from "@/lib/types";
import { cn } from "@/lib/utils";

function Fact({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div>
      <dt className="text-2xs font-medium tracking-wide text-subtle-foreground uppercase">
        {label}
      </dt>
      <dd className="selectable mt-1 text-xs leading-relaxed text-muted-foreground">{children}</dd>
    </div>
  );
}

export function RecommendationRow({
  rec,
  max,
  selected,
  expanded,
  onToggleSelect,
  onToggleExpand,
}: {
  rec: Recommendation;
  max: number;
  selected: boolean;
  expanded: boolean;
  onToggleSelect: () => void;
  onToggleExpand: () => void;
}) {
  const home = useApp((s) => s.env?.homeDir);
  const openInExplorer = useApp((s) => s.openInExplorer);
  const actionable = rec.methods.length > 0;
  const color = TIER_META[rec.tier].color;

  return (
    <li className={cn("border-b border-border last:border-0", selected && "bg-accent-soft/40")}>
      <div className="flex items-center gap-3 px-4 py-2.5">
        <div className="flex w-5 shrink-0 justify-center">
          {actionable ? (
            <input
              type="checkbox"
              checked={selected}
              onChange={onToggleSelect}
              aria-label={`Select ${rec.title}`}
              className="size-3.5 accent-[var(--accent)]"
            />
          ) : (
            <span
              className="size-1.5 rounded-full"
              style={{ background: color }}
              title="No automated action"
              aria-hidden
            />
          )}
        </div>
        <button
          type="button"
          onClick={onToggleExpand}
          aria-expanded={expanded}
          className="flex min-w-0 flex-1 items-center gap-3 text-left"
        >
          <div className="min-w-0 flex-1">
            <div className="flex min-w-0 items-center gap-2">
              <AppIcon path={rec.path} />
              <span className="truncate text-[13px] font-medium">{rec.title}</span>
              <CategoryBadge category={rec.classification.category} />
              {rec.source !== "scan" ? (
                <span className="text-2xs text-subtle-foreground">via Developer Tools</span>
              ) : null}
              {!actionable ? (
                <span className="text-2xs text-subtle-foreground">
                  {rec.tier === "keep" ? "Keep" : "Manual"}
                </span>
              ) : null}
            </div>
            <p className="mt-0.5 truncate font-mono text-2xs text-subtle-foreground">
              {tildify(rec.path, home)}
            </p>
          </div>
          <div className="w-32 shrink-0">
            <p className="tabular text-right text-[13px]">{formatBytes(rec.bytes)}</p>
            <SizeBar value={rec.bytes} max={max} color={color} className="mt-1" />
          </div>
          <ChevronDown
            className={cn(
              "size-3.5 shrink-0 text-subtle-foreground transition-transform",
              expanded && "rotate-180",
            )}
            aria-hidden
          />
        </button>
      </div>

      {expanded ? (
        <div className="px-4 pb-4 pl-12">
          <dl className="grid grid-cols-1 gap-x-8 gap-y-3 rounded-lg border border-border bg-panel-raised p-4 md:grid-cols-2">
            <Fact label="Why it was detected">{rec.detectedBecause}</Fact>
            {rec.safeBecause ? <Fact label="Why it may be safe">{rec.safeBecause}</Fact> : null}
            <Fact label="Risks">{rec.risks}</Fact>
            <Fact label="Proposed action">
              {rec.proposedAction}
              {actionable ? (
                <span className="block text-subtle-foreground">
                  DiskPilot can:{" "}
                  {rec.methods
                    .map((m) => (m === "trash" ? "move to Trash" : "delete permanently"))
                    .join(" or ")}
                </span>
              ) : (
                <span className="block text-subtle-foreground">
                  DiskPilot will not remove this.
                </span>
              )}
            </Fact>
            <Fact label="Assessment">
              <span className="flex flex-wrap items-center gap-1.5">
                <TierBadge tier={rec.tier} />
                <span>{CONFIDENCE_LABEL[rec.confidence]}</span>
              </span>
              <span className="mt-1 block">
                {formatCount(rec.fileCount)} files · estimated reclaim{" "}
                {formatBytes(rec.estimatedReclaimBytes)}
                {rec.lastActivityMs
                  ? ` · project files changed ${formatRelative(rec.lastActivityMs)}`
                  : ""}
              </span>
            </Fact>
            {rec.alternative ? (
              <Fact label="Or run it yourself">
                <span className="flex items-center gap-2">
                  <code className="flex min-w-0 items-center gap-1.5 rounded-md border border-border bg-panel px-2 py-1 font-mono text-2xs text-foreground">
                    <Terminal className="size-3 shrink-0 text-subtle-foreground" aria-hidden />
                    <span className="truncate">{rec.alternative}</span>
                  </code>
                  <CopyButton text={rec.alternative} />
                </span>
              </Fact>
            ) : null}
            {rec.notes.length ? (
              <div className="md:col-span-2">
                <Fact label="Notes">
                  {rec.notes.map((n) => (
                    <span key={n} className="block">
                      {n}
                    </span>
                  ))}
                </Fact>
              </div>
            ) : null}
          </dl>
          <div className="mt-2 flex gap-1.5">
            <Button variant="ghost" size="sm" onClick={() => void api.revealInFinder(rec.path)}>
              <FolderOpen />
              Reveal in Finder
            </Button>
            <Button variant="ghost" size="sm" onClick={() => openInExplorer(rec.path)}>
              <FolderTree />
              Open in Explorer
            </Button>
          </div>
        </div>
      ) : null}
    </li>
  );
}
