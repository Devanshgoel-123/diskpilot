import { History } from "lucide-react";
import { useEffect } from "react";

import { PageHeader } from "@/components/layout/PageHeader";
import { EmptyState } from "@/components/shared/EmptyState";
import { Badge } from "@/components/ui/badge";
import { Panel, PanelHeader } from "@/components/ui/panel";
import { formatBytes, formatCount, formatDate, formatDuration, tildify } from "@/lib/format";
import { useApp } from "@/lib/store";
import type { ScanStatus } from "@/lib/types";

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
      <PageHeader title="History" description="Scans and cleanup actions on this Mac." />
      <div className="min-h-0 flex-1 overflow-y-auto">
        <div className="mx-auto max-w-[1180px] space-y-4 px-6 py-5">
          <Panel>
            <PanelHeader
              title="Cleanup actions"
              description="Executed actions, affected paths, commands, reclaimed space and errors will be recorded here."
            />
            <p className="text-muted-foreground px-4 pb-4 text-xs">
              No cleanup actions have been executed. This version of DiskPilot is read-only.
            </p>
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
                  <tr className="border-border text-muted-foreground border-y text-left text-xs">
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
                    <tr key={r.id} className="border-border/60 h-8 border-b last:border-0">
                      <td className="truncate px-4 text-xs">{formatDate(r.startedAtMs)}</td>
                      <td className="text-2xs text-muted-foreground truncate px-2 font-mono">
                        {tildify(r.root, home)}
                      </td>
                      <td className="tabular px-2 text-right">
                        {r.status === "completed" ? formatBytes(r.totalBytes) : "—"}
                      </td>
                      <td className="text-muted-foreground tabular px-2 text-right text-xs">
                        {r.status === "completed" ? formatCount(r.fileCount) : "—"}
                      </td>
                      <td className="text-muted-foreground tabular px-2 text-right text-xs">
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
