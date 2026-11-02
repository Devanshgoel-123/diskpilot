import { FolderOpen, X } from "lucide-react";
import { type ReactNode, useEffect, useState } from "react";

import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { api, errorMessage } from "@/lib/api";
import { formatBytes, formatCount, formatDate, tildify } from "@/lib/format";
import { useApp } from "@/lib/store";
import type { ItemDetails } from "@/lib/types";

import { ClassificationCard } from "./ClassificationCard";

function Row({ label, value }: { label: string; value: ReactNode }) {
  return (
    <div className="flex items-baseline justify-between gap-3 py-1">
      <dt className="shrink-0 text-xs text-muted-foreground">{label}</dt>
      <dd className="selectable tabular min-w-0 text-right text-xs">{value}</dd>
    </div>
  );
}

/** Live metadata for one item, with its classification and safe actions. */
export function DetailsPanel({ path, onClose }: { path: string; onClose: () => void }) {
  // Remount per path so state starts fresh without resetting it in an effect.
  return <DetailsBody key={path} path={path} onClose={onClose} />;
}

function DetailsBody({ path, onClose }: { path: string; onClose: () => void }) {
  const home = useApp((s) => s.env?.homeDir);
  const [details, setDetails] = useState<ItemDetails | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let live = true;
    api
      .getItemDetails(path)
      .then((d) => live && setDetails(d))
      .catch((e: unknown) => live && setError(errorMessage(e)));
    return () => {
      live = false;
    };
  }, [path]);

  const reveal = () => {
    api.revealInFinder(path).catch((e: unknown) => setError(errorMessage(e)));
  };

  return (
    <aside className="flex w-80 shrink-0 flex-col border-l border-border bg-panel">
      <div className="flex items-center justify-between gap-2 border-b border-border px-4 py-2.5">
        <p className="text-xs font-medium text-muted-foreground">Details</p>
        <Button variant="ghost" size="icon" onClick={onClose} aria-label="Close details">
          <X />
        </Button>
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto px-4 py-3">
        <p className="selectable text-[13px] font-medium break-all">
          {details?.name ?? path.split("/").pop()}
        </p>
        <p className="selectable mt-0.5 font-mono text-2xs break-all text-subtle-foreground">
          {tildify(path, home)}
        </p>

        {error ? <p className="mt-3 text-xs text-critical">{error}</p> : null}

        {details ? (
          <>
            <dl className="mt-3 divide-y divide-border">
              <Row label="Size on disk" value={formatBytes(details.bytes)} />
              {details.logicalBytes != null ? (
                <Row label="Logical size" value={formatBytes(details.logicalBytes)} />
              ) : null}
              {details.fileCount != null ? (
                <Row label="Files" value={formatCount(details.fileCount)} />
              ) : null}
              <Row label="Kind" value={details.kind} />
              <Row label="Modified" value={formatDate(details.modifiedMs)} />
              <Row label="Created" value={formatDate(details.createdMs)} />
              <Row label="Last opened" value={formatDate(details.accessedMs)} />
              {details.readonly ? <Row label="Permissions" value="Read-only" /> : null}
              {details.symlinkTarget ? (
                <Row
                  label="Links to"
                  value={<span className="font-mono break-all">{details.symlinkTarget}</span>}
                />
              ) : null}
            </dl>
            <div className="mt-4">
              <ClassificationCard classification={details.classification} itemPath={path} />
            </div>
          </>
        ) : !error ? (
          <div className="mt-4 space-y-2">
            <Skeleton className="h-4 w-full" />
            <Skeleton className="h-4 w-3/4" />
            <Skeleton className="h-24 w-full" />
          </div>
        ) : null}
      </div>
      <div className="border-t border-border p-3">
        <Button variant="secondary" className="w-full" onClick={reveal}>
          <FolderOpen />
          Reveal in Finder
        </Button>
      </div>
    </aside>
  );
}
