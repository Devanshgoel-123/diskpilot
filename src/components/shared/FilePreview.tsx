import { Check, ChevronLeft, ChevronRight, File, FolderOpen, Loader2, Trash2 } from "lucide-react";
import { useEffect, useState } from "react";

import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "@/components/ui/dialog";
import { api, errorMessage } from "@/lib/api";
import { formatBytes, formatRelative, tildify } from "@/lib/format";
import { useApp } from "@/lib/store";
import { cachedThumbnail, loadThumbnail } from "@/lib/thumbnails";

export interface PreviewFile {
  path: string;
  bytes?: number;
  modifiedMs?: number | null;
  /** Shown under the name, e.g. "Keep" or "Will move to Trash". */
  note?: string;
  /** Whether the file is selected to move to the Trash. */
  selected?: boolean;
  /** False for a file that cannot be selected, such as the copy being kept. */
  selectable?: boolean;
}

const LARGE = 512;

function nameOf(path: string) {
  return path.slice(path.lastIndexOf("/") + 1);
}
function folderOf(path: string) {
  const i = path.lastIndexOf("/");
  return i > 0 ? path.slice(0, i) : "/";
}

/**
 * A large preview of one file at a time, like Quick Look in Finder: Space or
 * Escape closes it, the arrow keys move through the list. Clicking the image
 * opens the file in its own app. With `onToggleSelect`, the file can be
 * selected for the Trash from here (also with Command-Delete); nothing is
 * moved until the page's review step.
 */
export function FilePreview({
  files,
  index,
  onIndexChange,
  onClose,
  onToggleSelect,
}: {
  files: PreviewFile[];
  index: number | null;
  onIndexChange: (index: number) => void;
  onClose: () => void;
  onToggleSelect?: (path: string) => void;
}) {
  const home = useApp((s) => s.env?.homeDir);
  const at = index ?? 0;
  const file = index === null ? null : (files[index] ?? null);
  const [loaded, setLoaded] = useState<{ path: string; url: string | null } | null>(null);
  const cached = file ? cachedThumbnail(file.path, LARGE) : undefined;
  const url = cached !== undefined ? cached : loaded?.path === file?.path ? loaded?.url : undefined;
  const [openError, setOpenError] = useState<{ path: string; message: string } | null>(null);
  const canSelect = !!onToggleSelect && !!file && file.selectable !== false;

  const open = (path: string) => {
    setOpenError(null);
    api
      .openListedFile(path)
      .catch((e: unknown) => setOpenError({ path, message: errorMessage(e) }));
  };

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
          } else if (e.key === "Backspace" && e.metaKey && canSelect && file) {
            e.preventDefault();
            onToggleSelect?.(file.path);
          } else if (e.key === "o" && e.metaKey && file) {
            e.preventDefault();
            open(file.path);
          }
        }}
      >
        {file ? (
          <>
            <div className="flex h-[min(520px,60vh)] items-center justify-center rounded-t-lg bg-track">
              {url ? (
                <button
                  type="button"
                  onClick={() => open(file.path)}
                  title="Open in its app (⌘O)"
                  aria-label={`Open ${nameOf(file.path)}`}
                  className="flex size-full cursor-zoom-in items-center justify-center"
                >
                  <img
                    src={url}
                    alt={`Preview of ${nameOf(file.path)}`}
                    className="max-h-full max-w-full object-contain"
                    draggable={false}
                  />
                </button>
              ) : url === null ? (
                <div className="flex flex-col items-center gap-2 text-sm text-muted-foreground">
                  <File className="size-10 text-subtle-foreground" aria-hidden />
                  macOS has no preview for this kind of file.
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
                {openError?.path === file.path ? (
                  <p role="alert" className="mt-1 text-2xs text-critical">
                    {openError.message}
                  </p>
                ) : null}
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
                Show in Finder
              </Button>
              {canSelect ? (
                <Button
                  variant={file.selected ? "secondary" : "default"}
                  size="sm"
                  onClick={() => onToggleSelect?.(file.path)}
                  title="Select for Trash (⌘⌫)"
                  aria-pressed={!!file.selected}
                >
                  {file.selected ? (
                    <Check className="size-3.5" aria-hidden />
                  ) : (
                    <Trash2 className="size-3.5" aria-hidden />
                  )}
                  {file.selected ? "Selected for Trash" : "Select for Trash"}
                </Button>
              ) : null}
            </div>
          </>
        ) : null}
      </DialogContent>
    </Dialog>
  );
}
