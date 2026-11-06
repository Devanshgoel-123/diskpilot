import { describe, expect, it } from "vitest";

import { parseInline, parseMarkdown } from "./markdown";

describe("parseInline", () => {
  it("handles bold and code", () => {
    expect(parseInline("Run `cargo clean` for **5 GB**.")).toEqual([
      { kind: "text", text: "Run " },
      { kind: "code", text: "cargo clean" },
      { kind: "text", text: " for " },
      { kind: "bold", text: "5 GB" },
      { kind: "text", text: "." },
    ]);
  });
  it("leaves HTML as plain text", () => {
    expect(parseInline("<img src=x onerror=alert(1)>")).toEqual([
      { kind: "text", text: "<img src=x onerror=alert(1)>" },
    ]);
  });
});

describe("parseMarkdown", () => {
  it("splits paragraphs, lists, headings and code blocks", () => {
    const blocks = parseMarkdown(
      "## Summary\nYour largest item\nis Docker.\n\n- one\n- two\n\n1. first\n2. second\n\n```\nnpm cache clean --force\n```",
    );
    expect(blocks.map((b) => b.kind)).toEqual([
      "heading",
      "paragraph",
      "list",
      "list",
      "codeblock",
    ]);
    expect(blocks[1]).toEqual({
      kind: "paragraph",
      inlines: [{ kind: "text", text: "Your largest item is Docker." }],
    });
    expect(blocks[3]).toMatchObject({ kind: "list", ordered: true });
    expect(blocks[4]).toEqual({ kind: "codeblock", text: "npm cache clean --force" });
  });
});

describe("links", () => {
  it("show as their text, never as links", () => {
    expect(parseInline("by [@chinmayajha](https://github.com/chinmayajha), thanks")).toEqual([
      { kind: "text", text: "by @chinmayajha, thanks" },
    ]);
  });
});
