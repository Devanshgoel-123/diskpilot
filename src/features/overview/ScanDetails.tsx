import { AlertTriangle } from "lucide-react";
import { useState } from "react";

import { Button } from "@/components/ui/button";
import { Section } from "@/components/ui/section";
import { formatCount, formatDate, formatDuration, formatRelative, tildify } from "@/lib/format";
import type { ScanSummary } from "@/lib/types";
import { isWindows } from "@/lib/platform";

function Row({ label, value }: { label: string; value: React.ReactNode }) {
  return (
    <div className="flex items-baseline justify-between gap-4 py-1.5">
      <dt className="text-[13px] text-muted-foreground">{label}</dt>
      <dd className="tabular min-w-0 truncate text-right text-[13px]">{value}</dd>
    </div>
  );
}

export function ScanDetails({ summary, home }: { summary: ScanSummary; home: string | undefined }) {
  const [showIssues, setShowIssues] = useState(false);
  return (
    <Section title="Last scan" description={formatRelative(summary.finishedAtMs)}>
      <dl className="divide-y divide-border">
        <Row
          label="Folder"
          value={<span className="font-mono">{tildify(summary.root, home)}</span>}
        />
        <Row label="Finished" value={formatDate(summary.finishedAtMs)} />
        <Row label="Duration" value={formatDuration(summary.durationMs)} />
        <Row label="Files" value={formatCount(summary.fileCount)} />
        <Row label="Folders" value={formatCount(summary.dirCount)} />
        <Row
          label={
            isWindows() ? "Links and junctions (not followed)" : "Symbolic links (not followed)"
          }
          value={formatCount(summary.symlinkCount)}
        />
        {summary.skippedVolumes > 0 ? (
          <Row
            label={isWindows() ? "Other drives skipped" : "Other volumes skipped"}
            value={formatCount(summary.skippedVolumes)}
          />
        ) : null}
        <Row
          label="Unreadable items"
          value={
            summary.errorCount > 0 ? (
              <span className="inline-flex items-center gap-1 text-warning">
                <AlertTriangle className="size-3" aria-hidden />
                {formatCount(summary.errorCount)}
              </span>
            ) : (
              "0"
            )
          }
        />
      </dl>
      {summary.errorCount > 0 ? (
        <div className="border-t border-border py-2.5">
          <p className="text-2xs leading-snug text-subtle-foreground">
            {isWindows()
              ? "Usually folders only administrators or the system can read. Their sizes are not included."
              : "Usually folders protected by macOS privacy settings. Their sizes are not included."}
          </p>
          <Button
            variant="ghost"
            size="sm"
            className="mt-1 -ml-2"
            onClick={() => setShowIssues((v) => !v)}
          >
            {showIssues ? "Hide" : "Show"} details
          </Button>
          {showIssues ? (
            <ul className="selectable mt-1 max-h-48 space-y-1 overflow-y-auto">
              {summary.errorSamples.map((e, i) => (
                <li key={`${e.path}-${i}`} className="text-2xs">
                  <p className="truncate font-mono text-muted-foreground" title={e.path}>
                    {tildify(e.path, home)}
                  </p>
                  <p className="text-subtle-foreground">{e.message}</p>
                </li>
              ))}
            </ul>
          ) : null}
        </div>
      ) : null}
    </Section>
  );
}
