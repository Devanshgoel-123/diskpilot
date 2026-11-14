// Measures the app's UI in Chrome against the synthetic harness (perf/index.html).
// Usage: node perf/ui.mjs [out.json]. Set CHROME to a Chrome binary if it is
// not in the default macOS location.
import { readdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { join } from "node:path";

import { chromium } from "playwright-core";
import { build, preview } from "vite";

const OUT = process.argv[2] ?? "perf-ui.json";
const CHROME = process.env.CHROME ?? "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
const RUNS = Number(process.env.PERF_RUNS ?? 5);
const median = (xs) => {
  const s = [...xs].sort((a, b) => a - b);
  return s[Math.floor(s.length / 2)];
};
const round = (x) => Math.round(x * 10) / 10;
// Timings are the best of the runs: the fastest run is the least disturbed by
// the machine, which keeps comparisons between commits steady.
const best = (xs) => Math.min(...xs);

// A production build of the harness, served like the app's own files.
// PERF_SKIP_BUILD=1 reuses dist-perf from an earlier run.
if (!process.env.PERF_SKIP_BUILD)
  await build({
    logLevel: "error",
    build: { outDir: "dist-perf", emptyOutDir: true, rollupOptions: { input: "perf/index.html" } },
  });
const server = await preview({
  // Serving needs no plugins, so a side without its own dependencies
  // installed (a cached base build in CI) can still be served.
  configFile: false,
  logLevel: "error",
  build: { outDir: "dist-perf" },
  preview: { port: 0, strictPort: false },
});
const url = `${server.resolvedUrls.local[0]}perf/`;
const browser = await chromium.launch({ executablePath: CHROME, headless: true });
const ctx = await browser.newContext({ viewport: { width: 1280, height: 820 } });
const page = await ctx.newPage();
// Slow the CPU down (4x by default) to stand in for older Macs and make
// regressions visible above the noise.
const THROTTLE = Number(process.env.PERF_CPU_THROTTLE ?? 4);
await (await ctx.newCDPSession(page)).send("Emulation.setCPUThrottlingRate", { rate: THROTTLE });

// Runs inside the page: navigates and reports when the DOM has settled
// (no changes for three frames), measured from the call.
const SETTLE = `window.__settle = (fn) => new Promise((resolve) => {
  let last = performance.now();
  const mo = new MutationObserver(() => { last = performance.now(); });
  mo.observe(document.body, { subtree: true, childList: true, attributes: true, characterData: true });
  const start = performance.now();
  fn();
  let quiet = 0;
  const tick = () => {
    if (performance.now() - last > 0 && quiet++ >= 3) { mo.disconnect(); resolve(last - start); return; }
    if (performance.now() - start > 5000) { mo.disconnect(); resolve(5000); return; }
    requestAnimationFrame(tick);
  };
  requestAnimationFrame(tick);
});
window.__longTasks = [];
new PerformanceObserver((l) => { for (const e of l.getEntries()) window.__longTasks.push(e.duration); })
  .observe({ type: "longtask", buffered: true });`;

const results = {
  firstRenderMs: [],
  pageSwitchMs: {},
  scrollFps: {},
  previewOpenMs: [],
  previewFirstImageMs: [],
  longTasks: [],
};
const PAGES = [
  "explorer",
  "duplicates",
  "applications",
  "developer",
  "docker",
  "recommendations",
  "history",
  "settings",
  "overview",
];

// One unrecorded load first, so every recorded run starts with the browser warm.
await page.goto(url, { waitUntil: "load" });
await page.waitForFunction(() => window.__perf, null, { polling: "raf", timeout: 20_000 });

for (let run = 0; run < RUNS; run++) {
  await page.goto(url, { waitUntil: "load" });
  await page.evaluate(SETTLE);
  // First render: navigation start to the Overview showing scan results.
  const first = await page.waitForFunction(
    () => {
      const main = document.querySelector("main");
      return main &&
        /used of/.test(main.textContent ?? "") &&
        /Largest/.test(main.textContent ?? "")
        ? performance.now()
        : null;
    },
    null,
    { polling: "raf", timeout: 20_000 },
  );
  results.firstRenderMs.push(await first.jsonValue());
  await page.waitForFunction(() => window.__perf, null, { polling: "raf" });

  for (const p of PAGES) {
    const ms = await page.evaluate(
      (p) => window.__settle(() => window.__perf.useApp.getState().navigate(p)),
      p,
    );
    (results.pageSwitchMs[p] ??= []).push(ms);
  }

  // Scrolling: one 60 px step per frame for 120 frames on the longest lists.
  for (const [name, setup] of [
    ["storageBrowse", () => window.__perf.useApp.getState().navigate("explorer")],
    [
      "filesByType",
      () => {
        const s = window.__perf.useApp.getState();
        s.navigate("explorer");
        s.setStorageView("types");
      },
    ],
  ]) {
    await page.evaluate(`window.__settle(${setup.toString()})`);
    const fps = await page.evaluate(
      () =>
        new Promise((resolve) => {
          const rows = document.querySelectorAll("main tr, main li");
          const row = rows[rows.length - 1] ?? document.querySelector("main");
          let el = row;
          while (
            el &&
            !(
              el.scrollHeight > el.clientHeight + 10 &&
              /auto|scroll/.test(getComputedStyle(el).overflowY)
            )
          )
            el = el.parentElement;
          el ??= document.scrollingElement;
          el.scrollTop = 0;
          let frames = 0;
          const start = performance.now();
          const step = () => {
            el.scrollTop += 60;
            if (++frames < 120) requestAnimationFrame(step);
            else resolve((frames * 1000) / (performance.now() - start));
          };
          requestAnimationFrame(step);
        }),
    );
    (results.scrollFps[name] ??= []).push(fps);
  }

  // Preview: click a row's preview button on Files by type, until the image shows.
  const open = await page.evaluate(
    () =>
      new Promise((resolve) => {
        const s = window.__perf.useApp.getState();
        s.navigate("explorer");
        s.setStorageView("types");
        const since = performance.now();
        const go = () => {
          const btns = document.querySelectorAll('main button[aria-label^="Preview "]');
          const btn = btns[Math.min(btns.length - 1, 7)];
          // Never wait forever: a page that fails to show files gives a time
          // that fails the gate instead of a job that hangs.
          if (!btn && performance.now() - since > 10_000)
            return resolve({ full: 5000, visual: 5000 });
          if (!btn) return requestAnimationFrame(go);
          const start = performance.now();
          btn.click();
          let visual = null;
          const wait = () => {
            const shown = [...document.querySelectorAll('[role="dialog"] img')].some(
              (i) => i.complete && i.naturalWidth > 0,
            );
            if (shown && visual === null) visual = performance.now() - start;
            const img = document.querySelector('[role="dialog"] img[alt^="Preview of"]');
            if (img && img.complete && img.naturalWidth > 0) {
              resolve({ full: performance.now() - start, visual });
              return;
            }
            if (performance.now() - start > 5000) resolve({ full: 5000, visual });
            else requestAnimationFrame(wait);
          };
          requestAnimationFrame(wait);
        };
        setTimeout(go, 300);
      }),
  );
  results.previewOpenMs.push(open.full);
  results.previewFirstImageMs.push(open.visual ?? open.full);
  results.longTasks.push(await page.evaluate(() => window.__longTasks.length));
  await page.keyboard.press("Escape");
}

// The app's own build (pnpm build), not the harness build.
const js = readdirSync("dist/assets")
  .filter((f) => f.endsWith(".js"))
  .reduce((s, f) => s + statSync(join("dist/assets", f)).size, 0);
const entry = readFileSync("dist/index.html", "utf8").match(/src="\/assets\/([^"]+\.js)"/)?.[1];
const entryJs = entry ? statSync(join("dist/assets", entry)).size : js;

const summary = {
  cpuThrottle: THROTTLE,
  jsBytes: js,
  entryJsBytes: entryJs,
  firstRenderMs: round(best(results.firstRenderMs)),
  pageSwitchMaxMs: round(Math.max(...PAGES.map((p) => best(results.pageSwitchMs[p])))),
  pageSwitchMs: Object.fromEntries(PAGES.map((p) => [p, round(best(results.pageSwitchMs[p]))])),
  scrollFpsMin: round(Math.min(...Object.values(results.scrollFps).map(median))),
  scrollFps: Object.fromEntries(
    Object.entries(results.scrollFps).map(([k, v]) => [k, round(median(v))]),
  ),
  previewOpenMs: round(best(results.previewOpenMs)),
  previewFirstImageMs: round(best(results.previewFirstImageMs)),
  longTasksPerRun: median(results.longTasks),
};
writeFileSync(OUT, JSON.stringify(summary, null, 2) + "\n");
console.log(JSON.stringify(summary, null, 2));
// Closing can hang on a throttled page; the results are already written.
const quit = (ms) => new Promise((r) => setTimeout(r, ms));
await Promise.race([Promise.all([browser.close(), server.close()]), quit(5000)]);
process.exit(0);
