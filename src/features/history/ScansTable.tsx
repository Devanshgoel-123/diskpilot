import { Fragment } from "react";

import { formatBytes, formatCount, formatDuration, tildify } from "@/lib/format";
import type { ScanRecord } from "@/lib/types";

import { dayLabel, formatChange, sizeChanges, timeOfDay } from "./scans";

/** Scans grouped by day, newest first. */
export function ScansTable({ records, home }: { records: ScanRecord[]; home?: string }) {
  const changes = sizeChanges(records);
  const days: { label: string; rows: ScanRecord[] }[] = [];
  for (const r of [...records].sort((a, b) => b.startedAtMs - a.startedAtMs)) {
    const label = dayLabel(r.startedAtMs);
    const group = days.at(-1);
    if (group?.label === label) group.rows.push(r);
    else days.push({ label, rows: [r] });
  }

  return (
    <table className="w-full table-fixed text-[13px]">
      <thead>
        <tr className="border-y border-border text-left text-xs text-muted-foreground">
          <th className="w-28 px-4 py-2 font-normal">Time</th>
          <th className="px-2 py-2 font-normal">Folder</th>
          <th className="w-24 px-2 py-2 text-right font-normal">Size</th>
          <th
            className="w-24 px-2 py-2 text-right font-normal"
            title="Since the previous scan of the same folder"
          >
            Change
          </th>
          <th className="w-24 px-2 py-2 text-right font-normal">Files</th>
          <th className="w-20 px-2 py-2 pr-4 text-right font-normal">Took</th>
        </tr>
      </thead>
      <tbody>
        {days.map((day) => (
          <Fragment key={day.label}>
            <tr>
              <th
                colSpan={6}
                scope="colgroup"
                className="px-4 pt-4 pb-1 text-left text-xs font-medium text-muted-foreground"
              >
                {day.label}
              </th>
            </tr>
            {day.rows.map((r) => {
              const change = changes.get(r.id);
              return (
                <tr key={r.id} className="h-8 border-b border-border/60 last:border-0">
                  <td className="tabular px-4">{timeOfDay(r.startedAtMs)}</td>
                  <td className="truncate px-2 font-mono text-2xs text-muted-foreground">
                    {tildify(r.root, home)}
                  </td>
                  {r.status === "completed" ? (
                    <>
                      <td className="tabular px-2 text-right">{formatBytes(r.totalBytes)}</td>
                      <td className="tabular px-2 text-right text-xs text-muted-foreground">
                        {change === undefined ? "" : formatChange(change)}
                      </td>
                      <td className="tabular px-2 text-right text-xs text-muted-foreground">
                        {formatCount(r.fileCount)}
                      </td>
                    </>
                  ) : (
                    <td
                      colSpan={3}
                      className={`px-2 text-right text-xs ${r.status === "failed" ? "text-critical" : "text-muted-foreground"}`}
                    >
                      {r.status === "failed" ? "Failed" : "Cancelled"}
                    </td>
                  )}
                  <td className="tabular px-2 pr-4 text-right text-xs text-muted-foreground">
                    {formatDuration(r.durationMs)}
                  </td>
                </tr>
              );
            })}
          </Fragment>
        ))}
      </tbody>
    </table>
  );
}
