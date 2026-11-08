/**
 * The operating system DiskPilot runs on, and every word that differs
 * between macOS and Windows. Components read `words()` instead of writing
 * "Finder" or "Trash" themselves, so both platforms stay in one place.
 *
 * The OS is known before the first render: WebView2 reports "Windows" in its
 * user agent. The engine's `get_environment` confirms it at startup, and the
 * browser-only dev mode defaults to macOS.
 */

export type Os = "macos" | "windows";

function detect(): Os {
  try {
    return /Windows/i.test(navigator.userAgent) ? "windows" : "macos";
  } catch {
    return "macos";
  }
}

let current: Os = detect();

export function currentOs(): Os {
  return current;
}

export function isWindows(): boolean {
  return current === "windows";
}

/** Set from the engine's `os` value ("macos", "windows"); anything else is macOS. */
export function setOs(os: string | null | undefined): Os {
  current = os === "windows" ? "windows" : "macos";
  applyPlatformClass();
  return current;
}

/** `platform-windows` or `platform-macos` on <html>, for the font stack and window chrome. */
export function applyPlatformClass(): void {
  try {
    const cl = document.documentElement.classList;
    cl.toggle("platform-windows", current === "windows");
    cl.toggle("platform-macos", current === "macos");
  } catch {
    /* no document in some tests */
  }
}

export interface PlatformWords {
  /** The operating system's name. */
  os: string;
  /** The computer, as in "on this Mac" / "on this PC". */
  computer: string;
  /** Where moved files go, used after "the": "the Trash", "the Recycle Bin". */
  trash: string;
  /** The empty action's label, e.g. on the button. */
  emptyTrash: string;
  fileManager: string;
  /** Action label for showing a file in the file manager. */
  showInFileManager: string;
  /** The same action where the Mac app says "Reveal". */
  revealInFileManager: string;
  /** The system settings app. */
  systemSettings: string;
  /** Where the secret store keeps API keys, used after "in". */
  keyStore: string;
  /** Where DiskPilot sits when its window is closed, used after "in the". */
  tray: string;
  /** "log in" or "sign in". */
  logIn: string;
  /** The user's own folder, used after "your". */
  homeFolder: string;
  /** The app data folder, as shown in examples. */
  appData: string;
  /** Modifier key for keyboard shortcuts, as printed on keycaps. */
  modKey: string;
  /** The key that opens or confirms: "Return" on a Mac keyboard, "Enter" on a PC. */
  enterKey: string;
  /** A previewer for files, as in "like ... in Finder". */
  previewer: string;
  /** The keyboard shortcut handler checks this modifier. */
  modifier: "metaKey" | "ctrlKey";
}

const MAC: PlatformWords = {
  os: "macOS",
  computer: "Mac",
  trash: "Trash",
  emptyTrash: "Empty Trash",
  fileManager: "Finder",
  showInFileManager: "Show in Finder",
  revealInFileManager: "Reveal in Finder",
  systemSettings: "System Settings",
  keyStore: "the macOS Keychain",
  tray: "menu bar",
  logIn: "log in",
  homeFolder: "home folder",
  appData: "~/Library",
  modKey: "⌘",
  enterKey: "Return",
  previewer: "Quick Look",
  modifier: "metaKey",
};

const WINDOWS: PlatformWords = {
  os: "Windows",
  computer: "PC",
  trash: "Recycle Bin",
  emptyTrash: "Empty Recycle Bin",
  fileManager: "File Explorer",
  showInFileManager: "Show in File Explorer",
  revealInFileManager: "Show in File Explorer",
  systemSettings: "Settings",
  keyStore: "Windows Credential Manager",
  tray: "notification area",
  logIn: "sign in",
  homeFolder: "user folder",
  appData: "AppData",
  modKey: "Ctrl",
  enterKey: "Enter",
  previewer: "the preview pane",
  modifier: "ctrlKey",
};

export function words(os: Os = current): PlatformWords {
  return os === "windows" ? WINDOWS : MAC;
}

/** Whether the platform's shortcut modifier (⌘ or Ctrl) is held. */
export function modifierHeld(e: { metaKey: boolean; ctrlKey: boolean }, os: Os = current): boolean {
  return e[words(os).modifier];
}
