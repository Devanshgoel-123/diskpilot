import {
  Bot,
  BrainCircuit,
  ChevronDown,
  Cloud,
  Code2,
  FileCode,
  Hammer,
  Hexagon,
  History,
  MousePointer2,
  Sparkles,
  SquareTerminal,
  Wrench,
  type LucideIcon,
} from "lucide-react";
import { type CSSProperties, useState } from "react";

import { AppIcon } from "@/components/shared/AppIcon";

import { Button } from "@/components/ui/button";
import { Panel } from "@/components/ui/panel";
import { formatBytes, formatCount, formatRelative, tildify } from "@/lib/format";
import { useApp } from "@/lib/store";
import type { ItemState, ToolReport } from "@/lib/types";
import { cn } from "@/lib/utils";

import { TOOL_MARKS } from "./toolMarks";

const TOOL_ICON: Record<string, LucideIcon> = {
  cursor: MousePointer2,
  vscode: Code2,
  windsurf: Code2,
  "claude-code": SquareTerminal,
  "claude-desktop": Sparkles,
  codex: Cloud,
  copilot: Bot,
  "ai-agents": Bot,
  node: Hexagon,
  xcode: Hammer,
  rust: Wrench,
  python: FileCode,
  "local-models": BrainCircuit,
  versions: History,
};

const TOOL_COLOR: Record<string, string> = {
  cursor: "var(--cat-developer-tools)",
  vscode: "var(--cat-application-data)",
  windsurf: "var(--cat-cache)",
  "claude-code": "var(--cat-user-data)",
  "claude-desktop": "var(--cat-user-data)",
  codex: "var(--cat-application-data)",
  copilot: "var(--cat-developer-tools)",
  "ai-agents": "var(--cat-user-data)",
  node: "var(--cat-downloads)",
  xcode: "var(--cat-docker-storage)",
  rust: "var(--cat-build-artifacts)",
  python: "var(--cat-cache)",
  "local-models": "var(--cat-binary-data)",
  versions: "var(--cat-system-files)",
};

/**
 * Where the installed app's own icon comes from: an app bundle (absolute) or
 * a folder in the home folder that carries the app's icon.
 */
const TOOL_APP_PATH: Record<string, string> = {
  cursor: ".cursor",
  vscode: "/Applications/Visual Studio Code.app",
  windsurf: ".windsurf",
  "claude-code": ".claude",
  "claude-desktop": "Library/Application Support/Claude",
  codex: ".codex",
  xcode: "Library/Developer/Xcode",
};

const STATE_META: Record<ItemState, { label: string; color: string }> = {
  current: { label: "In use", color: "var(--tier-safe)" },
  stale: { label: "Inactive", color: "var(--tier-review)" },
  orphaned: { label: "Leftover", color: "var(--tier-review)" },
  superseded: { label: "Superseded", color: "var(--tier-review)" },
  info: { label: "", color: "var(--tier-keep)" },
};

function StatePill({ state }: { state: ItemState }) {
  const meta = STATE_META[state];
  if (!meta.label) return null;
  return (
    <span
      style={{ "--tint": meta.color } as CSSProperties}
      className="tinted tint-text inline-flex shrink-0 items-center rounded-full border px-1.5 py-px text-2xs font-medium"
    >
      {meta.label}
    </span>
  );
}

export function ToolCard({
  tool,
  reviewable,
  onReview,
  defaultOpen = false,
}: {
  tool: ToolReport;
  reviewable: Set<string>;
  onReview: (ids: string[]) => void;
  /** Start with the details shown, for a card that is the page's only content. */
  defaultOpen?: boolean;
}) {
  const home = useApp((s) => s.env?.homeDir);
  const [open, setOpen] = useState(defaultOpen);
  const Icon = TOOL_ICON[tool.id] ?? Wrench;
  const rel = TOOL_APP_PATH[tool.id];
  const appPath = !rel ? null : rel.startsWith("/") ? rel : home ? `${home}/${rel}` : null;
  const mark = TOOL_MARKS[tool.id];
  const color = TOOL_COLOR[tool.id] ?? "var(--accent)";
  const items = tool.sections.flatMap((s) => s.items);
  const actionable = items.filter((i) => i.recommendationId && reviewable.has(i.recommendationId));
  const actionableBytes = actionable.reduce((a, i) => a + i.bytes, 0);
  const counts = (["orphaned", "stale", "superseded"] as const)
    .map((s) => ({ state: s, n: items.filter((i) => i.state === s).length }))
    .filter((c) => c.n > 0);

  return (
    <Panel className={cn(open && "md:col-span-2")}>
      <div className="flex items-start gap-3 px-4 py-3.5">
        <AppIcon
          path={appPath}
          className="size-8"
          fallback={
            <span className="flex size-8 shrink-0 items-center justify-center rounded-lg bg-panel-raised">
              {mark ? (
                <svg
                  viewBox="0 0 24 24"
                  className="size-[18px]"
                  fill={mark.color ?? "currentColor"}
                  aria-hidden
                  focusable="false"
                >
                  <path d={mark.path} />
                </svg>
              ) : (
                <Icon className="size-4" style={{ color }} strokeWidth={1.75} aria-hidden />
              )}
            </span>
          }
        />
        <div className="min-w-0 flex-1">
          <div className="flex items-baseline justify-between gap-3">
            <p className="truncate text-[13px] font-medium">
              {tool.name}
              {tool.version ? (
                <span className="ml-2 font-mono text-2xs font-normal text-subtle-foreground">
                  {tool.version}
                </span>
              ) : null}
            </p>
            <span className="tabular shrink-0 text-[13px] font-semibold">
              {formatBytes(tool.totalBytes)}
            </span>
          </div>
          <p className="mt-0.5 text-xs leading-relaxed text-muted-foreground">{tool.summary}</p>
          <div className="mt-2 flex flex-wrap items-center gap-1.5">
            {counts.map((c) => (
              <span
                key={c.state}
                className="flex items-center gap-1 text-2xs text-muted-foreground"
              >
                <StatePill state={c.state} />
                {formatCount(c.n)}
              </span>
            ))}
            <div className="ml-auto flex items-center gap-1.5">
              {actionable.length ? (
                <Button
                  variant="secondary"
                  size="sm"
                  onClick={() =>
                    onReview(
                      actionable.map((i) => i.recommendationId).filter((x): x is string => !!x),
                    )
                  }
                >
                  Review {actionable.length} · {formatBytes(actionableBytes)}
                </Button>
              ) : null}
              {tool.sections.length ? (
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={() => setOpen((o) => !o)}
                  aria-expanded={open}
                >
                  Details
                  <ChevronDown className={cn("transition-transform", open && "rotate-180")} />
                </Button>
              ) : null}
            </div>
          </div>
        </div>
      </div>

      {open ? (
        <div className="space-y-4 border-t border-border px-4 py-3">
          {tool.sections.map((section) => (
            <div key={section.title}>
              <p className="text-xs font-medium">{section.title}</p>
              <p className="mt-0.5 text-2xs leading-snug text-subtle-foreground">
                {section.description}
              </p>
              <ul className="mt-2 max-h-80 divide-y divide-border overflow-y-auto rounded-md border border-border">
                {section.items.map((item, idx) => {
                  const canReview =
                    !!item.recommendationId && reviewable.has(item.recommendationId);
                  return (
                    <li
                      key={`${item.path ?? item.label}-${idx}`}
                      className="flex items-center gap-3 px-3 py-1.5"
                    >
                      <div className="min-w-0 flex-1">
                        <p className="flex min-w-0 items-center gap-2 text-xs">
                          <span className="truncate">{item.label}</span>
                          <StatePill state={item.state} />
                        </p>
                        <p
                          className="truncate text-2xs text-subtle-foreground"
                          title={item.path ?? undefined}
                        >
                          {item.detail
                            ? tildify(item.detail, home)
                            : item.path
                              ? tildify(item.path, home)
                              : ""}
                        </p>
                      </div>
                      <span className="hidden w-24 shrink-0 text-right text-2xs text-subtle-foreground lg:block">
                        {item.lastUsedMs ? formatRelative(item.lastUsedMs) : ""}
                      </span>
                      <span className="tabular w-20 shrink-0 text-right text-xs">
                        {formatBytes(item.bytes)}
                      </span>
                      <span className="w-16 shrink-0 text-right">
                        {canReview ? (
                          <Button
                            variant="ghost"
                            size="sm"
                            onClick={() =>
                              item.recommendationId && onReview([item.recommendationId])
                            }
                          >
                            Review
                          </Button>
                        ) : null}
                      </span>
                    </li>
                  );
                })}
              </ul>
            </div>
          ))}
        </div>
      ) : null}
    </Panel>
  );
}
