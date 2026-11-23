// Combines several result files from perf/ui.mjs (and the scan benchmark)
// into one, keeping the best value of each metric: the lowest time, the
// highest frame rate. Used by CI, which alternates base and pull request runs.
// With --median it keeps the median instead: the CI runner's UI timings vary
// by 2-3x between page loads of the same build, so the best of a few loads
// swung by up to 45% between runs, while the median of 9 stayed within 13%.
//
// Usage: node perf/merge.mjs [--median] <out.json> <in.json>...
import { readFileSync, writeFileSync } from "node:fs";

const args = process.argv.slice(2);
const useMedian = args[0] === "--median";
const [out, ...inputs] = useMedian ? args.slice(1) : args;
const all = inputs.map((p) => JSON.parse(readFileSync(p, "utf8")));
const higherIsBetter = (k) => /fps/i.test(k);
const merged = {};
if (useMedian) {
  for (const r of all)
    for (const [k, v] of Object.entries(r)) if (typeof v !== "number") merged[k] ??= v;
  const keys = new Set(all.flatMap((r) => Object.keys(r)));
  for (const k of keys) {
    const vs = all
      .map((r) => r[k])
      .filter((v) => typeof v === "number")
      .sort((a, b) => a - b);
    // The middle value; with an even count, the lower of the two.
    if (vs.length) merged[k] = vs[Math.floor((vs.length - 1) / 2)];
  }
  writeFileSync(out, JSON.stringify(merged, null, 2) + "\n");
  process.exit(0);
}
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
