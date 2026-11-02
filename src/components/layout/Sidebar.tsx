import {
  AppWindow,
  Bot,
  Box,
  Code2,
  FileStack,
  FolderTree,
  Gauge,
  History,
  Settings,
  Sparkles,
  type LucideIcon,
} from "lucide-react";

import { formatRelative, tildify } from "@/lib/format";
import { useApp, type PageId } from "@/lib/store";
import { cn } from "@/lib/utils";

interface NavItem {
  id: PageId;
  label: string;
  icon: LucideIcon;
  /** Optional count shown at the trailing edge. */
  count?: number;
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
        <span className="tabular text-xs text-subtle-foreground">{item.count}</span>
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
  const { scanning, progress, summary, env } = useApp();
  const home = env?.homeDir;
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
      <p className="truncate font-mono text-2xs text-subtle-foreground">
        {tildify(summary.root, home)}
      </p>
    </div>
  );
}

export function Sidebar() {
  const recommendations = useApp((s) => s.recommendations);
  const assistantEnabled = useApp((s) => s.assistantEnabled);
  const cleanupCount = recommendations.filter((r) => r.methods.length > 0).length;

  const analyze: NavItem[] = [
    { id: "explorer", label: "Storage", icon: FolderTree },
    { id: "large-files", label: "Large Files", icon: FileStack },
    { id: "applications", label: "Applications", icon: AppWindow },
  ];
  const cleanUp: NavItem[] = [
    { id: "recommendations", label: "Cleanup", icon: Sparkles, count: cleanupCount },
    { id: "developer", label: "Developer Tools", icon: Code2 },
    { id: "docker", label: "Docker", icon: Box },
  ];
  const activity: NavItem[] = [
    { id: "history", label: "Cleanup History", icon: History },
    ...(assistantEnabled ? [{ id: "assistant" as const, label: "Assistant", icon: Bot }] : []),
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
        <Section items={[{ id: "overview", label: "Overview", icon: Gauge }]} />
        <Section title="Analyze" items={analyze} />
        <Section title="Clean up" items={cleanUp} />
        <Section title="Activity" items={activity} />
      </div>
      <div className="space-y-1 border-t border-border px-2.5 pt-2 pb-2.5">
        <ScanStatus />
        <NavButton item={{ id: "settings", label: "Settings", icon: Settings }} />
      </div>
    </nav>
  );
}
