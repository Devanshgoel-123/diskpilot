import { AlertCircle, MonitorX, X } from "lucide-react";
import { useEffect, useRef } from "react";
import { useShallow } from "zustand/react/shallow";

import { Sidebar } from "@/components/layout/Sidebar";
import { EmptyState } from "@/components/shared/EmptyState";
import { Button } from "@/components/ui/button";
import { TooltipProvider } from "@/components/ui/tooltip";
import { ApplicationsPage } from "@/features/applications/ApplicationsPage";
import { AssistantPage } from "@/features/assistant/AssistantPage";
import { DeveloperToolsPage } from "@/features/developer/DeveloperToolsPage";
import { DockerPage } from "@/features/docker/DockerPage";
import { DuplicatesPage } from "@/features/duplicates/DuplicatesPage";
import { HistoryPage } from "@/features/history/HistoryPage";
import { OverviewPage } from "@/features/overview/OverviewPage";
import { RecommendationsPage } from "@/features/recommendations/RecommendationsPage";
import { SettingsPage } from "@/features/settings/SettingsPage";
import { StoragePage } from "@/features/storage/StoragePage";
import { EmptyTrashDialog } from "@/features/trash/EmptyTrashDialog";
import { useTrash } from "@/features/trash/trashStore";
import { inDesktopApp, onTrayNavigate } from "@/lib/api";
import { useApp, type PageId } from "@/lib/store";
import { ShortcutsDialog } from "@/components/layout/ShortcutsDialog";
import { WhatsNewAfterUpdate } from "@/components/layout/WhatsNewAfterUpdate";
import { runShortcut, useShortcutsDialog } from "@/lib/shortcuts";
import { startUpdateChecks } from "@/lib/updates";
import { playScanDone, startScanSound, stopScanSound } from "@/lib/sounds";

const PAGES: Record<PageId, () => React.JSX.Element> = {
  assistant: AssistantPage,
  applications: ApplicationsPage,
  overview: OverviewPage,
  explorer: StoragePage,
  recommendations: RecommendationsPage,
  docker: DockerPage,
  developer: DeveloperToolsPage,
  duplicates: DuplicatesPage,
  history: HistoryPage,
  settings: SettingsPage,
};

function StatusBanner() {
  const { error, lastStatus, dismissError } = useApp(
    useShallow((s) => ({ error: s.error, lastStatus: s.lastStatus, dismissError: s.dismissError })),
  );
  const message = error ?? (lastStatus?.status === "failed" ? lastStatus.error : null);
  if (!message) return null;
  return (
    <div
      role="alert"
      className="flex items-center gap-2 border-b border-border bg-panel px-6 py-2 text-xs text-critical"
    >
      <AlertCircle className="size-3.5 shrink-0" aria-hidden />
      <span className="selectable min-w-0 flex-1">{message}</span>
      <Button variant="ghost" size="icon" aria-label="Dismiss" onClick={dismissError}>
        <X />
      </Button>
    </div>
  );
}

/** A quiet tone while a scan runs, and a soft chime when it completes. */
function ScanSounds() {
  const scanning = useApp((s) => s.scanning);
  const status = useApp((s) => s.lastStatus?.status);
  const was = useRef(false);
  useEffect(() => {
    if (scanning && !was.current) startScanSound();
    if (!scanning && was.current) {
      stopScanSound();
      if (status === "completed") playScanDone();
    }
    was.current = scanning;
  }, [scanning, status]);
  useEffect(() => () => stopScanSound(), []);
  return null;
}

export function App() {
  const { ready, desktop, page, init } = useApp(
    useShallow((s) => ({ ready: s.ready, desktop: s.desktop, page: s.page, init: s.init })),
  );
  const shortcuts = useShortcutsDialog();

  // App-wide keyboard shortcuts (⌘1 to ⌘8, ⌘R, ⌘F and the rest).
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (runShortcut(e, useApp.getState())) e.preventDefault();
    };
    addEventListener("keydown", onKey);
    return () => removeEventListener("keydown", onKey);
  }, []);

  useEffect(() => {
    void init();
  }, [init]);

  // Look for a newer DiskPilot shortly after launch, then daily.
  useEffect(() => {
    if (inDesktopApp()) startUpdateChecks();
  }, []);

  // Shortcuts from the menu bar (tray) item.
  useEffect(() => {
    if (!inDesktopApp()) return;
    const un = onTrayNavigate((target) => {
      if (target === "trash") useTrash.getState().openDialog();
      else if (target in PAGES) useApp.getState().navigate(target as PageId);
    });
    return () => void un.then((f) => f());
  }, []);

  if (ready && !desktop) {
    return (
      <EmptyState
        className="h-full justify-center"
        icon={MonitorX}
        title="DiskPilot runs as a desktop app"
        description={
          <>
            This page is the app's interface without its engine. Start it with{" "}
            <code className="font-mono">pnpm app:dev</code> to scan your disk.
          </>
        }
      />
    );
  }

  const Page = PAGES[page];
  return (
    <TooltipProvider delayDuration={300}>
      <div className="flex h-full">
        <Sidebar />
        <main className="flex min-w-0 flex-1 flex-col">
          <StatusBanner />
          <Page />
        </main>
      </div>
      <ScanSounds />
      <EmptyTrashDialog />
      <WhatsNewAfterUpdate />
      <ShortcutsDialog open={shortcuts.open} onOpenChange={shortcuts.setOpen} />
    </TooltipProvider>
  );
}
