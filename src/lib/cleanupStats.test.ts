import { describe, expect, it } from "vitest";

import { type CleanupActivity, cleanupStats, weekStart } from "./cleanupStats";

/** Local noon on a date, so tests hold in any time zone. */
const at = (y: number, m: number, d: number) => new Date(y, m - 1, d, 12).getTime();
const GB = 1e9;

function act(ms: number, over: Partial<CleanupActivity> = {}): CleanupActivity {
  return {
    kind: "files",
    finishedAtMs: ms,
    status: "succeeded",
    bytesFreed: 0,
    bytesTrashed: GB,
    ...over,
  };
}

describe("weekStart", () => {
  it("starts weeks on Monday", () => {
    // 2026-11-06 is a Friday; 2026-11-02 is the Monday before it.
    expect(weekStart(at(2026, 11, 6))).toBe(new Date(2026, 10, 2).getTime());
    expect(weekStart(at(2026, 11, 2))).toBe(new Date(2026, 10, 2).getTime());
    expect(weekStart(at(2026, 11, 8))).toBe(new Date(2026, 10, 2).getTime());
  });
});

describe("cleanupStats", () => {
  const now = at(2026, 11, 6);

  it("is empty with no cleanups", () => {
    expect(cleanupStats([], now)).toEqual({
      clearedBytes: 0,
      cleanups: 0,
      firstAtMs: null,
      currentStreak: 0,
      longestStreak: 0,
      clearedThisWeek: false,
    });
  });

  it("counts deleted and trashed space, but not failures or emptying the Trash", () => {
    const s = cleanupStats(
      [
        act(at(2026, 11, 3), { bytesFreed: 2 * GB, bytesTrashed: 0, kind: "docker" }),
        act(at(2026, 11, 4), { status: "partial" }),
        act(at(2026, 11, 4), { status: "failed" }),
        act(at(2026, 11, 5), { kind: "trash", bytesFreed: 5 * GB, bytesTrashed: 0 }),
        act(at(2026, 11, 5), { bytesTrashed: 0 }),
      ],
      now,
    );
    expect(s.clearedBytes).toBe(3 * GB);
    expect(s.cleanups).toBe(2);
    expect(s.firstAtMs).toBe(at(2026, 11, 3));
  });

  it("counts weeks in a row and keeps the streak alive until this week ends", () => {
    const weeks = [at(2026, 10, 12), at(2026, 10, 20), at(2026, 10, 26), at(2026, 11, 1)];
    const s = cleanupStats(
      weeks.map((w) => act(w)),
      now,
    );
    // Oct 12 alone, then Oct 20, Oct 26 and Nov 1 (a Sunday) are three weeks in a row.
    expect(s.longestStreak).toBe(3);
    expect(s.currentStreak).toBe(3);
    expect(s.clearedThisWeek).toBe(false);

    const withThisWeek = cleanupStats(
      [...weeks, at(2026, 11, 4)].map((w) => act(w)),
      now,
    );
    expect(withThisWeek.currentStreak).toBe(4);
    expect(withThisWeek.clearedThisWeek).toBe(true);
  });

  it("ends the streak after a week with no cleanup", () => {
    const s = cleanupStats([act(at(2026, 10, 13)), act(at(2026, 10, 20))], now);
    expect(s.currentStreak).toBe(0);
    expect(s.longestStreak).toBe(2);
  });
});
