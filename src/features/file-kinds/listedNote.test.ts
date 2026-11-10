import { describe, expect, it } from "vitest";

import { listedNote } from "./listedNote";

describe("listedNote", () => {
  it("says every file is shown when nothing is left out", () => {
    expect(listedNote(28, 28)).toBe("Showing all 28 files.");
  });

  it("explains that smaller files are counted but not listed", () => {
    expect(listedNote(28, 2098)).toBe(
      "Showing the 28 files of 25 KB or more. The other 2,070 are smaller and count in the total.",
    );
    expect(listedNote(4, 5)).toContain("The other 1 is smaller");
  });

  it("names the cap when a type has more large files than are listed", () => {
    expect(listedNote(2000, 9500)).toBe(
      "Showing the 2,000 largest of 9,500 files. The rest count in the total.",
    );
  });
});
