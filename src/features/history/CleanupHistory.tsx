import { Box, CheckCircle2, ChevronDown, FileX, MinusCircle, XCircle } from "lucide-react";
import { useState } from "react";

import { EmptyState } from "@/components/shared/EmptyState";
import { Badge } from "@/components/ui/badge";
import { formatBytes, formatDate, tildify } from "@/lib/format";
import { useApp } from "@/lib/store";
import type { ActionStatus, CleanupRecord, ItemStatus } from "@/lib/types";
import { cn } from "@/lib/utils";

const STATUS_LABEL: Record<ActionStatus, string> = {
  succeeded: "Succeeded",
  partial: "Partly succeeded",
  failed: "Failed",
};

function ItemIcon({ status }: { status: ItemStatus }) {
  if (status === "succeeded")
    return <CheckCircle2 className="size-3.5 shrink-0 text-good" aria-label="Succeeded" />;
  if (status === "skipped")
    return <MinusCircle className="size-3.5 shrink-0 text-warning" aria-label="Skipped" />;
  return <XCircle className="size-3.5 shrink-0 text-critical" aria-label="Failed" />;
}

function Row({ record, home }: { record: CleanupRecord; home: string | undefined }) {
  const [open, setOpen] = useState(false);
  const Icon = record.kind === "docker" ? Box : FileX;
  return (
    <li className="border-b border-border last:border-0">
      <button
        type="button"
        onClick={() => setOpen((o) => !o)}
        aria-expanded={open}
        className="flex w-full items-center gap-3 px-4 py-2.5 text-left hover:bg-hover/60"
      >
        <Icon className="size-3.5 shrink-0 text-subtle-foreground" aria-hidden />
        <div className="min-w-0 flex-1">
          <p className="truncate text-[13px]">{record.title}</p>
          <p className="text-2xs text-subtle-foreground">{formatDate(record.startedAtMs)}</p>
        </div>
        <div className="tabular text-right text-xs">
          {record.bytesFreed > 0 ? <p>{formatBytes(record.bytesFreed)} freed</p> : null}
          {record.bytesTrashed > 0 ? (
            <p className="text-muted-foreground">{formatBytes(record.bytesTrashed)} to Trash</p>
          ) : null}
          {record.bytesFreed === 0 && record.bytesTrashed === 0 ? <p>—</p> : null}
        </div>
        <Badge
          className={cn(
            "w-28 justify-center",
            record.status === "succeeded" && "text-good",
            record.status === "partial" && "text-warning",
            record.status === "failed" && "text-critical",
          )}
        >
          {STATUS_LABEL[record.status]}
        </Badge>
        <ChevronDown
          className={cn(
            "size-3.5 text-subtle-foreground transition-transform",
            open && "rotate-180",
          )}
          aria-hidden
        />
      </button>
      {open ? (
        <ul className="space-y-1.5 px-4 pb-3 pl-10">
          {record.items.map((item, i) => (
            <li
              key={`${item.target}-${i}`}
              className="rounded-md border border-border bg-panel-raised px-3 py-2"
            >
              <p className="flex items-center gap-2 text-xs">
                <ItemIcon status={item.status} />
                <span className="selectable min-w-0 flex-1 truncate font-mono text-2xs">
                  {tildify(item.target, home)}
                </span>
                <span className="tabular shrink-0">{formatBytes(item.bytes)}</span>
              </p>
              <p className="selectable mt-1 pl-5 font-mono text-2xs break-all text-subtle-foreground">
                {tildify(item.operation, home)}
              </p>
              {item.error ? <p className="mt-1 pl-5 text-2xs text-critical">{item.error}</p> : null}
            </li>
          ))}
        </ul>
      ) : null}
    </li>
  );
}

export function CleanupHistory() {
  const cleanups = useApp((s) => s.cleanups);
  const home = useApp((s) => s.env?.homeDir);
  if (cleanups.length === 0) {
    return (
      <EmptyState
        icon={FileX}
        title="No cleanup actions yet"
        description="Every cleanup DiskPilot runs is recorded here: paths, exact operations, space reclaimed and any errors."
        className="py-8"
      />
    );
  }
  return (
    <ul>
      {cleanups.map((r) => (
        <Row key={r.id} record={r} home={home} />
      ))}
    </ul>
  );
}
