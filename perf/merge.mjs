// Combines several result files from perf/ui.mjs (and the scan benchmark)
// into one, keeping the best value of each metric: the lowest time, the
// highest frame rate. Used by CI, which alternates base and pull request runs.
//
// Usage: node perf/merge.mjs <out.json> <in.json>...
import { readFileSync, writeFileSync } from "node:fs";

const [out, ...inputs] = process.argv.slice(2);
const all = inputs.map((p) => JSON.parse(readFileSync(p, "utf8")));
const higherIsBetter = (k) => /fps/i.test(k);
const merged = {};
for (const r of all) {
  for (const [k, v] of Object.entries(r)) {
    if (typeof v !== "number") {
      merged[k] ??= v;
      continue;
    }
    const cur = merged[k];
    merged[k] = cur === undefined ? v : higherIsBetter(k) ? Math.max(cur, v) : Math.min(cur, v);
  }
}
writeFileSync(out, JSON.stringify(merged, null, 2) + "\n");
