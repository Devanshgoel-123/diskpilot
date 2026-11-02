import { AlertCircle, MonitorX, X } from "lucide-react";
import { useEffect } from "react";

import { Sidebar } from "@/components/layout/Sidebar";
import { EmptyState } from "@/components/shared/EmptyState";
import { Button } from "@/components/ui/button";
import { TooltipProvider } from "@/components/ui/tooltip";
import { AssistantPage } from "@/features/assistant/AssistantPage";
import { DeveloperToolsPage } from "@/features/developer/DeveloperToolsPage";
import { DockerPage } from "@/features/docker/DockerPage";
import { ExplorerPage } from "@/features/explorer/ExplorerPage";
import { HistoryPage } from "@/features/history/HistoryPage";
import { LargeFilesPage } from "@/features/large-files/LargeFilesPage";
import { OverviewPage } from "@/features/overview/OverviewPage";
import { RecommendationsPage } from "@/features/recommendations/RecommendationsPage";
import { SettingsPage } from "@/features/settings/SettingsPage";
import { useApp, type PageId } from "@/lib/store";

const PAGES: Record<PageId, () => React.JSX.Element> = {
  assistant: AssistantPage,
  overview: OverviewPage,
  explorer: ExplorerPage,
  recommendations: RecommendationsPage,
  docker: DockerPage,
  developer: DeveloperToolsPage,
  "large-files": LargeFilesPage,
  history: HistoryPage,
  settings: SettingsPage,
};

function StatusBanner() {
  const { error, lastStatus, dismissError } = useApp();
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

export function App() {
  const { ready, desktop, page, init } = useApp();

  useEffect(() => {
    void init();
  }, [init]);

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
    </TooltipProvider>
  );
}
