import { PageHeader } from "@/components/layout/PageHeader";
import { RoadmapNote } from "@/components/shared/RoadmapNote";
import { Panel, PanelHeader } from "@/components/ui/panel";
import { groupByTool } from "@/lib/analysis";
import { formatBytes } from "@/lib/format";
import { useApp } from "@/lib/store";

import { LocationList } from "../overview/LocationList";

export function DockerPage() {
  const summary = useApp((s) => s.summary);
  const docker = summary
    ? groupByTool(summary.detectedLocations).find((g) => g.id === "docker")
    : null;

  return (
    <>
      <PageHeader title="Docker" description="Docker disk usage and safe resource management." />
      <div className="min-h-0 flex-1 overflow-y-auto">
        <div className="mx-auto max-w-[920px] space-y-4 px-6 py-5">
          <Panel>
            <PanelHeader
              title="Docker storage on disk"
              description="Found by the filesystem scan. This is the size of Docker's VM disk files, not a breakdown of images, containers or volumes."
              actions={
                docker && docker.bytes > 0 ? (
                  <span className="tabular text-[13px] font-medium">
                    {formatBytes(docker.bytes)}
                  </span>
                ) : null
              }
            />
            {docker && docker.locations.length > 0 ? (
              <LocationList locations={docker.locations} />
            ) : (
              <p className="text-muted-foreground px-4 pb-4 text-xs">
                {summary
                  ? "No Docker Desktop, OrbStack or Colima data was found in the scanned folder."
                  : "Run a scan to see how much space Docker's data uses."}
              </p>
            )}
          </Panel>
          <RoadmapNote phase="Phase 3" title="Docker Manager is not built yet">
            DiskPilot does not talk to Docker yet. Planned: images, containers, volumes and build
            cache via the Docker CLI; start/stop and inspect; image dependency explanations; and
            previewed, item-by-item removal. Volumes will never be deleted automatically, stopped
            containers will never be assumed unnecessary, and no broad <code>prune</code> will run
            without first showing exactly what it affects.
          </RoadmapNote>
        </div>
      </div>
    </>
  );
}
