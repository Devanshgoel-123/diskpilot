import { History } from "lucide-react";
import { useEffect } from "react";

import { PageHeader } from "@/components/layout/PageHeader";
import { EmptyState } from "@/components/shared/EmptyState";
import { Panel, PanelHeader } from "@/components/ui/panel";
import { useApp } from "@/lib/store";
import { isWindows, words } from "@/lib/platform";

import { ClearedStatsPanel } from "./ClearedStats";
import { CleanupHistory } from "./CleanupHistory";
import { ScansTable } from "./ScansTable";

export function HistoryPage() {
  const { history, env, refreshHistory } = useApp();
  const home = env?.homeDir;

  useEffect(() => {
    void refreshHistory();
  }, [refreshHistory]);

  return (
    <>
      <PageHeader
        title="History"
        description={`Scans and cleanup actions on this ${words().computer}.`}
      />
      <div className="min-h-0 flex-1 overflow-y-auto">
        <div className="mx-auto max-w-[1180px] space-y-4 px-6 py-5">
          <ClearedStatsPanel />
          <Panel>
            <PanelHeader
              title="Cleanup actions"
              description={`What was executed, the exact operations or commands, space reclaimed and errors. ${isWindows() ? "Items moved to the Recycle Bin can be restored from it" : "Moved-to-Trash items can be put back from the Trash"} until it is emptied; permanently deleted items cannot be restored.`}
            />
            <CleanupHistory />
          </Panel>
          <Panel>
            <PanelHeader
              title="Scans"
              description="Stored locally in DiskPilot's app data folder."
            />
            {history.length === 0 ? (
              <EmptyState icon={History} title="No scans yet" className="py-8" />
            ) : (
              <ScansTable records={history} home={home} />
            )}
          </Panel>
        </div>
      </div>
    </>
  );
}
