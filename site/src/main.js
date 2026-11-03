import "@fontsource-variable/geist";
import "@fontsource-variable/geist-mono";
import "./styles.css";

import { inject, track } from "@vercel/analytics";

// Vercel Web Analytics: cookieless page views. In development the package
// logs events to the console instead of sending them.
inject();

// Count clicks on download and GitHub links by where they were placed.
document.querySelectorAll("[data-track]").forEach((link) => {
  link.addEventListener("click", () => {
    track(link.dataset.track, { where: link.dataset.where ?? "unknown" });
  });
});

// ---------------------------------------------------------------------------
// Example data. Everything below is illustrative, labelled as such on the
// page, and shaped like what the app reports. Sizes are in GB.

const CATEGORIES = [
  { id: "app", label: "Application data", gb: 20.0 },
  { id: "cache", label: "Cache", gb: 14.6 },
  { id: "build", label: "Build artifacts", gb: 11.9 },
  { id: "dev", label: "Developer tools", gb: 9.6 },
  { id: "docker", label: "Docker storage", gb: 7.5 },
  { id: "unknown", label: "Unknown", gb: 4.0 },
  { id: "user", label: "User data", gb: 0.8 },
];
const NOT_SCANNED_GB = 61.6;
const FREE_GB = 115;

const EMPTY = {
  unknown:
    "DiskPilot suggests nothing it cannot explain. Unknown data is shown, never offered for cleanup.",
  user: "Your own files are never suggested for cleanup.",
};

const START = [
  {
    cat: "build",
    title: "Rust build output",
    path: "~/code/ledger/target",
    gb: 11.0,
    tier: "safe",
  },
  {
    cat: "build",
    title: "Xcode derived data for Atlas",
    path: "~/Library/Developer/Xcode/DerivedData/Atlas-bqkxwcmd",
    gb: 6.2,
    tier: "safe",
  },
  {
    cat: "dev",
    title: "Ollama layers no model uses",
    path: "~/.ollama/models/blobs",
    gb: 4.1,
    tier: "review",
  },
  {
    cat: "cache",
    title: "Google application caches",
    path: "~/Library/Caches/Google",
    gb: 3.6,
    tier: "safe",
  },
  {
    cat: "docker",
    title: "Unused Docker build cache",
    path: "docker builder prune",
    gb: 3.1,
    tier: "safe",
  },
  {
    cat: "cache",
    title: "Cursor downloaded update",
    path: "~/Library/Caches/com.todesktop.230313mzl4w4u92.ShipIt",
    gb: 1.6,
    tier: "safe",
  },
  {
    cat: "build",
    title: "node_modules in an inactive project",
    path: "~/code/old-dashboard/node_modules",
    gb: 1.4,
    tier: "review",
  },
  {
    cat: "cache",
    title: "Homebrew downloads",
    path: "~/Library/Caches/Homebrew",
    gb: 0.95,
    tier: "safe",
  },
  {
    cat: "app",
    title: "Editor storage for 81 deleted projects",
    path: "~/Library/Application Support/Cursor/User/workspaceStorage",
    gb: 0.64,
    tier: "review",
  },
  {
    cat: "dev",
    title: "Older Claude Code versions",
    path: "~/.local/share/claude/versions",
    gb: 0.55,
    tier: "review",
  },
];

const TIER_LABEL = { safe: "Likely safe", review: "Review first", keep: "Keep" };

const RECS = [
  {
    name: "Rust build output",
    sub: "Cargo rebuilds everything in target/ on the next build.",
    path: "~/code/ledger/target",
    gb: 11.0,
    tier: "safe",
    why: "A Cargo.toml sits next to this target/ folder, so it is Cargo's build output for that project.",
    after: "The next cargo build recreates it. That first build takes longer.",
    run: "cargo clean",
  },
  {
    name: "Xcode derived data for Atlas",
    sub: "Build products and indexes Xcode recreates.",
    path: "~/Library/Developer/Xcode/DerivedData/Atlas-bqkxwcmd",
    gb: 6.2,
    tier: "safe",
    why: "Its info.plist names the workspace it belongs to, which still exists at ~/code/atlas.",
    after: "Xcode rebuilds it the next time you build Atlas, and re-indexes the project.",
  },
  {
    name: "Google application caches",
    sub: "macOS designates ~/Library/Caches for data apps can recreate.",
    path: "~/Library/Caches/Google",
    gb: 3.6,
    tier: "safe",
    why: "Everything here is inside ~/Library/Caches, which apps use for data they can download or compute again.",
    after: "Chrome and Google's updater refill what they need over time.",
  },
  {
    name: "node_modules in an inactive project",
    sub: "package.json and the lockfile have not changed in 14 months.",
    path: "~/code/old-dashboard/node_modules",
    gb: 1.4,
    tier: "review",
    why: "Activity is inferred from when the dependency manifests last changed, which can miss recent work that did not touch them.",
    after: "Running pnpm install in the project recreates it from the lockfile.",
    run: "pnpm install",
  },
  {
    name: "Editor storage for 81 deleted projects",
    sub: "Cursor keeps state for every folder it has opened.",
    path: "~/Library/Application Support/Cursor/User/workspaceStorage",
    gb: 0.64,
    tier: "review",
    why: "Each folder's workspace.json points to a project folder that no longer exists on this Mac.",
    after:
      "Cursor forgets layout and history for those projects. Projects that still exist are not included.",
  },
  {
    name: "Older Claude Code versions",
    sub: "2.1.312 and 2.1.314. A newer version is installed.",
    path: "~/.local/share/claude/versions",
    gb: 0.55,
    tier: "review",
    why: "The claude launcher points to 2.1.316. The version in use is never marked.",
    after:
      "Nothing changes for the claude command. You can no longer switch back to these versions offline.",
  },
  {
    name: "Docker Desktop disk image",
    sub: "Holds every image, container and volume.",
    path: "~/Library/Containers/com.docker.docker/Data/vms/0/data/Docker.raw",
    gb: 7.4,
    tier: "keep",
    why: "Volumes inside it can hold databases and other data that exists nowhere else.",
    keep: "DiskPilot never offers to remove this file. Reclaim space from the Docker page instead, one image or cache at a time.",
  },
  {
    name: "Cursor chat history",
    sub: "Settings, extension state and 1,339 conversations.",
    path: "~/Library/Application Support/Cursor/User/globalStorage/state.vscdb",
    gb: 3.1,
    tier: "keep",
    why: "This database holds your conversations and settings. Removing it resets Cursor.",
    keep: "Conversation history is never recommended for cleanup.",
  },
];

// ---------------------------------------------------------------------------

const $ = (sel, root = document) => root.querySelector(sel);

function fmt(gb) {
  if (gb >= 100) return `${Math.round(gb)} GB`;
  if (gb >= 1) return `${gb.toFixed(1)} GB`;
  return `${Math.round(gb * 1000)} MB`;
}

function esc(s) {
  return s.replace(
    /[&<>"]/g,
    (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c],
  );
}

const color = (id) => `var(--cat-${id})`;
const badge = (tier) =>
  `<span class="badge ${tier}" title="${TIER_LABEL[tier]}"><i></i>${TIER_LABEL[tier]}</span>`;

// ---------- Hero: the Overview ----------

function initOverview() {
  const strip = $("#hero-strip");
  const legend = $("#hero-legend");
  const start = $("#hero-start");
  const where = $("#hero-where");
  const sub = $("#hero-start-sub");
  if (!strip) return;

  const scanned = CATEGORIES.reduce((a, c) => a + c.gb, 0);
  const segs = [
    ...CATEGORIES.map((c) => ({ ...c, cls: "", c: color(c.id) })),
    { id: "not-scanned", label: "Not scanned", gb: NOT_SCANNED_GB, cls: "other" },
    { id: "free", label: "Available", gb: FREE_GB, cls: "free" },
  ];
  strip.innerHTML = segs
    .map(
      (s, i) =>
        `<span role="listitem" class="${s.cls}" data-cat="${s.id}" style="--w:${s.gb};--c:${s.c ?? "transparent"};--i:${i}" title="${s.label}, ${fmt(s.gb)}"><span class="sr-only">${s.label}, ${fmt(s.gb)}</span></span>`,
    )
    .join("");
  strip.classList.add("grow");

  legend.innerHTML = CATEGORIES.map(
    (c) =>
      `<button type="button" aria-pressed="false" data-cat="${c.id}"><i style="--c:${color(c.id)}"></i>${c.label}<b>${fmt(c.gb)}</b></button>`,
  ).join("");

  where.innerHTML = CATEGORIES.map(
    (c, i) =>
      `<li data-cat="${c.id}"><i class="dot" style="--c:${color(c.id)}"></i><span>${c.label}</span><span class="bar"><span style="--p:${(c.gb / CATEGORIES[0].gb) * 100};--c:${color(c.id)};--i:${i}"></span></span><span class="sz">${fmt(c.gb)}</span></li>`,
  ).join("");
  where.classList.add("grow");

  const recoverable = START.filter((s) => s.tier === "safe").reduce((a, s) => a + s.gb, 0);
  $("#hero-recoverable").textContent = fmt(recoverable);

  function renderStart(cat) {
    const items = START.filter((s) => !cat || s.cat === cat)
      .sort((a, b) => b.gb - a.gb)
      .slice(0, 5);
    const c = CATEGORIES.find((x) => x.id === cat);
    sub.textContent = cat
      ? `Suggestions in ${c.label.toLowerCase()}, ${fmt(c.gb)} in total.`
      : "The largest items DiskPilot can explain and clean up.";
    if (!items.length) {
      start.innerHTML = `<li class="empty-row"><span class="empty">${EMPTY[cat] ?? "Nothing to suggest here."}</span></li>`;
      return;
    }
    start.innerHTML = items
      .map(
        (s, i) =>
          `<li style="animation-delay:${i * 35}ms"><span class="sq" style="--c:${color(s.cat)}"></span><span class="t"><p>${esc(s.title)}</p><code>${esc(s.path)}</code></span>${badge(s.tier)}<span class="sz">${fmt(s.gb)}</span></li>`,
      )
      .join("");
  }

  let focus = null;
  function choose(cat) {
    focus = focus === cat ? null : cat;
    for (const b of legend.querySelectorAll("button")) {
      b.setAttribute("aria-pressed", String(b.dataset.cat === focus));
    }
    for (const el of [strip, where]) {
      if (focus) el.dataset.focus = focus;
      else delete el.dataset.focus;
      for (const s of el.querySelectorAll("[data-cat]")) {
        s.classList.toggle("on", s.dataset.cat === focus);
      }
    }
    renderStart(focus);
  }

  legend.addEventListener("click", (e) => {
    const b = e.target.closest("button[data-cat]");
    if (b) choose(b.dataset.cat);
  });
  strip.addEventListener("click", (e) => {
    const s = e.target.closest("[data-cat]");
    if (s && CATEGORIES.some((c) => c.id === s.dataset.cat)) choose(s.dataset.cat);
  });

  renderStart(null);
}

// ---------- Cleanup demonstration ----------

function initDemo() {
  const app = $("#demo-app");
  if (!app) return;
  const rows = $("#demo-rows");
  const insp = $("#demo-inspector");
  const tabs = [...app.querySelectorAll('#demo-tabs [role="tab"]')];
  const selectedLabel = $("#demo-selected");
  const reviewBtn = $("#demo-review");
  const sheet = $("#demo-sheet");
  const ok = $("#sheet-ok");
  const go = $("#sheet-go");

  let tier = "all";
  let active = 0;
  const checked = new Set();

  function renderRows() {
    const list = RECS.map((r, i) => ({ r, i })).filter(
      ({ r }) => tier === "all" || r.tier === tier,
    );
    if (!list.some(({ i }) => i === active)) active = list[0]?.i ?? 0;
    rows.innerHTML = list
      .map(({ r, i }, n) => {
        const sel =
          r.tier === "keep"
            ? `<svg class="lock" aria-label="Not offered for cleanup"><use href="#i-lock" /></svg>`
            : `<input class="check" type="checkbox" data-i="${i}" ${checked.has(i) ? "checked" : ""} aria-label="Include ${esc(r.name)}" />`;
        return `<tr data-i="${i}" aria-selected="${i === active}" style="animation-delay:${n * 25}ms">
          <td class="c-sel">${sel}</td>
          <td class="cl-name"><p>${esc(r.name)}</p><span>${esc(r.sub)}</span></td>
          <td class="c-loc"><code title="${esc(r.path)}">&lrm;${esc(r.path)}&lrm;</code></td>
          <td class="cl-size">${fmt(r.gb)}</td>
          <td>${badge(r.tier)}</td>
        </tr>`;
      })
      .join("");
  }

  function renderInspector() {
    const r = RECS[active];
    const action = r.keep
      ? `<p class="never"><svg aria-hidden="true"><use href="#i-lock" /></svg><span>${esc(r.keep)}</span></p>`
      : `<dl>
           <dt>What happens after cleanup</dt><dd>${esc(r.after)}</dd>
           ${r.run ? `<dt>Prefer to do it yourself</dt><dd><code>${esc(r.run)}</code></dd>` : ""}
         </dl>
         <p class="never"><svg aria-hidden="true"><use href="#i-undo" /></svg><span>It moves to the Trash, so you can put it back until you empty the Trash.</span></p>`;
    insp.innerHTML = `<div class="insp">
      ${badge(r.tier)}
      <h3 style="margin-top:10px">${esc(r.name)}</h3>
      <code class="path">${esc(r.path)}</code>
      <div class="big"><b>${fmt(r.gb)}</b><span>${r.keep ? "on disk" : "recoverable"}</span></div>
      <dl><dt>Why DiskPilot flagged it</dt><dd>${esc(r.why)}</dd></dl>
      ${action}
    </div>`;
  }

  function renderFoot() {
    const total = [...checked].reduce((a, i) => a + RECS[i].gb, 0);
    selectedLabel.innerHTML = checked.size
      ? `<b>${checked.size}</b> selected, <b>${fmt(total)}</b>`
      : "Nothing selected";
    reviewBtn.disabled = checked.size === 0;
  }

  function render() {
    renderRows();
    renderInspector();
    renderFoot();
  }

  rows.addEventListener("click", (e) => {
    const box = e.target.closest("input.check");
    if (box) {
      const i = Number(box.dataset.i);
      if (box.checked) checked.add(i);
      else checked.delete(i);
      active = i;
      for (const tr of rows.querySelectorAll("tr"))
        tr.setAttribute("aria-selected", String(Number(tr.dataset.i) === i));
      renderInspector();
      renderFoot();
      return;
    }
    const tr = e.target.closest("tr[data-i]");
    if (!tr) return;
    active = Number(tr.dataset.i);
    for (const t of rows.querySelectorAll("tr")) t.setAttribute("aria-selected", String(t === tr));
    renderInspector();
  });

  function selectTab(tab, focus) {
    for (const t of tabs) {
      const on = t === tab;
      t.setAttribute("aria-selected", String(on));
      t.tabIndex = on ? 0 : -1;
    }
    tier = tab.dataset.tier;
    if (focus) tab.focus();
    renderRows();
    renderInspector();
  }
  tabs.forEach((tab, i) => {
    tab.addEventListener("click", () => selectTab(tab, false));
    tab.addEventListener("keydown", (e) => {
      const step = e.key === "ArrowRight" ? 1 : e.key === "ArrowLeft" ? -1 : 0;
      if (!step) return;
      e.preventDefault();
      selectTab(tabs[(i + step + tabs.length) % tabs.length], true);
    });
  });

  // The review sheet.
  function openSheet() {
    const items = [...checked].map((i) => RECS[i]);
    $("#sheet-list").innerHTML = items
      .map((r) => `<li><code title="${esc(r.path)}">${esc(r.path)}</code><b>${fmt(r.gb)}</b></li>`)
      .join("");
    $("#sheet-total").textContent = fmt(items.reduce((a, r) => a + r.gb, 0));
    go.textContent = `Move ${items.length} ${items.length === 1 ? "item" : "items"} to Trash`;
    ok.checked = false;
    go.disabled = true;
    $("#sheet-review").hidden = false;
    $("#sheet-done").hidden = true;
    sheet.hidden = false;
    ok.focus();
  }

  function closeSheet() {
    sheet.hidden = true;
    reviewBtn.focus();
  }

  reviewBtn.addEventListener("click", openSheet);
  $("#sheet-cancel").addEventListener("click", closeSheet);
  ok.addEventListener("change", () => {
    go.disabled = !ok.checked;
  });
  go.addEventListener("click", () => {
    const items = [...checked].map((i) => RECS[i]);
    go.disabled = true;
    go.textContent = "Checking each path…";
    setTimeout(() => {
      const total = items.reduce((a, r) => a + r.gb, 0);
      $("#done-text").textContent =
        `${items.length} ${items.length === 1 ? "item" : "items"}, ${fmt(total)}, moved to the Trash in this demonstration.`;
      $("#sheet-review").hidden = true;
      $("#sheet-done").hidden = false;
      $("#sheet-reset").focus();
    }, 900);
  });
  $("#sheet-reset").addEventListener("click", () => {
    checked.clear();
    sheet.hidden = true;
    selectTab(tabs[0], false);
    reviewBtn.focus();
    render();
  });
  sheet.addEventListener("keydown", (e) => {
    if (e.key === "Escape") closeSheet();
    if (e.key !== "Tab") return;
    // Keep focus inside the sheet while it is open.
    const f = [...sheet.querySelectorAll("button, input")].filter(
      (el) => !el.disabled && el.offsetParent,
    );
    if (!f.length) return;
    const first = f[0];
    const last = f[f.length - 1];
    if (e.shiftKey && document.activeElement === first) {
      e.preventDefault();
      last.focus();
    } else if (!e.shiftKey && document.activeElement === last) {
      e.preventDefault();
      first.focus();
    }
  });

  render();
}

initOverview();
initDemo();

// ---------- Theme: system, then light, then dark ----------
// The choice is remembered in this browser only.

const order = ["system", "light", "dark"];
const toggle = document.getElementById("theme-toggle");

function current() {
  return document.documentElement.dataset.theme ?? "system";
}

function apply(theme) {
  if (theme === "system") delete document.documentElement.dataset.theme;
  else document.documentElement.dataset.theme = theme;
  toggle.setAttribute("aria-label", `Theme: ${theme}. Switch theme`);
  toggle.title = `Theme: ${theme}`;
  try {
    if (theme === "system") localStorage.removeItem("diskpilot-theme");
    else localStorage.setItem("diskpilot-theme", theme);
  } catch {
    /* storage unavailable: the choice lasts for this page view */
  }
}

toggle.setAttribute("aria-label", `Theme: ${current()}. Switch theme`);
toggle.title = `Theme: ${current()}`;
toggle.addEventListener("click", () => {
  apply(order[(order.indexOf(current()) + 1) % order.length]);
});
