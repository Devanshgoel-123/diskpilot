import { History } from "lucide-react";
import { useEffect, useMemo } from "react";
import { useShallow } from "zustand/react/shallow";

import { PageHeader } from "@/components/layout/PageHeader";
import { EmptyState } from "@/components/shared/EmptyState";
import { Pager } from "@/components/shared/Pager";
import { Panel, PanelHeader } from "@/components/ui/panel";
import { useApp } from "@/lib/store";
import type { ScanRecord } from "@/lib/types";
import { usePage } from "@/lib/usePage";

import { ClearedStatsPanel } from "./ClearedStats";
import { CleanupHistory } from "./CleanupHistory";
import { ScansTable } from "./ScansTable";

const SCANS_PER_PAGE = 25;

function PagedScans({ records, home }: { records: ScanRecord[]; home?: string }) {
  const sorted = useMemo(
    () => [...records].sort((a, b) => b.startedAtMs - a.startedAtMs),
    [records],
  );
  const page = usePage(sorted, SCANS_PER_PAGE);
  return (
    <>
      <ScansTable records={records} home={home} shown={page.items} />
      <Pager page={page} label="scans" className="border-t border-border px-4 py-2" />
    </>
  );
}

export function HistoryPage() {
  const { history, env, refreshHistory } = useApp(
    useShallow((s) => ({ history: s.history, env: s.env, refreshHistory: s.refreshHistory })),
  );
  const home = env?.homeDir;

  useEffect(() => {
    void refreshHistory();
  }, [refreshHistory]);

  return (
    <>
      <PageHeader title="History" description="Scans and cleanup actions on this Mac." />
      <div className="min-h-0 flex-1 overflow-y-auto">
        <div className="page-content space-y-4">
          <ClearedStatsPanel />
          <Panel>
            <PanelHeader
              title="Cleanup actions"
              description="What was executed, the exact operations or commands, space reclaimed and errors. Moved-to-Trash items can be put back from the Trash until it is emptied; permanently deleted items cannot be restored."
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
              <PagedScans records={history} home={home} />
            )}
          </Panel>
        </div>
      </div>
    </>
  );
}
