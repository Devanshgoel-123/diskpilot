import "@fontsource-variable/bricolage-grotesque";
import "@fontsource/ibm-plex-sans/400.css";
import "@fontsource/ibm-plex-sans/500.css";
import "@fontsource/ibm-plex-sans/600.css";
import "@fontsource/ibm-plex-mono/400.css";
import "@fontsource/ibm-plex-mono/500.css";
import "./styles.css";

import { inject, track } from "@vercel/analytics";

// Vercel Web Analytics: cookieless page views. In development the package
// logs events to the console instead of sending them.
inject();

// Count clicks on the GitHub links by where they were placed. Custom events
// appear in the Vercel dashboard on plans that include them.
document.querySelectorAll("[data-track]").forEach((link) => {
  link.addEventListener("click", () => {
    track(link.dataset.track, { where: link.dataset.where ?? "unknown" });
  });
});

// App window tabs (WAI-ARIA tabs pattern).
const tabs = [...document.querySelectorAll('.tabs [role="tab"]')];

function select(tab, focus) {
  for (const t of tabs) {
    const on = t === tab;
    t.setAttribute("aria-selected", String(on));
    t.tabIndex = on ? 0 : -1;
    document.getElementById(t.getAttribute("aria-controls")).hidden = !on;
  }
  if (focus) tab.focus();
}

tabs.forEach((tab, i) => {
  tab.addEventListener("click", () => select(tab, false));
  tab.addEventListener("keydown", (e) => {
    const step = e.key === "ArrowRight" ? 1 : e.key === "ArrowLeft" ? -1 : 0;
    if (!step) return;
    e.preventDefault();
    select(tabs[(i + step + tabs.length) % tabs.length], true);
  });
});

// Theme switcher: system, then light, then dark. The choice is remembered
// in this browser only.
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
