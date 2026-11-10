import { create } from "zustand";

import data from "./shortcuts.json";

import { isWindows, modifierHeld, words } from "@/lib/platform";
import type { PageId } from "@/lib/store";

export interface Shortcut {
  keys: string[];
  action: string;
  /** A page to open. */
  page?: PageId;
  /** Something else the shortcut does. */
  command?: "scan" | "stop" | "filter" | "help";
  since: string;
}

export interface ShortcutGroup {
  title: string;
  items: Shortcut[];
}

/** Every shortcut, grouped as the Keyboard Shortcuts window shows them. */
export const SHORTCUT_GROUPS = data.groups as ShortcutGroup[];

const ALL = SHORTCUT_GROUPS.flatMap((g) => g.items);

/** Shortcuts that first came in `version`, in the order the shortcuts window lists them. */
export function shortcutsNewIn(version: string | undefined): Shortcut[] {
  return version ? ALL.filter((s) => s.since === version) : [];
}

/**
 * A key as printed on this platform's keyboard. Shortcuts are written with
 * the Mac's ⌘ and ⌫; on Windows they read Ctrl and Backspace.
 */
export function keyLabel(key: string): string {
  if (key === "⌘") return words().modKey;
  if (key === "⌫" && isWindows()) return "Backspace";
  return key;
}

/** "⌘R" on a Mac, "Ctrl+R" on Windows, for tooltips and descriptions. */
export function keysText(keys: string[]): string {
  return keys.map(keyLabel).join(isWindows() ? "+" : "");
}

/** The keys in `aria-keyshortcuts` form: "Meta+R" on a Mac, "Control+R" on Windows. */
export function ariaKeys(keys: string[]): string {
  return keys
    .map((k) => (k === "⌘" ? (isWindows() ? "Control" : "Meta") : k === "⌫" ? "Backspace" : k))
    .join("+");
}

/** Text for a shortcut's action, with "the Trash" as the platform calls it. */
export function actionText(action: string): string {
  return action.replace(/\bTrash\b/g, words().trash);
}

/**
 * The page or command for a key press with the shortcut modifier held (⌘ on
 * a Mac, Ctrl on Windows), if it is one of DiskPilot's shortcuts. Only these
 * shortcuts are global; Space and the arrow keys belong to the list or
 * preview that has focus.
 */
export function shortcutFor(
  e: Pick<KeyboardEvent, "key" | "metaKey" | "ctrlKey" | "altKey" | "shiftKey">,
) {
  const other = isWindows() ? e.metaKey : e.ctrlKey;
  if (!modifierHeld(e) || other || e.altKey || e.shiftKey) return null;
  const key = e.key.length === 1 ? e.key.toUpperCase() : e.key;
  return (
    ALL.find(
      (s) => (s.page || s.command) && s.keys.length === 2 && s.keys[0] === "⌘" && s.keys[1] === key,
    ) ?? null
  );
}

/** Focuses the filter field of the page that is showing, if it has one. */
export function focusFilter(root: ParentNode = document): boolean {
  const field = [...root.querySelectorAll<HTMLInputElement>("input[data-filter]")].find(
    (el) => el.offsetParent !== null || el.getClientRects().length > 0,
  );
  if (!field) return false;
  field.focus();
  field.select();
  return true;
}

/** The keys of a page's shortcut, if it has one. */
export function shortcutKeys(page: PageId): string[] | null {
  return ALL.find((x) => x.page === page)?.keys ?? null;
}

/** "⌘2" (Ctrl+2 on Windows) for a page that has a shortcut, for tooltips. */
export function shortcutLabel(page: PageId): string | null {
  const keys = shortcutKeys(page);
  return keys ? keysText(keys) : null;
}

/** Whether the Keyboard Shortcuts window is open. */
export const useShortcutsDialog = create<{ open: boolean; setOpen: (open: boolean) => void }>(
  (set) => ({ open: false, setOpen: (open) => set({ open }) }),
);

/**
 * Runs the app-wide shortcut for a key press, if there is one. Returns
 * whether it was handled. A window that is open (a preview, a review) keeps
 * the keyboard, except for ⌘/ which shows the shortcuts.
 */
export function runShortcut(
  e: KeyboardEvent,
  app: {
    navigate: (page: PageId) => void;
    scanning: boolean;
    desktop: boolean;
    startScan: () => Promise<void>;
    cancelScan: () => Promise<void>;
  },
): boolean {
  const s = shortcutFor(e);
  if (!s) return false;
  const dialogOpen = !!document.querySelector('[role="dialog"], [role="alertdialog"]');
  const help = useShortcutsDialog.getState();
  if (s.command === "help") {
    help.setOpen(!help.open);
    return true;
  }
  if (dialogOpen) return false;
  if (s.page) app.navigate(s.page);
  else if (s.command === "scan") {
    if (app.desktop && !app.scanning) void app.startScan();
  } else if (s.command === "stop") {
    if (app.scanning) void app.cancelScan();
  } else if (s.command === "filter") {
    if (!focusFilter()) return false;
  }
  return true;
}
