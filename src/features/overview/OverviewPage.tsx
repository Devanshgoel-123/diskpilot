import { FileStack, Files, FolderSearch, HardDrive, Recycle, ShieldCheck } from "lucide-react";

import { PageHeader } from "@/components/layout/PageHeader";
import { EmptyState } from "@/components/shared/EmptyState";
import { Button } from "@/components/ui/button";
import { Panel, PanelHeader } from "@/components/ui/panel";
import { Skeleton } from "@/components/ui/skeleton";
import { regenerableBytes, topConsumers } from "@/lib/analysis";
import { formatBytes, formatCount, formatPercent, tildify } from "@/lib/format";
import { useApp } from "@/lib/store";

import { CapacityPanel } from "./CapacityPanel";
import { CategoryBreakdown } from "./CategoryBreakdown";
import { LocationList } from "./LocationList";
import { ScanDetails } from "./ScanDetails";
import { StatTile } from "./StatTile";

export function OverviewPage() {
  const { disk, summary, env, scanning, scanRoot, startScan, navigate } = useApp();
  const home = env?.homeDir;

  const consumers = summary ? topConsumers(summary.detectedLocations, 8) : [];
  const regenerable = summary ? regenerableBytes(summary.categories) : 0;
  const worthReviewing = consumers.filter(
    (l) => l.classification.category === "cache" || l.classification.category === "buildArtifacts",
  );

  return (
    <>
      <PageHeader
        title="Overview"
        description={
          disk ? `${disk.fileSystem.toUpperCase()} volume at ${disk.mountPoint}` : undefined
        }
      />
      <div className="min-h-0 flex-1 overflow-y-auto">
        <div className="mx-auto max-w-[1180px] space-y-4 px-6 py-5">
          {disk ? (
            <CapacityPanel disk={disk} summary={summary} home={home} />
          ) : (
            <Skeleton className="h-[132px] w-full rounded-lg" />
          )}

          {!summary ? (
            <Panel>
              <EmptyState
                icon={scanning ? FolderSearch : ShieldCheck}
                title={scanning ? "Scanning…" : "Scan to see what is using your storage"}
                description={
                  scanning ? (
                    "Folder sizes, categories and the largest files will appear here when the scan finishes."
                  ) : (
                    <>
                      DiskPilot reads file sizes in{" "}
                      <span className="font-mono">{tildify(scanRoot ?? home ?? "~", home)}</span>{" "}
                      without modifying anything. Symbolic links are not followed and other volumes
                      are skipped.
                    </>
                  )
                }
                action={
                  scanning ? null : (
                    <div className="flex items-center gap-2">
                      <Button onClick={() => void startScan()}>Start scan</Button>
                      <Button variant="ghost" onClick={() => navigate("settings")}>
                        Choose another folder
                      </Button>
                    </div>
                  )
                }
              />
            </Panel>
          ) : (
            <>
              <div className="grid grid-cols-2 gap-4 lg:grid-cols-4">
                <StatTile
                  icon={HardDrive}
                  tint="var(--accent)"
                  label="Scanned"
                  value={formatBytes(summary.totalBytes)}
                  hint={
                    disk
                      ? `${formatPercent(summary.totalBytes, disk.usedBytes)} of used space`
                      : undefined
                  }
                />
                <StatTile
                  icon={Files}
                  tint="var(--cat-developer-tools)"
                  label="Files"
                  value={formatCount(summary.fileCount)}
                  hint={`${formatCount(summary.dirCount)} folders`}
                />
                <StatTile
                  icon={Recycle}
                  tint="var(--tier-safe)"
                  label="Potentially reclaimable"
                  value={formatBytes(regenerable)}
                  hint="Cache and build artifacts. An upper bound, not a promise: each item still needs review."
                />
                <StatTile
                  icon={FileStack}
                  tint="var(--cat-build-artifacts)"
                  label="Largest file"
                  value={formatBytes(summary.largestFiles[0]?.bytes ?? 0)}
                  hint={
                    <span className="block truncate font-mono">
                      {summary.largestFiles[0]?.name ?? "—"}
                    </span>
                  }
                />
              </div>

              <div className="grid grid-cols-1 gap-4 xl:grid-cols-5">
                <div className="xl:col-span-3">
                  <CategoryBreakdown categories={summary.categories} />
                </div>
                <Panel className="xl:col-span-2">
                  <PanelHeader
                    title="Largest storage consumers"
                    description="Specific items DiskPilot recognized. Click to explore."
                  />
                  {consumers.length ? (
                    <LocationList locations={consumers} />
                  ) : (
                    <p className="px-4 pb-4 text-xs text-muted-foreground">
                      No known developer or app storage was detected in this folder.
                    </p>
                  )}
                </Panel>
              </div>

              <div className="grid grid-cols-1 gap-4 xl:grid-cols-5">
                <Panel className="xl:col-span-3">
                  <PanelHeader
                    title="Worth reviewing"
                    description="Caches and build output found by this scan. Cleanup actions with previews arrive in the next release; nothing is deleted today."
                  />
                  {worthReviewing.length ? (
                    <LocationList locations={worthReviewing} />
                  ) : (
                    <p className="px-4 pb-4 text-xs text-muted-foreground">
                      No large caches or build artifacts among the top consumers.
                    </p>
                  )}
                </Panel>
                <div className="xl:col-span-2">
                  <ScanDetails summary={summary} home={home} />
                </div>
              </div>
            </>
          )}
        </div>
      </div>
    </>
  );
}
