import { invoke, isTauri } from "@tauri-apps/api/core";

import { isWindows } from "./platform";

export type ThemePreference = "dark" | "light" | "system";

const KEY = "diskpilot.theme";

export function loadTheme(): ThemePreference {
  try {
    const v = localStorage.getItem(KEY);
    if (v === "dark" || v === "light" || v === "system") return v;
  } catch {
    /* storage unavailable */
  }
  return "dark";
}

export function saveTheme(t: ThemePreference): void {
  try {
    localStorage.setItem(KEY, t);
  } catch {
    /* storage unavailable */
  }
}

export function applyTheme(t: ThemePreference): void {
  const dark =
    t === "dark" || (t === "system" && window.matchMedia("(prefers-color-scheme: dark)").matches);
  document.documentElement.classList.toggle("dark", dark);
  // The Windows title bar is drawn by the system; match it to the app.
  // The same call `getCurrentWindow().setTheme()` makes, without loading the
  // whole window API for one Windows-only setting.
  if (isWindows() && isTauri()) {
    const internals = (
      window as { __TAURI_INTERNALS__?: { metadata?: { currentWindow?: { label?: string } } } }
    ).__TAURI_INTERNALS__;
    invoke("plugin:window|set_theme", {
      label: internals?.metadata?.currentWindow?.label ?? "main",
      value: t === "system" ? null : t,
    }).catch(() => undefined);
  }
}
