import { Code2 } from "lucide-react";

import { PageHeader } from "@/components/layout/PageHeader";
import { EmptyState } from "@/components/shared/EmptyState";
import { RoadmapNote } from "@/components/shared/RoadmapNote";
import { Button } from "@/components/ui/button";
import { Panel, PanelHeader } from "@/components/ui/panel";
import { groupByTool } from "@/lib/analysis";
import { formatBytes } from "@/lib/format";
import { useApp } from "@/lib/store";

import { LocationList } from "../overview/LocationList";

export function DeveloperToolsPage() {
  const { summary, scanning, startScan } = useApp();
  const groups = summary ? groupByTool(summary.detectedLocations).filter((g) => g.bytes > 0) : [];

  return (
    <>
      <PageHeader
        title="Developer Tools"
        description="Storage used by developer tools, detected by the last scan."
      />
      <div className="min-h-0 flex-1 overflow-y-auto">
        <div className="mx-auto max-w-[1180px] space-y-4 px-6 py-5">
          {!summary ? (
            <Panel>
              <EmptyState
                icon={Code2}
                title={scanning ? "Scanning…" : "No scan yet"}
                description="Scan your home folder to detect node_modules, package manager caches, Xcode data, Cursor and VS Code storage, and more."
                action={
                  scanning ? null : <Button onClick={() => void startScan()}>Start scan</Button>
                }
              />
            </Panel>
          ) : groups.length === 0 ? (
            <Panel>
              <EmptyState
                icon={Code2}
                title="No developer storage detected"
                description="The scanned folder does not contain any locations DiskPilot recognizes as developer tooling."
              />
            </Panel>
          ) : (
            <div className="grid grid-cols-1 gap-4 xl:grid-cols-2">
              {groups.map((g) => (
                <Panel key={g.id}>
                  <PanelHeader
                    title={g.name}
                    description={g.description}
                    actions={
                      <span className="tabular text-[13px] font-medium">
                        {formatBytes(g.bytes)}
                      </span>
                    }
                  />
                  <LocationList locations={g.locations.slice(0, 6)} />
                  {g.locations.length > 6 ? (
                    <p className="border-border text-2xs text-subtle-foreground border-t px-4 py-2">
                      +{g.locations.length - 6} more locations
                    </p>
                  ) : null}
                </Panel>
              ))}
            </div>
          )}
          <RoadmapNote phase="Phase 4" title="Deeper tool analysis is planned">
            Today this page shows what the scanner detected by path. Planned: Cursor agent versions
            and workspace storage per project, npm/pnpm/Yarn cache verification, Xcode simulator
            runtimes and device support by OS version, and stale <code>node_modules</code> detection
            by project activity. Totals exclude nested items so nothing is counted twice.
          </RoadmapNote>
        </div>
      </div>
    </>
  );
}
