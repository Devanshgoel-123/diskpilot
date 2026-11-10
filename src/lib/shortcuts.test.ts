import { beforeEach, describe, expect, it, vi } from "vitest";

import { setOs } from "./platform";
import {
  SHORTCUT_GROUPS,
  actionText,
  ariaKeys,
  focusFilter,
  keyLabel,
  keysText,
  runShortcut,
  shortcutFor,
  shortcutLabel,
  useShortcutsDialog,
} from "./shortcuts";

const key = (k: string, mods: Partial<KeyboardEvent> = {}) =>
  new KeyboardEvent("keydown", { key: k, metaKey: true, ...mods });

function app(over: Partial<Parameters<typeof runShortcut>[1]> = {}) {
  return {
    navigate: vi.fn(),
    scanning: false,
    desktop: true,
    startScan: vi.fn(() => Promise.resolve()),
    cancelScan: vi.fn(() => Promise.resolve()),
    ...over,
  };
}

beforeEach(() => {
  setOs("macos");
  document.body.innerHTML = "";
  useShortcutsDialog.setState({ open: false });
});

describe("shortcuts", () => {
  it("has one action per key combination", () => {
    const combos = SHORTCUT_GROUPS.flatMap((g) => g.items)
      .filter((s) => s.page || s.command)
      .map((s) => s.keys.join("+"));
    expect(new Set(combos).size).toBe(combos.length);
  });

  it("maps Command shortcuts only", () => {
    expect(shortcutFor(key("2"))?.page).toBe("explorer");
    expect(shortcutFor(key("r"))?.command).toBe("scan");
    expect(shortcutFor(key(","))?.page).toBe("settings");
    expect(shortcutFor(key("2", { metaKey: false }))).toBeNull();
    expect(shortcutFor(key("r", { shiftKey: true }))).toBeNull();
    // Handled by the preview itself, not globally.
    expect(shortcutFor(key("Backspace"))).toBeNull();
    expect(shortcutFor(key("o"))).toBeNull();
  });

  it("labels pages for tooltips", () => {
    expect(shortcutLabel("overview")).toBe("⌘1");
    expect(shortcutLabel("assistant")).toBeNull();
    expect(ariaKeys(["⌘", "R"])).toBe("Meta+R");
    expect(actionText("Select or unselect for the Trash")).toBe("Select or unselect for the Trash");
  });

  it("uses Ctrl and PC key names on Windows", () => {
    setOs("windows");
    expect(shortcutFor(key("2", { metaKey: false, ctrlKey: true }))?.page).toBe("explorer");
    expect(shortcutFor(key("r", { metaKey: false, ctrlKey: true }))?.command).toBe("scan");
    // The Windows key is not the shortcut modifier.
    expect(shortcutFor(key("2"))).toBeNull();
    expect(shortcutFor(key("2", { ctrlKey: true }))).toBeNull();
    expect(shortcutLabel("overview")).toBe("Ctrl+1");
    expect(keysText(["⌘", "⌫"])).toBe("Ctrl+Backspace");
    expect(keyLabel("⌘")).toBe("Ctrl");
    expect(ariaKeys(["⌘", "R"])).toBe("Control+R");
    expect(actionText("Select or unselect for the Trash")).toBe(
      "Select or unselect for the Recycle Bin",
    );
  });

  it("goes to pages and scans, but not while a window is open", () => {
    const a = app();
    expect(runShortcut(key("7"), a)).toBe(true);
    expect(a.navigate).toHaveBeenCalledWith("recommendations");
    runShortcut(key("r"), a);
    expect(a.startScan).toHaveBeenCalledTimes(1);

    document.body.innerHTML = '<div role="dialog"></div>';
    expect(runShortcut(key("1"), a)).toBe(false);
    expect(a.navigate).toHaveBeenCalledTimes(1);
  });

  it("does not start a scan while one runs, and stops it with ⌘.", () => {
    const a = app({ scanning: true });
    runShortcut(key("r"), a);
    expect(a.startScan).not.toHaveBeenCalled();
    runShortcut(key("."), a);
    expect(a.cancelScan).toHaveBeenCalledTimes(1);
  });

  it("toggles the shortcuts window with ⌘/, even over another window", () => {
    document.body.innerHTML = '<div role="dialog"></div>';
    expect(runShortcut(key("/"), app())).toBe(true);
    expect(useShortcutsDialog.getState().open).toBe(true);
    runShortcut(key("/"), app());
    expect(useShortcutsDialog.getState().open).toBe(false);
  });

  it("focuses the page's filter field, and passes ⌘F on when there is none", () => {
    expect(runShortcut(key("f"), app())).toBe(false);
    document.body.innerHTML = '<input data-filter value="abc" />';
    const input = document.querySelector("input") as HTMLInputElement;
    // jsdom has no layout; pretend the field is showing.
    input.getClientRects = () => [{} as DOMRect] as unknown as DOMRectList;
    expect(focusFilter()).toBe(true);
    expect(document.activeElement).toBe(input);
  });
});
