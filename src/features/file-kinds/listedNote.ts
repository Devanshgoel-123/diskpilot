import { formatCount } from "@/lib/format";

/** The engine lists files of at least 25 KB, at most 2,000 per type (filekinds.rs). */
const LISTED_PER_KIND = 2000;

export function listedNote(listed: number, total: number): string {
  if (listed >= total) return `Showing all ${formatCount(total)} files.`;
  if (listed >= LISTED_PER_KIND)
    return `Showing the ${formatCount(listed)} largest of ${formatCount(total)} files. The rest count in the total.`;
  const smaller = total - listed;
  return `Showing the ${formatCount(listed)} files of 25 KB or more. The other ${formatCount(smaller)} ${
    smaller === 1 ? "is" : "are"
  } smaller and count in the total.`;
}
