/**
 * Path helpers for display and navigation. The engine sends native paths:
 * `/Users/me/Downloads` on macOS, `C:\Users\me\Downloads` on Windows, where
 * a long path may also carry the `\\?\` prefix. Every helper here works on
 * both forms, so components never split on "/" themselves.
 */

const VERBATIM_UNC = /^\\\\\?\\UNC\\/i;
const VERBATIM = /^\\\\\?\\/;
const DRIVE = /^[A-Za-z]:(?:[\\/]|$)/;

/** A Windows path: a drive letter, a UNC share or a `\\?\` path. */
export function isWindowsPath(path: string): boolean {
  return DRIVE.test(path) || path.startsWith("\\\\");
}

/** Drops the `\\?\` prefix Windows adds to long paths; `\\?\UNC\` becomes `\\`. */
export function stripVerbatim(path: string): string {
  if (VERBATIM_UNC.test(path)) return "\\\\" + path.slice(8);
  if (VERBATIM.test(path)) return path.slice(4);
  return path;
}

/** The separator a path uses: `\` for Windows paths, `/` otherwise. */
export function separatorOf(path: string): "\\" | "/" {
  return isWindowsPath(stripVerbatim(path)) ? "\\" : "/";
}

/**
 * The root of a path and the folder names after it. The root is `/` on
 * macOS, `C:\` for a drive and `\\server\share\` for a share; it is ""
 * for a relative path.
 */
export function splitPath(path: string): { root: string; parts: string[] } {
  const p = stripVerbatim(path);
  if (!isWindowsPath(p)) {
    return {
      root: p.startsWith("/") ? "/" : "",
      parts: p.split("/").filter(Boolean),
    };
  }
  const segments = p.split(/[\\/]/);
  if (p.startsWith("\\\\")) {
    // ["", "", server, share, ...rest]
    const [server = "", share = ""] = segments.slice(2);
    return {
      root: `\\\\${server}\\${share}${share ? "\\" : ""}`,
      parts: segments.slice(4).filter(Boolean),
    };
  }
  return {
    root: `${(segments[0] ?? "").toUpperCase()}\\`,
    parts: segments.slice(1).filter(Boolean),
  };
}

/** `base` and `name` joined with the right separator. */
export function joinPath(base: string, name: string): string {
  const sep = separatorOf(base);
  return base.endsWith(sep) || base.endsWith("/") ? base + name : base + sep + name;
}

/** The last name in a path, or the root itself (such as `C:\`) for a root. */
export function basename(path: string): string {
  const { root, parts } = splitPath(path);
  return parts[parts.length - 1] ?? (root || path);
}

/** The folder that holds `path`; a root is its own parent. */
export function parentOf(path: string): string {
  const { root, parts } = splitPath(path);
  if (parts.length <= 1) return root || path;
  const sep = root === "/" || !root ? "/" : "\\";
  return root + parts.slice(0, -1).join(sep);
}

/** Path comparison key: Windows paths ignore case and separator style. */
function key(path: string): string {
  const p = stripVerbatim(path);
  return isWindowsPath(p) ? p.replace(/\//g, "\\").toLowerCase() : p;
}

export function samePath(a: string, b: string): boolean {
  return trimEnd(key(a)) === trimEnd(key(b));
}

/** Without a trailing separator, except for a root such as `/` or `C:\`. */
function trimEnd(p: string): string {
  if (p === "/" || /^[a-z]:\\$/i.test(p)) return p;
  return p.replace(/[\\/]+$/, "");
}

/** True if `child` is strictly inside `parent` (name by name, not by prefix). */
export function isInside(child: string, parent: string): boolean {
  const c = trimEnd(key(child));
  const p = trimEnd(key(parent));
  if (c === p) return false;
  const sep = isWindowsPath(stripVerbatim(parent)) ? "\\" : "/";
  return c.startsWith(p.endsWith(sep) ? p : p + sep);
}

/** `path` relative to `parent` as names, or `null` when it is not inside. */
export function relativeParts(path: string, parent: string): string[] | null {
  if (samePath(path, parent)) return [];
  if (!isInside(path, parent)) return null;
  return splitPath(path).parts.slice(splitPath(parent).parts.length);
}

/**
 * Replaces the home folder with `~` for compact display: `~/Downloads` on
 * macOS and `~\Downloads` on Windows (PowerShell reads `~` the same way).
 * Other paths are shown in full, without a `\\?\` prefix.
 */
export function tildify(path: string, home: string | undefined): string {
  const shown = stripVerbatim(path);
  if (!home) return shown;
  const rel = relativeParts(shown, home);
  if (rel === null) return shown;
  if (!rel.length) return "~";
  const sep = separatorOf(home);
  return "~" + sep + rel.join(sep);
}
