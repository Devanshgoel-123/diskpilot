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
