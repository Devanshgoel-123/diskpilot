import { getCurrentWindow } from "@tauri-apps/api/window";
import { isTauri } from "@tauri-apps/api/core";

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
  if (isWindows() && isTauri()) {
    getCurrentWindow()
      .setTheme(t === "system" ? null : t)
      .catch(() => undefined);
  }
}
