// Drives an installed DiskPilot on a Windows runner through WebView2's
// DevTools port: scans, opens every page, and saves a screenshot and the
// page text of each, plus any console errors. Used by windows-smoke.yml.
import { mkdirSync, writeFileSync, appendFileSync } from "node:fs";
import { join } from "node:path";

const OUT = process.argv[2] ?? "smoke-results";
mkdirSync(OUT, { recursive: true });
const log = (...a) => {
  const line = a.join(" ");
  console.log(line);
  appendFileSync(join(OUT, "log.txt"), line + "\n");
};
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function target() {
  for (let i = 0; i < 60; i++) {
    try {
      const list = await (await fetch("http://127.0.0.1:9222/json")).json();
      const page = list.find((t) => t.type === "page");
      if (page) return page;
    } catch {
      /* not up yet */
    }
    await sleep(1000);
  }
  throw new Error("no WebView2 page on port 9222");
}

const page = await target();
log("page:", page.url);
const ws = new WebSocket(page.webSocketDebuggerUrl);
await new Promise((r, j) => {
  ws.onopen = r;
  ws.onerror = j;
});
let id = 0;
const pending = new Map();
const errors = [];
ws.onmessage = (m) => {
  const msg = JSON.parse(m.data);
  if (msg.id && pending.has(msg.id)) {
    pending.get(msg.id)(msg);
    pending.delete(msg.id);
  } else if (msg.method === "Runtime.exceptionThrown") {
    errors.push("exception: " + JSON.stringify(msg.params.exceptionDetails).slice(0, 800));
  } else if (
    msg.method === "Runtime.consoleAPICalled" &&
    ["error", "warning"].includes(msg.params.type)
  ) {
    errors.push(
      msg.params.type +
        ": " +
        msg.params.args
          .map((a) => a.value ?? a.description ?? "")
          .join(" ")
          .slice(0, 800),
    );
  }
};
const send = (method, params = {}) =>
  new Promise((r) => {
    const n = ++id;
    pending.set(n, r);
    ws.send(JSON.stringify({ id: n, method, params }));
  });
const evaluate = async (expression) =>
  (await send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true })).result
    ?.result?.value;

await send("Runtime.enable");
await send("Page.enable");
await send("Emulation.setDeviceMetricsOverride", {
  width: 1280,
  height: 820,
  deviceScaleFactor: 1,
  mobile: false,
});

let n = 0;
async function snap(name) {
  await sleep(1200);
  const shot = await send("Page.captureScreenshot", { format: "png" });
  const file = `${String(++n).padStart(2, "0")}-${name}`;
  if (shot.result?.data)
    writeFileSync(join(OUT, file + ".png"), Buffer.from(shot.result.data, "base64"));
  const text = (await evaluate("document.body.innerText")) ?? "";
  writeFileSync(join(OUT, file + ".txt"), text);
  log(`[${file}]`, text.replace(/\s+/g, " ").slice(0, 300));
}

// Ctrl plus a key, as the app's global shortcut handler sees it.
async function ctrl(key) {
  const code = /^[0-9]$/.test(key)
    ? "Digit" + key
    : key.length === 1
      ? "Key" + key.toUpperCase()
      : key;
  const base = { modifiers: 2, key, code, windowsVirtualKeyCode: key.toUpperCase().charCodeAt(0) };
  await send("Input.dispatchKeyEvent", { type: "rawKeyDown", ...base });
  await send("Input.dispatchKeyEvent", { type: "keyUp", ...base });
}
async function press(key, code, vk) {
  await send("Input.dispatchKeyEvent", {
    type: "rawKeyDown",
    key,
    code,
    windowsVirtualKeyCode: vk,
  });
  await send("Input.dispatchKeyEvent", { type: "keyUp", key, code, windowsVirtualKeyCode: vk });
}
const clickText = (re) =>
  evaluate(`(() => {
    const re = ${re};
    const el = [...document.querySelectorAll('button, [role=button], a')].find(b => re.test(b.innerText.trim()) && !b.disabled);
    if (el) { el.click(); return el.innerText.trim(); }
    return null;
  })()`);

await sleep(4000);
await snap("launch");

// Dismiss a first-run or what's-new dialog if one is open.
for (let i = 0; i < 3; i++) {
  const hasDialog = await evaluate("!!document.querySelector('[role=dialog]')");
  if (!hasDialog) break;
  await snap("dialog");
  const clicked = await clickText("/^(Got it|Continue|Close|Done|Skip|Not now|Start|OK)$/i");
  log("dialog button:", clicked);
  if (!clicked) await press("Escape", "Escape", 27);
  await sleep(800);
}

// Scan: a scan button on the Overview, else Ctrl+R.
const scanBtn = await clickText(
  "/^(Scan|Scan now|Start scan|Scan (my|this|the|home).*|Scan again)$/i",
);
log("scan button:", scanBtn);
if (!scanBtn) await ctrl("r");
await snap("scan-started");

const t0 = Date.now();
for (;;) {
  await sleep(3000);
  const busy = await evaluate(
    "/Scanning|Stop the scan|files so far|Stop scan/i.test(document.body.innerText) || !!document.querySelector('[aria-busy=true]')",
  );
  if (!busy) break;
  if (Date.now() - t0 > 8 * 60_000) {
    log("scan still running after 8 minutes");
    break;
  }
}
log("scan took", Math.round((Date.now() - t0) / 1000), "s");
await snap("overview-after-scan");

const pages = [
  ["2", "storage"],
  ["3", "duplicates"],
  ["4", "applications"],
  ["5", "developer"],
  ["6", "docker"],
  ["7", "cleanup"],
  ["8", "history"],
  [",", "settings"],
  ["1", "overview"],
];
for (const [key, name] of pages) {
  await ctrl(key);
  await snap(name);
}

// Developer Tools: run the transcript check.
await ctrl("5");
await sleep(1000);
const checked = await clickText("/^Check transcripts$/i");
log("check transcripts:", checked);
await sleep(4000);
await evaluate(
  "window.scrollTo(0, document.body.scrollHeight); document.querySelector('main')?.scrollTo(0, 1e6)",
);
await snap("transcripts");

await ctrl("/");
await snap("shortcuts-dialog");
await press("Escape", "Escape", 27);

writeFileSync(join(OUT, "console-errors.txt"), errors.join("\n") || "none");
log("console errors:", errors.length);
ws.close();
