import { Lock, Play, Search, Square, Trash2 } from "lucide-react";
import type { ReactNode } from "react";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { formatBytes, formatRelative } from "@/lib/format";
import type {
  BuildCacheEntry,
  DockerAction,
  DockerContainer,
  DockerImage,
  DockerVolume,
} from "@/lib/types";
import { cn } from "@/lib/utils";

import type { InspectTarget } from "./InspectPanel";

interface Handlers {
  onAction: (a: DockerAction) => void;
  onInspect: (t: InspectTarget) => void;
}

function IconAction({
  label,
  disabledReason,
  onClick,
  danger,
  children,
}: {
  label: string;
  disabledReason?: string | null;
  onClick: () => void;
  danger?: boolean;
  children: ReactNode;
}) {
  const button = (
    <Button
      variant="ghost"
      size="icon"
      aria-label={label}
      disabled={!!disabledReason}
      onClick={onClick}
      className={cn(danger && "hover:text-critical")}
    >
      {children}
    </Button>
  );
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        {/* Wrapper keeps the tooltip working on disabled buttons. */}
        <span>{button}</span>
      </TooltipTrigger>
      <TooltipContent>{disabledReason ?? label}</TooltipContent>
    </Tooltip>
  );
}

const th = "px-2 py-2 text-left font-normal";

export function ContainersTable({
  containers,
  onAction,
  onInspect,
}: Handlers & { containers: DockerContainer[] }) {
  if (!containers.length) return <Empty text="No containers." />;
  return (
    <table className="w-full table-fixed text-[13px]">
      <thead>
        <tr className="border-b border-border text-xs text-muted-foreground">
          <th className={cn(th, "pl-4")}>Name</th>
          <th className={th}>Image</th>
          <th className={cn(th, "w-48")}>Status</th>
          <th className={cn(th, "w-24 text-right")}>Size</th>
          <th className={cn(th, "w-28")}>Created</th>
          <th className={cn(th, "w-28 pr-4 text-right")}>Actions</th>
        </tr>
      </thead>
      <tbody>
        {containers.map((c) => (
          <tr key={c.id} className="h-10 border-b border-border/60 last:border-0">
            <td className="truncate pr-2 pl-4">
              <span className="flex min-w-0 items-center gap-2">
                <span
                  className={cn(
                    "size-1.5 shrink-0 rounded-full",
                    c.running ? "bg-good" : "bg-subtle-foreground",
                  )}
                  aria-hidden
                />
                <span className="truncate">{c.name}</span>
                {c.composeProject ? <Badge variant="outline">{c.composeProject}</Badge> : null}
              </span>
            </td>
            <td className="truncate px-2 font-mono text-2xs text-muted-foreground">{c.image}</td>
            <td className="truncate px-2 text-xs text-muted-foreground">{c.status}</td>
            <td className="tabular px-2 text-right">{formatBytes(c.sizeBytes)}</td>
            <td className="px-2 text-xs text-muted-foreground">{formatRelative(c.createdAtMs)}</td>
            <td className="px-2 pr-4">
              <div className="flex justify-end">
                <IconAction
                  label="Inspect"
                  onClick={() => onInspect({ kind: "container", id: c.id, title: c.name })}
                >
                  <Search />
                </IconAction>
                {c.running ? (
                  <IconAction
                    label="Stop"
                    onClick={() => onAction({ type: "stopContainer", id: c.id })}
                  >
                    <Square />
                  </IconAction>
                ) : (
                  <IconAction
                    label="Start"
                    onClick={() => onAction({ type: "startContainer", id: c.id })}
                  >
                    <Play />
                  </IconAction>
                )}
                <IconAction
                  label="Remove…"
                  danger
                  disabledReason={c.running ? "Stop the container before removing it" : null}
                  onClick={() => onAction({ type: "removeContainer", id: c.id })}
                >
                  <Trash2 />
                </IconAction>
              </div>
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

export function ImagesTable({ images, onAction, onInspect }: Handlers & { images: DockerImage[] }) {
  if (!images.length) return <Empty text="No images." />;
  return (
    <table className="w-full table-fixed text-[13px]">
      <thead>
        <tr className="border-b border-border text-xs text-muted-foreground">
          <th className={cn(th, "pl-4")}>Image</th>
          <th className={cn(th, "w-28")}>ID</th>
          <th className={th}>Used by</th>
          <th className={cn(th, "w-28 text-right")}>Unique</th>
          <th className={cn(th, "w-24 text-right")}>Shared</th>
          <th className={cn(th, "w-28")}>Created</th>
          <th className={cn(th, "w-20 pr-4 text-right")}>Actions</th>
        </tr>
      </thead>
      <tbody>
        {images.map((i) => {
          const name = i.dangling ? "<untagged>" : `${i.repository}:${i.tag}`;
          return (
            <tr key={i.id} className="h-10 border-b border-border/60 last:border-0">
              <td className="truncate pr-2 pl-4">
                <span className="flex min-w-0 items-center gap-2">
                  <span className="truncate">{name}</span>
                  {i.dangling ? <Badge variant="outline">dangling</Badge> : null}
                </span>
              </td>
              <td className="px-2 font-mono text-2xs text-muted-foreground">{i.shortId}</td>
              <td className="truncate px-2 text-xs text-muted-foreground">
                {i.usedBy.length ? (
                  i.usedBy.join(", ")
                ) : (
                  <span className="text-subtle-foreground">No containers</span>
                )}
              </td>
              <td className="tabular px-2 text-right">{formatBytes(i.uniqueBytes)}</td>
              <td className="tabular px-2 text-right text-xs text-muted-foreground">
                {formatBytes(i.sharedBytes)}
              </td>
              <td className="px-2 text-xs text-muted-foreground">
                {formatRelative(i.createdAtMs)}
              </td>
              <td className="px-2 pr-4">
                <div className="flex justify-end">
                  <IconAction
                    label="Inspect"
                    onClick={() => onInspect({ kind: "image", id: i.id, title: name })}
                  >
                    <Search />
                  </IconAction>
                  <IconAction
                    label="Remove…"
                    danger
                    disabledReason={
                      i.usedBy.length
                        ? `Used by ${i.usedBy.join(", ")}. Remove those containers first.`
                        : null
                    }
                    onClick={() => onAction({ type: "removeImage", id: i.id })}
                  >
                    <Trash2 />
                  </IconAction>
                </div>
              </td>
            </tr>
          );
        })}
      </tbody>
    </table>
  );
}

export function VolumesTable({
  volumes,
  onInspect,
}: Omit<Handlers, "onAction"> & { volumes: DockerVolume[] }) {
  if (!volumes.length) return <Empty text="No volumes." />;
  return (
    <table className="w-full table-fixed text-[13px]">
      <thead>
        <tr className="border-b border-border text-xs text-muted-foreground">
          <th className={cn(th, "pl-4")}>Volume</th>
          <th className={cn(th, "w-36")}>Project</th>
          <th className={th}>Used by</th>
          <th className={cn(th, "w-24 text-right")}>Size</th>
          <th className={cn(th, "w-20 pr-4 text-right")}>Actions</th>
        </tr>
      </thead>
      <tbody>
        {volumes.map((v) => (
          <tr key={v.name} className="h-10 border-b border-border/60 last:border-0">
            <td className="truncate pr-2 pl-4">
              <span className="flex min-w-0 items-center gap-2">
                <Lock
                  className="size-3 shrink-0 text-subtle-foreground"
                  aria-label="Never removed by DiskPilot"
                />
                <span className="truncate font-mono text-xs">{v.name}</span>
              </span>
            </td>
            <td className="truncate px-2 text-xs text-muted-foreground">
              {v.composeProject ?? "—"}
            </td>
            <td className="truncate px-2 text-xs text-muted-foreground">
              {v.usedBy.length ? (
                v.usedBy.join(", ")
              ) : (
                <span className="text-warning">Not attached to a container</span>
              )}
            </td>
            <td className="tabular px-2 text-right">
              {v.sizeBytes == null ? "—" : formatBytes(v.sizeBytes)}
            </td>
            <td className="px-2 pr-4">
              <div className="flex justify-end">
                <IconAction
                  label="Inspect"
                  onClick={() => onInspect({ kind: "volume", id: v.name, title: v.name })}
                >
                  <Search />
                </IconAction>
              </div>
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

export function BuildCacheTable({ entries }: { entries: BuildCacheEntry[] }) {
  if (!entries.length) return <Empty text="No build cache." />;
  return (
    <table className="w-full table-fixed text-[13px]">
      <thead>
        <tr className="border-b border-border text-xs text-muted-foreground">
          <th className={cn(th, "pl-4")}>Step</th>
          <th className={cn(th, "w-32")}>Type</th>
          <th className={cn(th, "w-32")}>Last used</th>
          <th className={cn(th, "w-24")}>State</th>
          <th className={cn(th, "w-24 pr-4 text-right")}>Size</th>
        </tr>
      </thead>
      <tbody>
        {entries.map((b) => (
          <tr key={b.id} className="h-9 border-b border-border/60 last:border-0">
            <td className="truncate pr-2 pl-4 font-mono text-2xs" title={b.description}>
              {b.description || b.id}
            </td>
            <td className="truncate px-2 text-xs text-muted-foreground">{b.cacheType}</td>
            <td className="truncate px-2 text-xs text-muted-foreground">{b.lastUsed}</td>
            <td className="px-2 text-xs">
              {b.inUse ? (
                <span className="text-good">In use</span>
              ) : (
                <span className="text-muted-foreground">Unused</span>
              )}
            </td>
            <td className="tabular px-2 pr-4 text-right">{formatBytes(b.sizeBytes)}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

function Empty({ text }: { text: string }) {
  return <p className="px-4 py-6 text-xs text-muted-foreground">{text}</p>;
}
