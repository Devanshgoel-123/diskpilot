import type { Recommendation } from "@/lib/types";

/** Recommendations for `path` itself or anything inside it. */
export function recommendationsUnder(path: string, recs: Recommendation[]): Recommendation[] {
  return recs.filter((r) => r.path === path || r.path.startsWith(`${path}/`));
}
