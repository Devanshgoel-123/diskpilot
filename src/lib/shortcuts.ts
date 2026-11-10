import { create } from "zustand";

import data from "./shortcuts.json";

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
 * The page or command for a key press with Command held, if it is one of
 * DiskPilot's shortcuts. Only Command shortcuts are global; Space and the
 * arrow keys belong to the list or preview that has focus.
 */
export function shortcutFor(
  e: Pick<KeyboardEvent, "key" | "metaKey" | "ctrlKey" | "altKey" | "shiftKey">,
) {
  if (!e.metaKey || e.ctrlKey || e.altKey || e.shiftKey) return null;
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

/** "⌘2" for a page that has a shortcut, for tooltips. */
export function shortcutLabel(page: PageId): string | null {
  const s = ALL.find((x) => x.page === page);
  return s ? s.keys.join("") : null;
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
