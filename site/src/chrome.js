// Shared by every page: fonts, styles, analytics, the nav, the theme switch
// and copy buttons on code blocks.
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
  // Screenshots are named name-dark.webp / name-dark-1200.webp; swap the
  // theme word in src and srcset so the responsive sizes keep working.
  const swap = (value) => value.replace(/-(dark|light)(?=[.-])/g, `-${theme}`);
  for (const el of $$("img[data-shot], source[data-shot-m]")) {
    for (const attr of ["src", "srcset"]) {
      const value = el.getAttribute(attr);
      if (value && swap(value) !== value) el.setAttribute(attr, swap(value));
    }
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

// ---------- Copy buttons on commands ----------

for (const pre of $$("pre")) {
  const code = $("code", pre);
  if (!code || pre.querySelector(".copy-btn")) continue;
  const btn = document.createElement("button");
  btn.type = "button";
  btn.className = "copy-btn";
  btn.setAttribute("aria-label", "Copy to clipboard");
  btn.innerHTML =
    '<svg class="ico" aria-hidden="true"><use href="#i-copy" /></svg><span>Copy</span>';
  btn.addEventListener("click", async () => {
    try {
      await navigator.clipboard.writeText(code.innerText.trim());
      btn.classList.add("done");
      btn.querySelector("span").textContent = "Copied";
      btn.querySelector("use").setAttribute("href", "#i-check");
      track("copy", { where: location.pathname });
      setTimeout(() => {
        btn.classList.remove("done");
        btn.querySelector("span").textContent = "Copy";
        btn.querySelector("use").setAttribute("href", "#i-copy");
      }, 1600);
    } catch {
      /* clipboard unavailable: the text can still be selected */
    }
  });
  pre.classList.add("has-copy");
  pre.append(btn);
}
