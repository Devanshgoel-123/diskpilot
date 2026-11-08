import { Container, HardDrive, Loader2, RefreshCw, SquareTerminal } from "lucide-react";
import { useEffect, useState } from "react";

import { Button } from "@/components/ui/button";
import { Section } from "@/components/ui/section";
import { formatBytes, tildify } from "@/lib/format";
import { useApp } from "@/lib/store";
import type { CompactMethod, VirtualDisk } from "@/lib/types";

import { CompactDialog } from "./CompactDialog";
import { KIND_LABEL, WORTH_SHOWING_BYTES, canMakeSparse, useWsl } from "./wslStore";

/** One open compaction: the disk and what to do with it. */
type Compacting = { disk: VirtualDisk; method: CompactMethod };

function runningLabel(running: boolean | null): string | null {
  if (running === null) return null;
  return running ? "Running" : "Stopped";
}

function DiskRow({
  disk,
  onCompact,
}: {
  disk: VirtualDisk;
  onCompact: (method: CompactMethod) => void;
}) {
  const home = useApp((s) => s.env?.homeDir);
  const Icon = disk.kind === "distro" ? SquareTerminal : Container;
  const state = runningLabel(disk.running);
  const facts = [KIND_LABEL[disk.kind], disk.isDefault ? "Default" : null, state].filter(Boolean);
  return (
    <li className="flex items-start gap-3 border-b border-border px-2 py-2.5 last:border-b-0">
      <Icon
        className="mt-0.5 size-4 shrink-0 text-muted-foreground"
        strokeWidth={1.75}
        aria-hidden
      />
      <div className="min-w-0 flex-1">
        <p className="truncate text-[13px] font-medium">{disk.name}</p>
        <p className="text-2xs text-subtle-foreground">
          {facts.join(" · ")}
          {disk.sparse ? " · Shrinks by itself" : ""}
        </p>
        <p
          className="selectable mt-0.5 truncate font-mono text-2xs text-subtle-foreground"
          title={disk.path}
        >
          {tildify(disk.path, home)}
        </p>
      </div>
      <div className="w-48 shrink-0 text-right">
        <p className="tabular text-[13px]">{formatBytes(disk.allocatedBytes)} on disk</p>
        <p className="text-2xs text-subtle-foreground">
          {disk.usedBytes != null ? `${formatBytes(disk.usedBytes)} in use` : disk.usageNote}
        </p>
        {disk.reclaimableBytes ? (
          <p className="tabular text-2xs text-accent">
            About {formatBytes(disk.reclaimableBytes)} can be returned
          </p>
        ) : null}
      </div>
      <div className="flex shrink-0 flex-col items-end gap-1">
        <Button variant="secondary" size="sm" onClick={() => onCompact("compact")}>
          Compact…
        </Button>
        {canMakeSparse(disk) ? (
          <Button variant="ghost" size="sm" onClick={() => onCompact("sparse")}>
            Make it shrink by itself…
          </Button>
        ) : null}
      </div>
    </li>
  );
}

/**
 * Developer Tools, Windows: WSL distributions and Docker Desktop keep Linux
 * files in .vhdx disks that grow but do not shrink on their own. Lists them
 * with the space each could return, and compacts one after review.
 */
export function VirtualDisksSection() {
  const { overview, loading, error, load, ensure } = useWsl();
  const [compacting, setCompacting] = useState<Compacting | null>(null);

  useEffect(() => ensure(), [ensure]);

  if (!overview && !error) {
    return loading ? (
      <Section title="Virtual disks">
        <p className="flex items-center gap-2 text-xs text-muted-foreground">
          <Loader2 className="size-3.5 animate-spin" aria-hidden /> Looking for WSL and Docker
          disks…
        </p>
      </Section>
    ) : null;
  }
  if (overview && !overview.supported) return null;

  const disks = overview?.disks ?? [];
  return (
    <Section
      title="Virtual disks"
      description="WSL and Docker Desktop keep Linux files in .vhdx disks. They grow as Linux writes but do not give space back on their own. Compacting returns the unused space to Windows."
      actions={
        <Button variant="ghost" size="sm" onClick={() => void load()} disabled={loading}>
          {loading ? <Loader2 className="animate-spin" /> : <RefreshCw />}
          Check again
        </Button>
      }
    >
      {error ? <p className="text-xs text-critical">{error}</p> : null}
      {disks.length ? (
        <>
          <ul className="border-t border-border">
            {disks.map((d) => (
              <DiskRow
                key={d.id}
                disk={d}
                onCompact={(method) => setCompacting({ disk: d, method })}
              />
            ))}
          </ul>
          {overview && overview.disks.length > 1 ? (
            <p className="tabular mt-2 px-2 text-2xs text-subtle-foreground">
              {formatBytes(overview.totalBytes)} on disk in total
              {overview.reclaimableBytes
                ? `, about ${formatBytes(overview.reclaimableBytes)} can be returned`
                : ""}
              .
            </p>
          ) : null}
        </>
      ) : overview ? (
        <p className="flex items-center gap-2 text-xs text-muted-foreground">
          <HardDrive className="size-3.5 shrink-0" aria-hidden />
          {overview.message ?? "No WSL or Docker Desktop disks were found on this PC."}
        </p>
      ) : null}
      {overview?.message && disks.length ? (
        <p className="mt-1 px-2 text-2xs text-subtle-foreground">{overview.message}</p>
      ) : null}
      {compacting ? (
        <CompactDialog
          disk={compacting.disk}
          method={compacting.method}
          onClose={() => setCompacting(null)}
          onDone={() => void load()}
        />
      ) : null}
    </Section>
  );
}

/**
 * Overview, Windows: a short note when WSL or Docker disks hold at least
 * 1 GB they could give back, linking to the full list in Developer Tools.
 */
export function VirtualDisksSummary() {
  const { overview, ensure } = useWsl();
  const navigate = useApp((s) => s.navigate);

  useEffect(() => ensure(), [ensure]);

  if (!overview?.supported || overview.reclaimableBytes < WORTH_SHOWING_BYTES) return null;
  const count = overview.disks.filter((d) => (d.reclaimableBytes ?? 0) > 0).length;
  return (
    <div
      role="status"
      className="flex items-center justify-between gap-4 rounded-[10px] border border-border bg-panel-raised px-3 py-2.5"
    >
      <div className="flex min-w-0 gap-2 text-xs">
        <SquareTerminal className="mt-0.5 size-3.5 shrink-0 text-muted-foreground" aria-hidden />
        <div>
          <p className="text-foreground">
            WSL and Docker disks could give back about{" "}
            <span className="tabular font-medium">{formatBytes(overview.reclaimableBytes)}</span>.
          </p>
          <p className="mt-0.5 text-muted-foreground">
            {count === 1 ? "One virtual disk has" : `${count} virtual disks have`} space Linux no
            longer uses. Compacting returns it to Windows.
          </p>
        </div>
      </div>
      <Button size="sm" variant="secondary" onClick={() => navigate("developer")}>
        Review
      </Button>
    </div>
  );
}
