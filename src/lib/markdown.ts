/**
 * A deliberately small Markdown subset for assistant answers: paragraphs,
 * bullet and numbered lists, headings, **bold** and `code`. Output is a tree
 * rendered as React elements, never as raw HTML.
 */

export type Inline = { kind: "text" | "bold" | "code"; text: string };

export type Block =
  | { kind: "paragraph"; inlines: Inline[] }
  | { kind: "heading"; inlines: Inline[] }
  | { kind: "list"; ordered: boolean; items: Inline[][] }
  | { kind: "codeblock"; text: string };

export function parseInline(raw: string): Inline[] {
  // Links show as their text; this renderer never makes clickable links.
  const text = raw.replace(/\[([^\]]+)\]\([^)\s]+\)/g, "$1");
  const out: Inline[] = [];
  const re = /(\*\*[^*]+\*\*|`[^`]+`)/g;
  let last = 0;
  for (const m of text.matchAll(re)) {
    const idx = m.index ?? 0;
    if (idx > last) out.push({ kind: "text", text: text.slice(last, idx) });
    const tok = m[0];
    out.push(
      tok.startsWith("**")
        ? { kind: "bold", text: tok.slice(2, -2) }
        : { kind: "code", text: tok.slice(1, -1) },
    );
    last = idx + tok.length;
  }
  if (last < text.length) out.push({ kind: "text", text: text.slice(last) });
  return out;
}

const BULLET = /^\s*[-*•]\s+(.*)$/;
const NUMBERED = /^\s*\d+[.)]\s+(.*)$/;

export function parseMarkdown(src: string): Block[] {
  const blocks: Block[] = [];
  const lines = src.replace(/\r\n/g, "\n").split("\n");
  let i = 0;
  while (i < lines.length) {
    const line = lines[i] ?? "";
    if (line.trim() === "") {
      i++;
      continue;
    }
    if (line.trim().startsWith("```")) {
      const body: string[] = [];
      i++;
      while (i < lines.length && !(lines[i] ?? "").trim().startsWith("```"))
        body.push(lines[i++] ?? "");
      i++;
      blocks.push({ kind: "codeblock", text: body.join("\n") });
      continue;
    }
    const heading = /^#{1,4}\s+(.*)$/.exec(line);
    if (heading) {
      blocks.push({ kind: "heading", inlines: parseInline(heading[1] ?? "") });
      i++;
      continue;
    }
    const ordered = NUMBERED.test(line);
    if (ordered || BULLET.test(line)) {
      const re = ordered ? NUMBERED : BULLET;
      const items: Inline[][] = [];
      while (i < lines.length && re.test(lines[i] ?? "")) {
        items.push(parseInline(re.exec(lines[i] ?? "")?.[1] ?? ""));
        i++;
      }
      blocks.push({ kind: "list", ordered, items });
      continue;
    }
    const para: string[] = [];
    while (
      i < lines.length &&
      (lines[i] ?? "").trim() !== "" &&
      !BULLET.test(lines[i] ?? "") &&
      !NUMBERED.test(lines[i] ?? "") &&
      !(lines[i] ?? "").trim().startsWith("```")
    ) {
      para.push((lines[i] ?? "").trim());
      i++;
    }
    blocks.push({ kind: "paragraph", inlines: parseInline(para.join(" ")) });
  }
  return blocks;
}
