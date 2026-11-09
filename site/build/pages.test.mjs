// Run with `node --test site/build/pages.test.mjs` (CI runs it before building the site).
import assert from "node:assert/strict";
import { test } from "node:test";

import {
  compareVersions,
  frontMatter,
  renderMarkdown,
  roundDown,
  slug,
  withoutInstallSections,
} from "./pages.mjs";

test("download counts round down, never up", () => {
  assert.equal(roundDown(7), "7");
  assert.equal(roundDown(299), "290+");
  assert.equal(roundDown(300), "300+");
  assert.equal(roundDown(1999), "1.9k+");
});

test("versions compare as numbers", () => {
  assert.ok(compareVersions("v0.6.10", "v0.6.2") > 0);
  assert.equal(compareVersions("0.6.1", "v0.6.1"), 0);
  assert.ok(compareVersions("0.6.3", "0.6.2") > 0);
});

test("front matter is read and removed from the body", () => {
  const { meta, body } = frontMatter("---\ntitle: A: B\norder: 2\n---\n\nHello");
  assert.equal(meta.title, "A: B");
  assert.equal(meta.order, "2");
  assert.equal(body.trim(), "Hello");
});

test("release notes lose their download and install sections only", () => {
  const md = [
    "Intro.",
    "## Download",
    "Get it.",
    "## What's changed",
    "### Files",
    "- A change",
    "### Install with Homebrew",
    "```sh",
    "## not a heading",
    "```",
  ].join("\n");
  const out = withoutInstallSections(md);
  assert.match(out, /Intro\./);
  assert.match(out, /A change/);
  assert.doesNotMatch(out, /Get it|Homebrew|not a heading/);
});

test("markdown gets heading ids, safe external links and scrollable tables", () => {
  const { html, headings } = renderMarkdown(
    "## Free space safely\n\n[Docker](https://docs.docker.com) and [a guide](/guides/x)\n\n| a | b |\n|---|---|\n| 1 | 2 |",
  );
  assert.deepEqual(headings, [{ id: "free-space-safely", text: "Free space safely" }]);
  assert.match(html, /<h2 id="free-space-safely">/);
  assert.match(html, /href="https:\/\/docs.docker.com" target="_blank" rel="noopener"/);
  assert.match(html, /<a href="\/guides\/x">a guide<\/a>/);
  assert.match(html, /<div class="doc-table"><table>/);
});

test("slugs are stable", () => {
  assert.equal(slug("What is <code>System Data</code>?"), "what-is-system-data");
  assert.equal(slug("v0.6.2"), "v0-6-2");
});
