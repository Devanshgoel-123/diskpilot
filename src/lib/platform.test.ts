import { afterEach, describe, expect, it } from "vitest";

import { currentOs, isWindows, modifierHeld, setOs, words } from "./platform";

afterEach(() => {
  setOs("macos");
});

describe("platform", () => {
  it("defaults to macOS outside WebView2", () => {
    expect(currentOs()).toBe("macos");
    expect(isWindows()).toBe(false);
  });

  it("takes the engine's os value; anything else is macOS", () => {
    expect(setOs("windows")).toBe("windows");
    expect(isWindows()).toBe(true);
    expect(document.documentElement.classList.contains("platform-windows")).toBe(true);
    expect(setOs("linux")).toBe("macos");
    expect(document.documentElement.classList.contains("platform-windows")).toBe(false);
    expect(document.documentElement.classList.contains("platform-macos")).toBe(true);
  });

  it("uses Mac words on macOS", () => {
    const w = words("macos");
    expect(w.trash).toBe("Trash");
    expect(w.emptyTrash).toBe("Empty Trash");
    expect(w.showInFileManager).toBe("Show in Finder");
    expect(w.revealInFileManager).toBe("Reveal in Finder");
    expect(w.computer).toBe("Mac");
    expect(w.tray).toBe("menu bar");
    expect(w.modKey).toBe("⌘");
    expect(w.enterKey).toBe("Return");
  });

  it("uses Windows words on Windows", () => {
    const w = words("windows");
    expect(w.trash).toBe("Recycle Bin");
    expect(w.emptyTrash).toBe("Empty Recycle Bin");
    expect(w.showInFileManager).toBe("Show in File Explorer");
    expect(w.revealInFileManager).toBe("Show in File Explorer");
    expect(w.computer).toBe("PC");
    expect(w.tray).toBe("notification area");
    expect(w.homeFolder).toBe("user folder");
    expect(w.modKey).toBe("Ctrl");
    expect(w.enterKey).toBe("Enter");
  });

  it("follows the current OS by default", () => {
    setOs("windows");
    expect(words().trash).toBe("Recycle Bin");
    setOs("macos");
    expect(words().trash).toBe("Trash");
  });

  it("never names a Mac thing on Windows, and keeps the text plain", () => {
    for (const value of Object.values(words("windows"))) {
      expect(value).not.toMatch(/Finder|Trash\b|Mac\b|macOS|⌘|Keychain|menu bar|Library/);
      expect(value).not.toContain("!");
    }
  });

  it("checks Ctrl on Windows and Command on macOS for shortcuts", () => {
    const ctrl = { metaKey: false, ctrlKey: true };
    const cmd = { metaKey: true, ctrlKey: false };
    expect(modifierHeld(ctrl, "windows")).toBe(true);
    expect(modifierHeld(cmd, "windows")).toBe(false);
    expect(modifierHeld(cmd, "macos")).toBe(true);
    expect(modifierHeld(ctrl, "macos")).toBe(false);
  });
});
