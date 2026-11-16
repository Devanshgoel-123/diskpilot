import {
  Blocks,
  AppWindow,
  Bot,
  Box,
  Code2,
  Copy,
  FolderTree,
  Gauge,
  History,
  KeyRound,
  Settings,
  Sparkles,
  type LucideIcon,
} from "lucide-react";
import { useShallow } from "zustand/react/shallow";

import { formatCount, formatRelative, tildify } from "@/lib/format";
import { shortcutLabel } from "@/lib/shortcuts";
import { useApp, type PageId } from "@/lib/store";
import { cn } from "@/lib/utils";
import { UpdateBanner } from "@/components/layout/UpdateBanner";
import { useAgents } from "@/features/agents/agentsStore";
import { useKeys } from "@/features/keys/keysStore";

interface NavItem {
  id: PageId;
  label: string;
  icon: LucideIcon;
  /** Optional count shown at the trailing edge. */
  count?: number;
  /** What the count means, for the tooltip and screen readers. */
  countLabel?: string;
  /** Warn counts are things that need a look, in the review color. */
  tone?: "warn";
}

function NavButton({ item }: { item: NavItem }) {
  const page = useApp((s) => s.page);
  const navigate = useApp((s) => s.navigate);
  const active = page === item.id;
  const Icon = item.icon;
  return (
    <button
      type="button"
      onClick={() => navigate(item.id)}
      title={
        [
          item.label,
          shortcutLabel(item.id) ? `(${shortcutLabel(item.id)})` : null,
          item.count && item.countLabel ? `· ${item.countLabel}` : null,
        ]
          .filter(Boolean)
          .join(" ") || undefined
      }
      aria-keyshortcuts={shortcutLabel(item.id)?.replace("⌘", "Meta+") ?? undefined}
      aria-current={active ? "page" : undefined}
      className={cn(
        "flex h-7 w-full items-center gap-2 rounded-md px-2 text-left text-[13px] transition-colors duration-[120ms]",
        active ? "bg-selected text-foreground" : "text-foreground/85 hover:bg-hover",
      )}
    >
      <Icon
        className={cn("size-4 shrink-0", active ? "text-accent" : "text-muted-foreground")}
        strokeWidth={1.75}
        aria-hidden
      />
      <span className="min-w-0 flex-1 truncate">{item.label}</span>
      {item.count ? (
        item.tone === "warn" ? (
          <span className="tabular flex items-center gap-1 text-xs text-tier-review">
            <span className="size-[5px] rounded-full bg-tier-review" aria-hidden />
            {item.count}
            {item.countLabel ? <span className="sr-only"> {item.countLabel}</span> : null}
          </span>
        ) : (
          <span className="tabular text-xs text-subtle-foreground">
            {item.count}
            {item.countLabel ? <span className="sr-only"> {item.countLabel}</span> : null}
          </span>
        )
      ) : null}
    </button>
  );
}

function Section({ title, items }: { title?: string; items: NavItem[] }) {
  return (
    <div>
      {title ? (
        <p className="px-2 pb-1 text-2xs font-semibold text-subtle-foreground">{title}</p>
      ) : null}
      <div className="space-y-px">
        {items.map((item) => (
          <NavButton key={item.id} item={item} />
        ))}
      </div>
    </div>
  );
}

/** Scan status at the foot of the sidebar: what was scanned and when. */
function ScanStatus() {
  const { scanning, partial, progress, summary, env } = useApp(
    useShallow((s) => ({
      scanning: s.scanning,
      partial: s.partial,
      progress: s.progress,
      summary: s.summary,
      env: s.env,
    })),
  );
  const home = env?.homeDir;
  if (scanning && partial && progress && progress.containersTotal > 0) {
    const pct = Math.round((progress.containersDone / progress.containersTotal) * 100);
    return (
      <div className="px-2 py-1.5" aria-live="polite">
        <p className="text-xs font-medium">Measuring app containers…</p>
        <div
          className="mt-1.5 h-[3px] overflow-hidden rounded-full bg-track"
          role="progressbar"
          aria-valuenow={pct}
          aria-valuemin={0}
          aria-valuemax={100}
          aria-label="App containers measured"
        >
          <div className="h-full rounded-full bg-accent" style={{ width: `${pct}%` }} />
        </div>
        <p className="tabular mt-1 text-2xs text-subtle-foreground">
          {formatCount(progress.containersDone)} of {formatCount(progress.containersTotal)}
        </p>
      </div>
    );
  }
  if (scanning) {
    return (
      <div className="px-2 py-1.5" aria-live="polite">
        <p className="text-xs font-medium">Scanning…</p>
        <div className="mt-1.5 h-[3px] overflow-hidden rounded-full bg-track">
          <div className="scan-indeterminate h-full w-1/3 rounded-full bg-accent" />
        </div>
        <p className="mt-1 truncate font-mono text-2xs text-subtle-foreground">
          {progress ? tildify(progress.currentPath, home) : "Starting"}
        </p>
      </div>
    );
  }
  if (!summary) {
    return <p className="px-2 py-1.5 text-xs text-subtle-foreground">Not scanned yet</p>;
  }
  return (
    <div className="px-2 py-1.5">
      <p className="text-xs text-muted-foreground">
        Scanned {formatRelative(summary.finishedAtMs)}
      </p>
      {/* A lone "~" reads like a stray character; name the usual case. */}
      {tildify(summary.root, home) === "~" ? (
        <p className="truncate text-2xs text-subtle-foreground">Home folder</p>
      ) : (
        <p className="truncate font-mono text-2xs text-subtle-foreground">
          {tildify(summary.root, home)}
        </p>
      )}
    </div>
  );
}

export function Sidebar() {
  const recommendations = useApp((s) => s.recommendations);
  const assistantEnabled = useApp((s) => s.assistantEnabled);
  const cleanupCount = recommendations.filter((r) => r.methods.length > 0).length;
  // Shown once each page has been opened; neither is read just for the sidebar.
  const riskyRules = useAgents(
    (s) =>
      s.report?.agents
        .filter((a) => a.detected)
        .reduce((n, a) => n + a.permissions.filter((p) => p.risk).length, 0) ?? 0,
  );
  const exposedKeys = useKeys(
    (s) => s.report?.files.filter((f) => f.risks.some((r) => r.serious)).length ?? 0,
  );

  // What most people come for first: the summary and the one place to get
  // space back. Then where the space went, then tools for developers.
  const main: NavItem[] = [
    { id: "overview", label: "Overview", icon: Gauge },
    {
      id: "recommendations",
      label: "Cleanup",
      icon: Sparkles,
      count: cleanupCount,
      countLabel: "suggestions you can act on",
    },
    ...(assistantEnabled ? [{ id: "assistant" as const, label: "Assistant", icon: Bot }] : []),
  ];
  const findSpace: NavItem[] = [
    { id: "explorer", label: "Storage", icon: FolderTree },
    { id: "duplicates", label: "Duplicates", icon: Copy },
    { id: "applications", label: "Applications", icon: AppWindow },
  ];
  const developers: NavItem[] = [
    {
      id: "agents",
      label: "AI Agents",
      icon: Blocks,
      count: riskyRules,
      countLabel: "settings let agents act without asking",
      tone: "warn",
    },
    {
      id: "keys",
      label: "Keys & secrets",
      icon: KeyRound,
      count: exposedKeys,
      countLabel: "key files need attention",
      tone: "warn",
    },
    { id: "developer", label: "Developer Tools", icon: Code2 },
    { id: "docker", label: "Docker", icon: Box },
  ];

  return (
    <nav
      aria-label="Main"
      className="flex w-[220px] shrink-0 flex-col border-r border-border bg-sidebar"
    >
      {/* Space for the macOS traffic lights; doubles as a window drag handle. */}
      <div data-tauri-drag-region className="h-[38px] shrink-0" />
      <div data-tauri-drag-region className="flex items-center gap-2 px-4 pb-3">
        <img src="/logo.svg" alt="" className="pointer-events-none size-[22px]" />
        <span className="pointer-events-none text-[13px] font-semibold">DiskPilot</span>
      </div>
      <div className="flex-1 space-y-4 overflow-y-auto px-2.5 pb-2">
        <Section items={main} />
        <Section title="Find space" items={findSpace} />
        <Section title="For developers" items={developers} />
      </div>
      <div className="space-y-1 border-t border-border px-2.5 pt-2 pb-2.5">
        <UpdateBanner />
        <ScanStatus />
        <NavButton item={{ id: "history", label: "History", icon: History }} />
        <NavButton item={{ id: "settings", label: "Settings", icon: Settings }} />
      </div>
    </nav>
  );
}
