// Compares performance results of a pull request with its base branch, both
// measured on the same runner, and with the absolute budgets in
// docs/perf/budgets.json. Prints a Markdown table and exits 1 on a regression.
//
// Usage: node perf/compare.mjs <base.json> <head.json> [budgets.json]
import { existsSync, readFileSync } from "node:fs";

const [basePath, headPath, budgetsPath = "docs/perf/budgets.json"] = process.argv.slice(2);
const read = (p) => (p && existsSync(p) ? JSON.parse(readFileSync(p, "utf8")) : null);
const base = read(basePath);
const head = read(headPath);
const budgets = read(budgetsPath) ?? {};
if (!head) {
  console.error(`No results at ${headPath}`);
  process.exit(2);
}

// Lower is better unless `higherIsBetter`. A change counts as a regression
// only when it is worse by more than `tolerance` AND by more than `floor`
// (absolute), so a 2 ms page going to 3 ms does not fail a pull request.
// The floors come from measuring the same commit twice: first render varied
// by up to ~20 ms (about 20%) between sets of runs, the other timings by a few ms.
const METRICS = [
  // The same scanner measured 12% apart on the CI runner, so the scan needs a
  // wider margin.
  {
    key: "scanFixtureMs",
    label: "Scan of the generated tree",
    unit: "ms",
    tolerance: 0.15,
    floor: 100,
  },
  { key: "jsBytes", label: "JavaScript, total", unit: "B", tolerance: 0.05, floor: 2048 },
  { key: "entryJsBytes", label: "JavaScript at startup", unit: "B", tolerance: 0.05, floor: 2048 },
  { key: "firstRenderMs", label: "First render", unit: "ms", tolerance: 0.2, floor: 40 },
  { key: "pageSwitchMaxMs", label: "Slowest page switch", unit: "ms", floor: 15 },
  { key: "previewFirstImageMs", label: "Preview, first image", unit: "ms", floor: 15 },
  { key: "previewOpenMs", label: "Preview, full image", unit: "ms", floor: 15 },
  {
    key: "scrollFpsMin",
    label: "Scrolling, slowest list",
    unit: "fps",
    higherIsBetter: true,
    floor: 3,
  },
  { key: "longTasksPerRun", label: "Long tasks per run", unit: "", floor: 2 },
];
const TOLERANCE = 0.1;

const fmt = (v, unit) => {
  if (v === undefined || v === null) return "–";
  if (unit === "B") return `${(v / 1024).toFixed(1)} KB`;
  return `${Math.round(v * 10) / 10}${unit ? ` ${unit}` : ""}`;
};

let failed = false;
const rows = [];
for (const m of METRICS) {
  const h = head[m.key];
  if (h === undefined) continue;
  const b = base?.[m.key];
  const tol = m.tolerance ?? TOLERANCE;
  let status = "ok";
  let change = "–";
  if (b !== undefined && b !== null) {
    const worse = m.higherIsBetter ? b - h : h - b;
    const rel = b === 0 ? 0 : worse / b;
    const pct = ((h - b) / (b || 1)) * 100;
    change = `${pct >= 0 ? "+" : ""}${pct.toFixed(1)}%`;
    if (rel > tol && worse > m.floor) {
      status = `regressed (more than ${Math.round(tol * 100)}%)`;
      failed = true;
    }
  }
  const budget = budgets[m.key];
  if (budget !== undefined) {
    const over = m.higherIsBetter ? h < budget : h > budget;
    if (over) {
      status = `${status === "ok" ? "" : status + "; "}over budget (${fmt(budget, m.unit)})`;
      failed = true;
    }
  }
  rows.push(`| ${m.label} | ${fmt(b, m.unit)} | ${fmt(h, m.unit)} | ${change} | ${status} |`);
}

const out = [
  "### Performance",
  "",
  base
    ? "Measured on the same runner for the base branch and this pull request."
    : "No base results; checked against the budgets only.",
  "",
  "| Metric | Base | This PR | Change | Status |",
  "| --- | --- | --- | --- | --- |",
  ...rows,
  "",
  failed
    ? "**A metric regressed or went over budget.** See docs/perf/BASELINE.md for how to measure locally."
    : "No regressions.",
].join("\n");
console.log(out);
process.exit(failed ? 1 : 0);
