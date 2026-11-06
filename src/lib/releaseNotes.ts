/** Release notes from docs/releases, bundled so "What's new" works offline. */
const files = import.meta.glob<string>("../../docs/releases/*.md", {
  query: "?raw",
  import: "default",
  eager: true,
});

/** Drops sections about downloading, which only make sense on GitHub. */
export function inAppNotes(markdown: string): string {
  const out: string[] = [];
  let skipping = false;
  for (const line of markdown.split("\n")) {
    if (line.startsWith("## ")) skipping = /^## (Download|Verify|Install)/.test(line);
    if (!skipping) out.push(line);
  }
  return out.join("\n").trim();
}

/** Notes for a version such as "0.5.1", or null if none were bundled. */
export function releaseNotes(version: string): string | null {
  const entry = Object.entries(files).find(([path]) => path.endsWith(`/v${version}.md`));
  return entry ? inAppNotes(entry[1]) : null;
}
