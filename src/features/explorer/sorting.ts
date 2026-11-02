import type { DirectoryEntry } from "@/lib/types";

export type SortKey = "name" | "size" | "files" | "modified";

const collator = new Intl.Collator("en", { numeric: true, sensitivity: "base" });

/** Sort entries; unknown values (null) always go last regardless of direction. */
export function sortEntries(
  entries: DirectoryEntry[],
  key: SortKey,
  desc: boolean,
): DirectoryEntry[] {
  const value = (e: DirectoryEntry): number | null => {
    switch (key) {
      case "size":
        return e.bytes;
      case "files":
        return e.fileCount;
      case "modified":
        return e.modifiedMs;
      case "name":
        return null;
    }
  };
  return [...entries].sort((a, b) => {
    if (key === "name") {
      const c = collator.compare(a.name, b.name);
      return desc ? -c : c;
    }
    const va = value(a);
    const vb = value(b);
    if (va == null && vb == null) return 0;
    if (va == null) return 1;
    if (vb == null) return -1;
    return desc ? vb - va : va - vb;
  });
}
