import {
  Box,
  Code2,
  FileStack,
  FolderTree,
  History,
  LayoutDashboard,
  Settings,
  ShieldCheck,
  Sparkles,
  type LucideIcon,
} from "lucide-react";

import { useApp, type PageId } from "@/lib/store";
import { cn } from "@/lib/utils";

interface NavItem {
  id: PageId;
  label: string;
  icon: LucideIcon;
  /** Shown for modules that are not built yet. */
  soon?: boolean;
}

const SECTIONS: { title: string; items: NavItem[] }[] = [
  {
    title: "Analyze",
    items: [
      { id: "overview", label: "Overview", icon: LayoutDashboard },
      { id: "explorer", label: "Storage Explorer", icon: FolderTree },
      { id: "large-files", label: "Large Files", icon: FileStack },
    ],
  },
  {
    title: "Clean up",
    items: [
      { id: "recommendations", label: "Recommendations", icon: Sparkles, soon: true },
      { id: "docker", label: "Docker", icon: Box, soon: true },
      { id: "developer", label: "Developer Tools", icon: Code2 },
      { id: "history", label: "History", icon: History },
    ],
  },
];

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
        "group flex h-7 w-full items-center gap-2 rounded-md px-2 text-left text-[13px] transition-colors",
        active
          ? "bg-hover text-foreground font-medium"
          : "text-muted-foreground hover:bg-hover/60 hover:text-foreground",
      )}
    >
      <Icon
        className={cn("size-[15px] shrink-0", active ? "text-accent" : "text-subtle-foreground")}
        aria-hidden
      />
      <span className="truncate">{item.label}</span>
      {item.soon ? <span className="text-2xs text-subtle-foreground ml-auto">Soon</span> : null}
    </button>
  );
}

export function Sidebar() {
  const readOnly = useApp((s) => s.env?.readOnly ?? true);
  return (
    <nav className="border-border bg-sidebar flex w-[212px] shrink-0 flex-col border-r">
      {/* Space for the macOS traffic lights; doubles as a window drag handle. */}
      <div data-tauri-drag-region className="h-[46px] shrink-0" />
      <div className="flex items-center gap-2 px-4 pb-4">
        <img src="/logo.svg" alt="" className="size-5" />
        <span className="text-[13px] font-semibold tracking-tight">DiskPilot</span>
      </div>
      <div className="flex-1 space-y-5 overflow-y-auto px-2.5">
        {SECTIONS.map((section) => (
          <div key={section.title}>
            <p className="text-2xs text-subtle-foreground px-2 pb-1 font-medium tracking-wide uppercase">
              {section.title}
            </p>
            <div className="space-y-px">
              {section.items.map((item) => (
                <NavButton key={item.id} item={item} />
              ))}
            </div>
          </div>
        ))}
      </div>
      <div className="space-y-2 px-2.5 pt-2 pb-3">
        {readOnly ? (
          <div
            className="text-2xs text-muted-foreground flex items-center gap-2 rounded-md px-2 py-1.5"
            title="This version of DiskPilot cannot modify or delete files."
          >
            <ShieldCheck className="text-good size-3.5" aria-hidden />
            Read-only mode
          </div>
        ) : null}
        <NavButton item={{ id: "settings", label: "Settings", icon: Settings }} />
      </div>
    </nav>
  );
}
