import { describe, expect, it } from "vitest";

import { inAppNotes, isNewerVersion, releaseNotes } from "./releaseNotes";

describe("release notes", () => {
  it("leaves out download instructions", () => {
    const md = "Intro.\n\n## Download\n\nGet it here.\n\n## What's changed\n\n- Faster.";
    expect(inAppNotes(md)).toBe("Intro.\n\n## What's changed\n\n- Faster.");
  });

  it("leaves out install instructions below a smaller heading, up to the next one", () => {
    const md = [
      "## What's changed",
      "",
      "### Safer",
      "",
      "- Fixed.",
      "",
      "### Install with Homebrew",
      "",
      "```sh",
      "# not a heading",
      "brew install --cask diskpilot",
      "```",
      "",
      "### Docker",
      "",
      "- Stop.",
      "",
      "## Verify",
      "",
      "shasum",
    ].join("\n");
    expect(inAppNotes(md)).toBe(
      "## What's changed\n\n### Safer\n\n- Fixed.\n\n### Docker\n\n- Stop.",
    );
  });

  it("shows 0.6.0's changes without download or Homebrew steps", () => {
    const notes = releaseNotes("0.6.0");
    expect(notes).toContain("### Files by type");
    expect(notes).toContain("### Safer");
    expect(notes).not.toContain("brew");
    expect(notes).not.toContain("## Download");
  });

  it("bundles the notes of a shipped version", () => {
    const notes = releaseNotes("0.5.1");
    expect(notes).toContain("What's changed");
    expect(notes).not.toContain("## Download");
  });

  it("has nothing for an unknown version", () => {
    expect(releaseNotes("9.9.9")).toBeNull();
  });
});

describe("isNewerVersion", () => {
  it("compares versions as numbers", () => {
    expect(isNewerVersion("0.6.1", "0.6.0")).toBe(true);
    expect(isNewerVersion("0.10.0", "0.9.9")).toBe(true);
    expect(isNewerVersion("0.6.0", "0.6.1")).toBe(false);
    expect(isNewerVersion("0.6.0", "0.6.0")).toBe(false);
    expect(isNewerVersion("1.0.0", "0.99.0")).toBe(true);
  });
});
