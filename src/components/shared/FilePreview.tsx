import { ChevronLeft, ChevronRight, File, FolderOpen, Loader2 } from "lucide-react";
import { useEffect, useState } from "react";

import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "@/components/ui/dialog";
import { api } from "@/lib/api";
import { formatBytes, formatRelative, tildify } from "@/lib/format";
import { useApp } from "@/lib/store";
import { cachedThumbnail, loadThumbnail } from "@/lib/thumbnails";
import { basename, parentOf } from "@/lib/paths";
import { words } from "@/lib/platform";

export interface PreviewFile {
  path: string;
  bytes?: number;
  modifiedMs?: number | null;
  /** Shown under the name, e.g. "Keep" or "Will move to Trash". */
  note?: string;
}

const LARGE = 512;

const nameOf = basename;
const folderOf = parentOf;

/**
 * A large preview of one file at a time, like Quick Look in Finder: Space or
 * Escape closes it, the arrow keys move through the list. Read-only.
 */
export function FilePreview({
  files,
  index,
  onIndexChange,
  onClose,
}: {
  files: PreviewFile[];
  index: number | null;
  onIndexChange: (index: number) => void;
  onClose: () => void;
}) {
  const home = useApp((s) => s.env?.homeDir);
  const at = index ?? 0;
  const file = index === null ? null : (files[index] ?? null);
  const [loaded, setLoaded] = useState<{ path: string; url: string | null } | null>(null);
  const cached = file ? cachedThumbnail(file.path, LARGE) : undefined;
  const url = cached !== undefined ? cached : loaded?.path === file?.path ? loaded?.url : undefined;

  useEffect(() => {
    if (!file) return;
    let live = true;
    void loadThumbnail(file.path, LARGE).then(
      (u) => live && setLoaded({ path: file.path, url: u }),
    );
    // Warm the neighbours so the arrow keys feel instant.
    for (const n of [at - 1, at + 1]) {
      const f = files[n];
      if (f) void loadThumbnail(f.path, LARGE);
    }
    return () => {
      live = false;
    };
  }, [file, files, at]);

  const move = (step: number) => {
    if (index === null || files.length === 0) return;
    onIndexChange((index + step + files.length) % files.length);
  };

  return (
    <Dialog open={file !== null} onOpenChange={(o) => !o && onClose()}>
      <DialogContent
        className="w-[min(760px,calc(100vw-32px))]"
        onKeyDown={(e) => {
          if (e.key === " ") {
            e.preventDefault();
            onClose();
          } else if (e.key === "ArrowRight" || e.key === "ArrowDown") {
            e.preventDefault();
            move(1);
          } else if (e.key === "ArrowLeft" || e.key === "ArrowUp") {
            e.preventDefault();
            move(-1);
          }
        }}
      >
        {file ? (
          <>
            <div className="flex h-[min(520px,60vh)] items-center justify-center rounded-t-lg bg-track">
              {url ? (
                <img
                  src={url}
                  alt={`Preview of ${nameOf(file.path)}`}
                  className="max-h-full max-w-full object-contain"
                  draggable={false}
                />
              ) : url === null ? (
                <div className="flex flex-col items-center gap-2 text-sm text-muted-foreground">
                  <File className="size-10 text-subtle-foreground" aria-hidden />
                  {words().os} has no preview for this kind of file.
                </div>
              ) : (
                <Loader2
                  className="size-5 animate-spin text-subtle-foreground"
                  aria-label="Loading preview"
                />
              )}
            </div>
            <div className="flex items-start gap-3 px-4 py-3">
              <div className="min-w-0 flex-1">
                <DialogTitle className="truncate text-[13px] font-medium">
                  {nameOf(file.path)}
                </DialogTitle>
                <DialogDescription className="selectable truncate font-mono text-2xs text-muted-foreground">
                  {tildify(folderOf(file.path), home)}
                </DialogDescription>
                <p className="tabular mt-1 text-2xs text-subtle-foreground">
                  {[
                    file.bytes !== undefined ? formatBytes(file.bytes) : null,
                    file.modifiedMs ? `changed ${formatRelative(file.modifiedMs)}` : null,
                    file.note ?? null,
                    files.length > 1 ? `${at + 1} of ${files.length}` : null,
                  ]
                    .filter(Boolean)
                    .join(" · ")}
                </p>
              </div>
              {files.length > 1 ? (
                <div className="flex shrink-0 gap-1">
                  <Button
                    variant="ghost"
                    size="sm"
                    onClick={() => move(-1)}
                    aria-label="Previous file"
                  >
                    <ChevronLeft className="size-4" aria-hidden />
                  </Button>
                  <Button variant="ghost" size="sm" onClick={() => move(1)} aria-label="Next file">
                    <ChevronRight className="size-4" aria-hidden />
                  </Button>
                </div>
              ) : null}
              <Button variant="ghost" size="sm" onClick={() => void api.revealInFinder(file.path)}>
                <FolderOpen className="size-3.5" aria-hidden />
                {words().showInFileManager}
              </Button>
            </div>
          </>
        ) : null}
      </DialogContent>
    </Dialog>
  );
}
