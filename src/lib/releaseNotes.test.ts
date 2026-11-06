import { describe, expect, it } from "vitest";

import { inAppNotes, releaseNotes } from "./releaseNotes";

describe("release notes", () => {
  it("leaves out download instructions", () => {
    const md = "Intro.\n\n## Download\n\nGet it here.\n\n## What's changed\n\n- Faster.";
    expect(inAppNotes(md)).toBe("Intro.\n\n## What's changed\n\n- Faster.");
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
