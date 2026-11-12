import { useEffect, useState } from "react";

import { api } from "@/lib/api";
import type { FileEntry, FileKind, ScanSummary } from "@/lib/types";

/**
 * The files the scan listed for each type, fetched when a type is shown
 * rather than sent with every scan summary (they were most of its size).
 * Everything loaded belongs to `summary`; a new summary starts over.
 */
export function useKindFiles(summary: ScanSummary | null, kind: FileKind | null) {
  const [loaded, setLoaded] = useState<{
    summary: ScanSummary | null;
    files: Map<FileKind, FileEntry[]>;
  }>({ summary: null, files: new Map() });
  const files = loaded.summary === summary ? loaded.files : EMPTY;

  useEffect(() => {
    if (!summary || !kind || files.has(kind)) return;
    let live = true;
    void api
      .getFileKindFiles(kind)
      .catch(() => [] as FileEntry[])
      .then((list) => {
        if (!live) return;
        setLoaded((prev) => {
          const next = new Map(prev.summary === summary ? prev.files : EMPTY);
          next.set(kind, list);
          return { summary, files: next };
        });
      });
    return () => {
      live = false;
    };
  }, [summary, kind, files]);

  return files;
}

const EMPTY = new Map<FileKind, FileEntry[]>();
