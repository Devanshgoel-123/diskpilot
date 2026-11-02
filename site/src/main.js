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
