import { describe, expect, it, vi } from "vitest";

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

describe("cleanupStats edge cases", () => {
  it("counts a streak across the new year", () => {
    // Mon 2026-12-21, Thu 2026-12-31 and Fri 2027-01-08 fall in three weeks in a row.
    const s = cleanupStats(
      [act(at(2026, 12, 21)), act(at(2026, 12, 31)), act(at(2027, 1, 8))],
      at(2027, 1, 9),
    );
    expect(s.currentStreak).toBe(3);
    expect(s.longestStreak).toBe(3);
    expect(s.clearedThisWeek).toBe(true);
  });

  it("puts Sunday night in the week before and Monday midnight in the next", () => {
    const sundayNight = new Date(2026, 10, 8, 23, 59, 59).getTime();
    const mondayMidnight = new Date(2026, 10, 9, 0, 0, 0).getTime();
    expect(weekStart(sundayNight)).toBe(new Date(2026, 10, 2).getTime());
    expect(weekStart(mondayMidnight)).toBe(mondayMidnight);
    const s = cleanupStats([act(sundayNight), act(mondayMidnight)], mondayMidnight);
    expect(s.currentStreak).toBe(2);
  });

  it("does not depend on the order cleanups are listed in", () => {
    const list = [act(at(2026, 11, 4)), act(at(2026, 10, 20)), act(at(2026, 10, 28))];
    expect(cleanupStats(list, at(2026, 11, 6))).toEqual(
      cleanupStats([...list].reverse(), at(2026, 11, 6)),
    );
    expect(cleanupStats(list, at(2026, 11, 6)).firstAtMs).toBe(at(2026, 10, 20));
  });

  it("counts several cleanups in one week once for the streak but each for the totals", () => {
    const s = cleanupStats(
      [act(at(2026, 11, 2)), act(at(2026, 11, 3)), act(at(2026, 11, 5))],
      at(2026, 11, 6),
    );
    expect(s.cleanups).toBe(3);
    expect(s.clearedBytes).toBe(3 * GB);
    expect(s.currentStreak).toBe(1);
  });

  it("keeps last week's streak alive through all of this week, then drops it", () => {
    const list = [act(at(2026, 10, 27)), act(at(2026, 11, 3))];
    // Sunday 2026-11-15 23:00 is still the week after the last cleanup's week.
    expect(cleanupStats(list, new Date(2026, 10, 15, 23).getTime()).currentStreak).toBe(2);
    expect(cleanupStats(list, new Date(2026, 10, 16, 0, 1).getTime()).currentStreak).toBe(0);
  });

  it("counts weeks in a row across a daylight saving change", () => {
    vi.stubEnv("TZ", "America/New_York");
    try {
      // The zone change must take effect, or this test checks nothing.
      expect(new Date(2026, 9, 28).getTimezoneOffset()).not.toBe(
        new Date(2026, 10, 3).getTimezoneOffset(),
      );
      // US clocks went back on Sunday 2026-11-01; that week is 1 hour longer.
      const s = cleanupStats(
        [act(at(2026, 10, 28)), act(at(2026, 11, 3)), act(at(2026, 11, 10))],
        at(2026, 11, 11),
      );
      expect(s.currentStreak).toBe(3);
      // Spring forward on 2026-03-08 makes that week an hour shorter.
      const spring = cleanupStats([act(at(2026, 3, 4)), act(at(2026, 3, 11))], at(2026, 3, 12));
      expect(spring.currentStreak).toBe(2);
    } finally {
      vi.unstubAllEnvs();
    }
  });
});
