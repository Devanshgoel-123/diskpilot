/** Release notes from docs/releases, bundled so "What's new" works offline. */
const files = import.meta.glob<string>("../../docs/releases/*.md", {
  query: "?raw",
  import: "default",
  eager: true,
});

/**
 * Drops sections about downloading, verifying or installing, at any heading
 * level, since those only make sense on GitHub. A dropped section ends at the
 * next heading of the same or a higher level.
 */
export function inAppNotes(markdown: string): string {
  const out: string[] = [];
  let skipLevel: number | null = null;
  let inCode = false;
  for (const line of markdown.split("\n")) {
    if (line.startsWith("```")) inCode = !inCode;
    const heading = inCode ? null : /^(#{2,6}) (.*)$/.exec(line);
    if (heading) {
      const [, hashes = "", title = ""] = heading;
      const level = hashes.length;
      if (skipLevel !== null && level <= skipLevel) skipLevel = null;
      if (skipLevel === null && /^(Download|Verify|Install)/.test(title)) skipLevel = level;
    }
    if (skipLevel === null) out.push(line);
  }
  return out.join("\n").trim();
}

/** Notes for a version such as "0.5.1", or null if none were bundled. */
export function releaseNotes(version: string): string | null {
  const entry = Object.entries(files).find(([path]) => path.endsWith(`/v${version}.md`));
  return entry ? inAppNotes(entry[1]) : null;
}

/** Whether version `a` (such as "0.6.1") is newer than `b`, comparing numbers. */
export function isNewerVersion(a: string, b: string): boolean {
  const parts = (v: string) =>
    v
      .split(/[.+-]/)
      .slice(0, 3)
      .map((n) => Number.parseInt(n, 10) || 0);
  const [x, y] = [parts(a), parts(b)];
  for (let i = 0; i < 3; i++) {
    if ((x[i] ?? 0) !== (y[i] ?? 0)) return (x[i] ?? 0) > (y[i] ?? 0);
  }
  return false;
}

/**
 * Drops the section under the heading `title` (any level), up to the next
 * heading of the same or a higher level.
 */
export function withoutSection(markdown: string, title: string): string {
  const out: string[] = [];
  let skipLevel: number | null = null;
  let inCode = false;
  for (const line of markdown.split("\n")) {
    if (line.startsWith("```")) inCode = !inCode;
    const heading = inCode ? null : /^(#{2,6}) (.*)$/.exec(line);
    if (heading) {
      const level = (heading[1] ?? "").length;
      if (skipLevel !== null && level <= skipLevel) skipLevel = null;
      if (skipLevel === null && heading[2]?.trim() === title) skipLevel = level;
    }
    if (skipLevel === null) out.push(line);
  }
  return out.join("\n").trim();
}

/**
 * Splits notes into their opening paragraph, shown larger, and the rest. A
 * heading with nothing under it but smaller headings (such as "What's
 * changed") is left out of the rest, since the dialog's title already says it.
 */
export function splitNotes(markdown: string): { lead: string | null; rest: string } {
  const text = markdown.trim();
  let lead: string | null = null;
  let body = text;
  if (!/^(#|- |\* |\d+\. |```)/.test(text)) {
    const end = text.indexOf("\n\n");
    lead = (end === -1 ? text : text.slice(0, end)).trim();
    body = end === -1 ? "" : text.slice(end + 2);
  }
  const lines = body.split("\n");
  const kept = lines.filter((line, i) => {
    const h = /^(#{2,6}) /.exec(line);
    if (!h) return true;
    const next = lines.slice(i + 1).find((l) => l.trim() !== "");
    const nh = next ? /^(#{2,6}) /.exec(next) : null;
    return !(nh && (nh[1] ?? "").length > (h[1] ?? "").length);
  });
  return { lead, rest: kept.join("\n").trim() };
}
