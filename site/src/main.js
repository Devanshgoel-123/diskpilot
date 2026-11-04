import "@fontsource-variable/geist";
import "@fontsource-variable/geist-mono";
import "./styles.css";

import { inject, track } from "@vercel/analytics";

// Vercel Web Analytics: cookieless page views. In development the package
// logs events to the console instead of sending them.
inject();

document.documentElement.classList.add("js");

// Count clicks on download and GitHub links by where they were placed.
document.querySelectorAll("[data-track]").forEach((link) => {
  link.addEventListener("click", () => {
    track(link.dataset.track, { where: link.dataset.where ?? "unknown" });
  });
});

const $ = (sel, root = document) => root.querySelector(sel);
const $$ = (sel, root = document) => [...root.querySelectorAll(sel)];
const reduced = matchMedia("(prefers-reduced-motion: reduce)").matches;

function fmt(gb) {
  if (gb >= 100) return `${Math.round(gb)} GB`;
  if (gb >= 1) return `${gb.toFixed(1)} GB`;
  return `${Math.round(gb * 1000)} MB`;
}

function esc(s) {
  return String(s).replace(
    /[&<>"]/g,
    (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c],
  );
}

const TIER_LABEL = { safe: "Likely safe", review: "Review first", keep: "Keep" };
const badge = (tier) => `<span class="badge ${tier}"><i></i>${TIER_LABEL[tier]}</span>`;

// ---------------------------------------------------------------------------
// Sample data. Illustrative, labelled as such on the page, and shaped like
// what the app reports. Sizes are in GB.

const CATEGORIES = [
  { id: "app", label: "Application data", gb: 20.0 },
  { id: "cache", label: "Caches", gb: 14.6 },
  { id: "build", label: "Build artifacts", gb: 11.9 },
  { id: "dev", label: "Developer tools", gb: 9.6 },
  { id: "docker", label: "Docker", gb: 7.5 },
  { id: "unknown", label: "Unknown", gb: 4.0 },
  { id: "user", label: "Your files", gb: 0.8 },
];
const SCANNED_GB = CATEGORIES.reduce((a, c) => a + c.gb, 0);
const RECOVERABLE_GB = 23.2;
const FILES = 912408;

const FOUND = [
  {
    name: "Docker build cache",
    path: "docker builder prune",
    gb: 3.1,
    tier: "safe",
    by: "Docker BuildKit",
    contains: "Layers saved from earlier docker build runs, so rebuilds can skip steps.",
    why: "Docker keeps it until you prune it. Docker's own report says none of it is in use.",
    verdict:
      "The next build is slower. DiskPilot shows the exact docker command before it runs, and never touches volumes.",
  },
  {
    name: "Cursor workspace storage",
    path: "~/Library/Application Support/Cursor/User/workspaceStorage",
    gb: 0.64,
    tier: "review",
    by: "Cursor",
    contains: "Layout, history and index state for every folder Cursor has opened.",
    why: "81 of these folders point to projects that no longer exist on this Mac.",
    verdict:
      "Cursor forgets those projects' layout and history. Folders for projects that still exist are left out.",
  },
  {
    name: "Xcode DerivedData",
    path: "~/Library/Developer/Xcode/DerivedData",
    gb: 6.2,
    tier: "safe",
    by: "Xcode",
    contains: "Build products, indexes and logs for each project Xcode has opened.",
    why: "Xcode writes it on every build. Its info.plist names the workspace it belongs to.",
    verdict: "Xcode rebuilds it the next time you build, which takes longer the first time.",
  },
  {
    name: "node_modules, inactive project",
    path: "~/code/old-dashboard/node_modules",
    gb: 1.4,
    tier: "review",
    by: "pnpm",
    contains: "Installed dependencies for one project.",
    why: "package.json and the lockfile haven't changed in 14 months.",
    verdict:
      "pnpm install recreates it from the lockfile. Activity is inferred from the manifests, so check the project first.",
  },
  {
    name: "Cursor chat history",
    path: "~/Library/Application Support/Cursor/User/globalStorage/state.vscdb",
    gb: 3.1,
    tier: "keep",
    by: "Cursor",
    contains: "Settings, extension state and your conversations.",
    why: "It holds data that exists nowhere else.",
    verdict: "Never suggested. Conversation history is always kept.",
  },
];

const RECS = [
  { name: "Rust build output", path: "~/code/ledger/target", gb: 11.0, tier: "safe" },
  {
    name: "Xcode DerivedData",
    path: "~/Library/Developer/Xcode/DerivedData",
    gb: 6.2,
    tier: "safe",
  },
  { name: "Google app caches", path: "~/Library/Caches/Google", gb: 3.6, tier: "safe" },
  { name: "Docker build cache", path: "docker builder prune", gb: 3.1, tier: "safe" },
  {
    name: "node_modules, inactive project",
    path: "~/code/old-dashboard/node_modules",
    gb: 1.4,
    tier: "review",
  },
  {
    name: "Cursor storage for 81 deleted projects",
    path: "~/Library/Application Support/Cursor/User/workspaceStorage",
    gb: 0.64,
    tier: "review",
  },
  {
    name: "Older Claude Code versions",
    path: "~/.local/share/claude/versions",
    gb: 0.55,
    tier: "review",
  },
  {
    name: "Docker Desktop disk image",
    path: "~/Library/Containers/com.docker.docker/Data/vms/0/data/Docker.raw",
    gb: 7.4,
    tier: "keep",
  },
  {
    name: "Cursor chat history",
    path: "~/Library/Application Support/Cursor/User/globalStorage/state.vscdb",
    gb: 3.1,
    tier: "keep",
  },
];

// ---------- Nav ----------

const nav = $(".nav");
const onScroll = () => nav.classList.toggle("scrolled", scrollY > 8);
addEventListener("scroll", onScroll, { passive: true });
onScroll();

const menuBtn = $("#menu-btn");
const links = $("#nav-links");
function setMenu(open) {
  links.classList.toggle("open", open);
  menuBtn.setAttribute("aria-expanded", String(open));
  menuBtn.setAttribute("aria-label", open ? "Close menu" : "Open menu");
}
menuBtn.addEventListener("click", () => setMenu(!links.classList.contains("open")));
links.addEventListener("click", (e) => {
  if (e.target.closest("a")) setMenu(false);
});
addEventListener("keydown", (e) => {
  if (e.key === "Escape" && links.classList.contains("open")) {
    setMenu(false);
    menuBtn.focus();
  }
});

// ---------- Theme: dark by default, light when chosen ----------

const toggle = $("#theme-toggle");
const isLight = () => document.documentElement.dataset.theme === "light";

function syncTheme() {
  const theme = isLight() ? "light" : "dark";
  for (const img of $$("img[data-shot]")) {
    const src = `/shots/${img.dataset.shot}-${theme}.jpg`;
    if (img.getAttribute("src") !== src) img.setAttribute("src", src);
  }
  for (const source of $$("source[data-shot-m]")) {
    const src = `/shots/${source.dataset.shotM}-m-${theme}.jpg`;
    if (source.getAttribute("srcset") !== src) source.setAttribute("srcset", src);
  }
  toggle.setAttribute("aria-label", isLight() ? "Switch to dark theme" : "Switch to light theme");
  $('meta[name="theme-color"]').content = isLight() ? "#f7f7f5" : "#121316";
}

toggle.addEventListener("click", () => {
  const light = !isLight();
  if (light) document.documentElement.dataset.theme = "light";
  else delete document.documentElement.dataset.theme;
  try {
    if (light) localStorage.setItem("diskpilot-theme", "light");
    else localStorage.removeItem("diskpilot-theme");
  } catch {
    /* storage unavailable: the choice lasts for this page view */
  }
  syncTheme();
});
syncTheme();

// ---------- Reveal on scroll ----------

function whenVisible(el, fn, threshold = 0.25) {
  if (!el) return;
  if (!("IntersectionObserver" in window)) return fn();
  const io = new IntersectionObserver(
    (entries) => {
      if (entries.some((e) => e.isIntersecting)) {
        io.disconnect();
        fn();
      }
    },
    { threshold },
  );
  io.observe(el);
}

for (const el of $$(".reveal")) whenVisible(el, () => el.classList.add("in"), 0.12);

// ---------- The problem: "System Data", resolved ----------

function initResolve() {
  const box = $("#resolve");
  const bar = $("#resolve-bar");
  const legend = $("#resolve-legend");
  if (!box) return;
  const shown = CATEGORIES.filter((c) => c.id !== "unknown" && c.id !== "user");
  const rest = SCANNED_GB - shown.reduce((a, c) => a + c.gb, 0);
  // Same proportions as the Storage settings bar above: the 68.4 GB of
  // System Data, then apps, documents and free space.
  bar.innerHTML =
    shown
      .map(
        (c, i) =>
          `<span data-c style="--w:${c.gb};--c:var(--cat-${c.id});--i:${i}" title="${c.label}, ${fmt(c.gb)}"></span>`,
      )
      .join("") +
    `<span class="rest" style="--w:${rest}" title="Unknown and your files, ${fmt(rest)}"></span>` +
    `<span class="seg-apps" style="--w:24"></span>` +
    `<span class="seg-docs" style="--w:37.6"></span>` +
    `<span class="free" style="--w:115"></span>`;
  bar.setAttribute(
    "aria-label",
    `The same ${fmt(SCANNED_GB)} split into ${shown.map((c) => `${c.label} ${fmt(c.gb)}`).join(", ")}, and ${fmt(rest)} unknown or your own files.`,
  );
  legend.innerHTML = shown
    .map(
      (c, i) =>
        `<li style="--i:${i}"><i class="sw" style="--c:var(--cat-${c.id})"></i>${c.label}<b>${fmt(c.gb)}</b></li>`,
    )
    .join("");
  whenVisible(box, () => setTimeout(() => (box.dataset.state = "after"), reduced ? 0 : 350), 0.45);
}

// ---------- Demonstration ----------

function initStepper() {
  const tabs = $$('#stepper [role="tab"]');
  const show = (name, focus = false) => {
    for (const t of tabs) {
      const on = t.id === `tab-${name}`;
      t.setAttribute("aria-selected", String(on));
      t.tabIndex = on ? 0 : -1;
      $(`#${t.getAttribute("aria-controls")}`).hidden = !on;
      if (on && focus) t.focus();
    }
  };
  tabs.forEach((t, i) => {
    t.addEventListener("click", () => show(t.id.slice(4)));
    t.addEventListener("keydown", (e) => {
      const step = e.key === "ArrowRight" ? 1 : e.key === "ArrowLeft" ? -1 : 0;
      if (!step) return;
      e.preventDefault();
      show(tabs[(i + step + tabs.length) % tabs.length].id.slice(4), true);
    });
  });
  for (const b of $$("[data-goto]")) {
    b.addEventListener("click", () => {
      show(b.dataset.goto);
      $("#stepper").scrollIntoView({ block: "nearest", behavior: reduced ? "auto" : "smooth" });
    });
  }
}

function initScan() {
  const list = $("#scan-cats");
  const files = $("#scan-files");
  const bytes = $("#scan-bytes");
  const rec = $("#scan-rec");
  const bar = $("#scan-progress");
  const status = $("#scan-status");
  const max = CATEGORIES[0].gb;

  list.innerHTML = CATEGORIES.map(
    (c) =>
      `<li><i class="sw" style="--c:var(--cat-${c.id})"></i><span>${c.label}</span><span class="bar"><span style="--p:0;--c:var(--cat-${c.id})"></span></span><span class="sz">0 GB</span></li>`,
  ).join("");
  const rows = $$("li", list);

  let raf = 0;
  function paint(t) {
    // Categories fill at different paces, the way a real scan finds them.
    rows.forEach((row, i) => {
      const c = CATEGORIES[i];
      const local = Math.min(1, Math.max(0, (t - i * 0.05) / (0.7 + i * 0.03)));
      const gb = c.gb * local;
      row.querySelector(".bar span").style.setProperty("--p", ((gb / max) * 100).toFixed(1));
      row.querySelector(".sz").textContent = gb === 0 ? "0 GB" : fmt(gb);
    });
    const k = Math.min(1, t);
    files.textContent = Math.round(FILES * k).toLocaleString("en-US");
    bytes.textContent = fmt(SCANNED_GB * k);
    rec.textContent = fmt(RECOVERABLE_GB * Math.max(0, (k - 0.35) / 0.65));
    bar.style.setProperty("--p", (k * 100).toFixed(1));
  }

  function done() {
    paint(1.2);
    $("#scan-k").textContent = "Scanned";
    status.textContent = `Scan complete: ${fmt(SCANNED_GB)} measured, ${fmt(RECOVERABLE_GB)} likely recoverable.`;
  }

  function start() {
    cancelAnimationFrame(raf);
    if (reduced) return done();
    $("#scan-k").textContent = "Scanning";
    status.textContent = "Scanning. Read-only, nothing changes.";
    const t0 = performance.now();
    const step = (now) => {
      const t = Math.min(1.15, (now - t0) / 2600);
      paint(t);
      if (t < 1.15) raf = requestAnimationFrame(step);
      else done();
    };
    raf = requestAnimationFrame(step);
  }

  paint(0);
  $("#scan-run").addEventListener("click", start);
  whenVisible($("#console"), start, 0.35);
}

function initUnderstand() {
  const picker = $("#picker");
  const panel = $("#explain");
  let active = 0;

  picker.innerHTML = FOUND.map(
    (f, i) =>
      `<div role="option" tabindex="${i === 0 ? 0 : -1}" aria-selected="${i === 0}" data-i="${i}">
        <b><i class="t-dot ${f.tier}"></i>${esc(f.name)}</b><span class="sz">${fmt(f.gb)}</span>
        <code>${esc(f.path)}</code>
      </div>`,
  ).join("");
  const opts = $$('[role="option"]', picker);

  function render() {
    const f = FOUND[active];
    panel.innerHTML = `<div>
      ${badge(f.tier)}
      <h3>${esc(f.name)}</h3>
      <code class="path">${esc(f.path)}</code>
      <dl class="facts">
        <div><dt>What it contains</dt><dd>${esc(f.contains)}</dd></div>
        <div><dt>Created by</dt><dd>${esc(f.by)}</dd></div>
        <div><dt>Size</dt><dd>${fmt(f.gb)}</dd></div>
        <div><dt>Why it's there</dt><dd>${esc(f.why)}</dd></div>
        <div><dt>${f.tier === "keep" ? "Why it stays" : "If you remove it"}</dt><dd>${esc(f.verdict)}</dd></div>
      </dl>
    </div>`;
  }

  function select(i, focus) {
    active = i;
    opts.forEach((o, n) => {
      o.setAttribute("aria-selected", String(n === i));
      o.tabIndex = n === i ? 0 : -1;
    });
    if (focus) opts[i].focus();
    render();
  }

  picker.addEventListener("click", (e) => {
    const o = e.target.closest('[role="option"]');
    if (o) select(Number(o.dataset.i));
  });
  picker.addEventListener("keydown", (e) => {
    const step = { ArrowDown: 1, ArrowUp: -1 }[e.key];
    if (step) {
      e.preventDefault();
      select((active + step + FOUND.length) % FOUND.length, true);
    }
  });
  render();
}

function initClean() {
  const list = $("#rows");
  const filter = $$("#tier-filter [role='radio']");
  const label = $("#sel-label");
  const reviewBtn = $("#review-btn");
  const sheet = $("#sheet");
  const ok = $("#sheet-ok");
  const go = $("#sheet-go");
  const checked = new Set();
  let tier = "all";

  function renderRows() {
    list.innerHTML = RECS.map((r, i) => ({ r, i }))
      .filter(({ r }) => tier === "all" || r.tier === tier)
      .map(({ r, i }, n) => {
        const id = `rec-${i}`;
        const control =
          r.tier === "keep"
            ? `<svg class="lock" aria-hidden="true"><use href="#i-lock" /></svg>`
            : `<input type="checkbox" id="${id}" data-i="${i}" ${checked.has(i) ? "checked" : ""} />`;
        const text = `<b>${esc(r.name)}</b><code title="${esc(r.path)}">${esc(r.path)}</code>`;
        const name =
          r.tier === "keep"
            ? `<span class="name">${text}<span class="sr-only">. Never offered for cleanup.</span></span>`
            : `<label for="${id}">${text}</label>`;
        return `<li style="animation-delay:${n * 25}ms">${control}${name}${badge(r.tier)}<span class="sz">${fmt(r.gb)}</span></li>`;
      })
      .join("");
  }

  function renderFoot() {
    const total = [...checked].reduce((a, i) => a + RECS[i].gb, 0);
    label.textContent = checked.size
      ? `${checked.size} selected, ${fmt(total)}`
      : "Nothing selected. Pick items to review.";
    reviewBtn.disabled = checked.size === 0;
  }

  list.addEventListener("change", (e) => {
    const box = e.target.closest("input[data-i]");
    if (!box) return;
    const i = Number(box.dataset.i);
    if (box.checked) checked.add(i);
    else checked.delete(i);
    renderFoot();
  });

  function setTier(btn, focus) {
    for (const b of filter) {
      const on = b === btn;
      b.setAttribute("aria-checked", String(on));
      b.tabIndex = on ? 0 : -1;
    }
    if (focus) btn.focus();
    tier = btn.dataset.tier;
    renderRows();
  }
  filter.forEach((b, i) => {
    b.addEventListener("click", () => setTier(b));
    b.addEventListener("keydown", (e) => {
      const step = { ArrowRight: 1, ArrowDown: 1, ArrowLeft: -1, ArrowUp: -1 }[e.key];
      if (!step) return;
      e.preventDefault();
      setTier(filter[(i + step + filter.length) % filter.length], true);
    });
  });

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
  ok.addEventListener("change", () => (go.disabled = !ok.checked));
  go.addEventListener("click", () => {
    const items = [...checked].map((i) => RECS[i]);
    go.disabled = true;
    go.textContent = "Checking each path…";
    setTimeout(
      () => {
        const total = items.reduce((a, r) => a + r.gb, 0);
        $("#done-text").textContent =
          `${items.length} ${items.length === 1 ? "item" : "items"}, ${fmt(total)}, moved to the Trash in this demonstration.`;
        $("#sheet-review").hidden = true;
        $("#sheet-done").hidden = false;
        $("#sheet-reset").focus();
      },
      reduced ? 0 : 900,
    );
  });
  $("#sheet-reset").addEventListener("click", () => {
    checked.clear();
    sheet.hidden = true;
    setTier(filter[0]);
    renderFoot();
    reviewBtn.focus();
  });
  sheet.addEventListener("keydown", (e) => {
    if (e.key === "Escape") closeSheet();
    if (e.key !== "Tab") return;
    // Keep focus inside the sheet while it is open.
    const f = $$("button, input", sheet).filter((el) => !el.disabled && el.offsetParent);
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

  renderRows();
  renderFoot();
}

// ---------- Latest release, from GitHub ----------

async function initRelease() {
  const spots = $$("[data-release]");
  if (!spots.length) return;
  try {
    const res = await fetch(
      "https://api.github.com/repos/Devanshgoel-123/diskpilot/releases/latest",
      { headers: { Accept: "application/vnd.github+json" } },
    );
    if (!res.ok) return;
    const r = await res.json();
    if (typeof r.tag_name !== "string" || !/^v\d+\.\d+\.\d+$/.test(r.tag_name)) return;
    const date = new Date(r.published_at).toLocaleDateString("en-US", {
      month: "short",
      day: "numeric",
      year: "numeric",
    });
    for (const el of spots) {
      el.innerHTML = `Latest: <a href="https://github.com/Devanshgoel-123/diskpilot/releases/tag/${r.tag_name}" target="_blank" rel="noopener">${r.tag_name}</a>, ${esc(date)}.`;
      el.hidden = false;
    }
  } catch {
    /* offline or rate limited: the page works without it */
  }
}

initResolve();
initStepper();
initScan();
initUnderstand();
initClean();
initRelease();
