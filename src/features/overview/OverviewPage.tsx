import { FolderSearch, ScanSearch } from "lucide-react";
import { useMemo } from "react";

import { PageHeader } from "@/components/layout/PageHeader";
import { EmptyState } from "@/components/shared/EmptyState";
import { ResultsAge } from "@/components/shared/ResultsAge";
import { Button } from "@/components/ui/button";
import { Section } from "@/components/ui/section";
import { Skeleton } from "@/components/ui/skeleton";
import { pickStartHere, topConsumers } from "@/lib/analysis";
import { formatBytes, formatCount, tildify } from "@/lib/format";
import { useApp } from "@/lib/store";

import { CategoryList } from "./CategoryList";
import { LocationList } from "./LocationList";
import { ScanDetails } from "./ScanDetails";
import { StartHere } from "./StartHere";
import { StorageStrip } from "./StorageStrip";

function RowsSkeleton({ rows }: { rows: number }) {
  return (
    <div className="space-y-3 py-1" aria-hidden>
      {Array.from({ length: rows }, (_, i) => (
        <div key={i} className="flex items-center gap-3">
          <Skeleton className="size-5" />
          <div className="flex-1 space-y-1.5">
            <Skeleton className="h-3 w-1/2" />
            <Skeleton className="h-2.5 w-1/3" />
          </div>
          <Skeleton className="h-3 w-14" />
        </div>
      ))}
    </div>
  );
}

export function OverviewPage() {
  const { disk, summary, env, scanning, progress, scanRoot, startScan, navigate, recommendations } =
    useApp();
  const home = env?.homeDir;

  const consumers = useMemo(
    () => (summary ? topConsumers(summary.detectedLocations, 7) : []),
    [summary],
  );
  const startHere = useMemo(() => pickStartHere(recommendations), [recommendations]);
  const actionable = recommendations.filter((r) => r.methods.length > 0).length;

  return (
    <>
      <PageHeader title="Overview" />
      <div className="min-h-0 flex-1 overflow-y-auto">
        <div className="mx-auto max-w-[1120px] space-y-8 px-6 pt-6 pb-10">
          <ResultsAge />
          {disk ? (
            <StorageStrip
              disk={disk}
              summary={summary}
              recommendations={recommendations}
              onReview={() => navigate("recommendations")}
            />
          ) : (
            <div className="space-y-3" aria-hidden>
              <Skeleton className="h-8 w-64" />
              <Skeleton className="h-6 w-full" />
              <Skeleton className="h-3 w-2/3" />
            </div>
          )}

          {!summary && !scanning ? (
            <div className="border-t border-border pt-4">
              <EmptyState
                icon={ScanSearch}
                title="Scan to see where your space goes"
                description={
                  <>
                    DiskPilot reads file sizes in{" "}
                    <span className="font-mono">{tildify(scanRoot ?? home ?? "~", home)}</span> and
                    changes nothing. Symbolic links are not followed and other volumes are skipped.
                  </>
                }
                action={
                  <div className="flex items-center gap-2">
                    <Button size="lg" onClick={() => void startScan()}>
                      Scan home folder
                    </Button>
                    <Button variant="ghost" size="lg" onClick={() => navigate("settings")}>
                      Choose another folder
                    </Button>
                  </div>
                }
              />
            </div>
          ) : null}

          {!summary && scanning ? (
            <div className="grid grid-cols-1 gap-x-12 gap-y-8 border-t border-border pt-6 lg:grid-cols-5">
              <Section
                className="lg:col-span-3"
                title="Scanning"
                description={
                  progress
                    ? `${formatCount(progress.filesScanned)} files, ${formatBytes(progress.bytesScanned)} so far`
                    : "Starting"
                }
              >
                <RowsSkeleton rows={4} />
              </Section>
              <Section className="lg:col-span-2" title="Where your space goes">
                <RowsSkeleton rows={4} />
              </Section>
              <p className="flex items-center gap-2 text-xs text-muted-foreground lg:col-span-5">
                <FolderSearch className="size-3.5" aria-hidden />
                Read-only. You can keep using your Mac while DiskPilot scans.
              </p>
            </div>
          ) : null}

          {summary ? (
            <div className="grid grid-cols-1 gap-x-12 gap-y-10 border-t border-border pt-6 lg:grid-cols-5">
              <Section
                className="lg:col-span-3"
                title="Start here"
                description="The largest items DiskPilot can explain and clean up."
                actions={
                  actionable > 0 ? (
                    <Button variant="ghost" size="sm" onClick={() => navigate("recommendations")}>
                      All {actionable} in Cleanup
                    </Button>
                  ) : null
                }
              >
                <StartHere items={startHere} />
              </Section>

              <Section
                className="lg:col-span-2"
                title="Where your space goes"
                description={`${formatBytes(summary.totalBytes)} in ${tildify(summary.root, home)}`}
              >
                <CategoryList categories={summary.categories} />
              </Section>

              <Section
                className="lg:col-span-3"
                title="Largest items"
                description="Folders DiskPilot recognized. Open one to see what is inside."
              >
                {consumers.length ? (
                  <LocationList locations={consumers} />
                ) : (
                  <p className="py-3 text-[13px] text-muted-foreground">
                    No known app or developer storage in this folder.
                  </p>
                )}
              </Section>

              <div className="lg:col-span-2">
                <ScanDetails summary={summary} home={home} />
              </div>
            </div>
          ) : null}
        </div>
      </div>
    </>
  );
}
