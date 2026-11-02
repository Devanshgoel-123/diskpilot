import { History } from "lucide-react";
import { useEffect } from "react";

import { PageHeader } from "@/components/layout/PageHeader";
import { EmptyState } from "@/components/shared/EmptyState";
import { Badge } from "@/components/ui/badge";
import { Panel, PanelHeader } from "@/components/ui/panel";
import { formatBytes, formatCount, formatDate, formatDuration, tildify } from "@/lib/format";
import { useApp } from "@/lib/store";
import type { ScanStatus } from "@/lib/types";

import { CleanupHistory } from "./CleanupHistory";

const STATUS: Record<ScanStatus, string> = {
  completed: "Completed",
  cancelled: "Cancelled",
  failed: "Failed",
};

export function HistoryPage() {
  const { history, env, refreshHistory } = useApp();
  const home = env?.homeDir;

  useEffect(() => {
    void refreshHistory();
  }, [refreshHistory]);

  return (
    <>
      <PageHeader title="Cleanup History" description="Scans and cleanup actions on this Mac." />
      <div className="min-h-0 flex-1 overflow-y-auto">
        <div className="mx-auto max-w-[1180px] space-y-4 px-6 py-5">
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
              <table className="w-full table-fixed text-[13px]">
                <thead>
                  <tr className="border-y border-border text-left text-xs text-muted-foreground">
                    <th className="px-4 py-2 font-normal">Started</th>
                    <th className="px-2 py-2 font-normal">Folder</th>
                    <th className="w-24 px-2 py-2 text-right font-normal">Size</th>
                    <th className="w-24 px-2 py-2 text-right font-normal">Files</th>
                    <th className="w-20 px-2 py-2 text-right font-normal">Duration</th>
                    <th className="w-28 px-2 py-2 pr-4 font-normal">Status</th>
                  </tr>
                </thead>
                <tbody>
                  {history.map((r) => (
                    <tr key={r.id} className="h-8 border-b border-border/60 last:border-0">
                      <td className="truncate px-4 text-xs">{formatDate(r.startedAtMs)}</td>
                      <td className="truncate px-2 font-mono text-2xs text-muted-foreground">
                        {tildify(r.root, home)}
                      </td>
                      <td className="tabular px-2 text-right">
                        {r.status === "completed" ? formatBytes(r.totalBytes) : "—"}
                      </td>
                      <td className="tabular px-2 text-right text-xs text-muted-foreground">
                        {r.status === "completed" ? formatCount(r.fileCount) : "—"}
                      </td>
                      <td className="tabular px-2 text-right text-xs text-muted-foreground">
                        {formatDuration(r.durationMs)}
                      </td>
                      <td className="px-2 pr-4">
                        <Badge variant={r.status === "completed" ? "default" : "outline"}>
                          {STATUS[r.status]}
                        </Badge>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
          </Panel>
        </div>
      </div>
    </>
  );
}
