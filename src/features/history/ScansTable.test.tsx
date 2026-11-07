import { render, screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import type { ScanRecord } from "@/lib/types";

import { dayLabel, formatChange, sizeChanges, timeOfDay } from "./scans";
import { ScansTable } from "./ScansTable";

const GB = 1e9;

function scan(id: string, at: Date, over: Partial<ScanRecord> = {}): ScanRecord {
  return {
    id,
    root: "/Users/me",
    startedAtMs: at.getTime(),
    finishedAtMs: at.getTime() + 16_000,
    durationMs: 16_000,
    totalBytes: 70 * GB,
    fileCount: 726_072,
    dirCount: 1,
    errorCount: 0,
    status: "completed",
    ...over,
  };
}

describe("scan table helpers", () => {
  const now = new Date(2026, 10, 7, 15, 48).getTime();

  it("names days relative to today", () => {
    expect(dayLabel(new Date(2026, 10, 7, 0, 1).getTime(), now)).toBe("Today");
    expect(dayLabel(new Date(2026, 10, 6, 23, 59).getTime(), now)).toBe("Yesterday");
    expect(dayLabel(new Date(2026, 10, 5, 9).getTime(), now)).toBe("Thu, Nov 5");
    expect(dayLabel(new Date(2025, 10, 5, 9).getTime(), now)).toBe("Wed, Nov 5, 2025");
  });

  it("shows the time without a leading zero", () => {
    expect(timeOfDay(new Date(2026, 10, 7, 15, 47).getTime())).toBe("3:47 PM");
    expect(timeOfDay(new Date(2026, 10, 7, 1, 5).getTime())).toBe("1:05 AM");
  });

  it("compares each scan with the previous completed scan of the same folder", () => {
    const changes = sizeChanges([
      scan("d", new Date(2026, 10, 7, 15), { totalBytes: 102 * GB }),
      scan("other", new Date(2026, 10, 7, 14), { root: "/Users/me/Code", totalBytes: 5 * GB }),
      scan("c", new Date(2026, 10, 7, 13), { status: "cancelled", totalBytes: 0 }),
      scan("b", new Date(2026, 10, 7, 2), { totalBytes: 74 * GB }),
      scan("a", new Date(2026, 10, 7, 1), { totalBytes: 70 * GB }),
    ]);
    expect(changes.get("a")).toBeUndefined();
    expect(changes.get("b")).toBe(4 * GB);
    // The cancelled scan in between is skipped.
    expect(changes.get("d")).toBe(28 * GB);
    expect(changes.get("other")).toBeUndefined();
    expect(changes.has("c")).toBe(false);
  });

  it("formats changes with a sign, and tiny ones as Same", () => {
    expect(formatChange(28.3 * GB)).toMatch(/^\+28\.3 GB$/);
    expect(formatChange(-4.1 * GB)).toMatch(/^−4\.1 GB$/);
    expect(formatChange(1000)).toBe("Same");
  });
});

describe("ScansTable", () => {
  it("groups scans by day, newest first, and only names unusual outcomes", () => {
    const today = new Date();
    const yesterday = new Date(today.getTime() - 24 * 60 * 60 * 1000);
    render(
      <ScansTable
        home="/Users/me"
        records={[
          scan("old", new Date(yesterday.setHours(9, 0, 0, 0)), { status: "cancelled" }),
          scan("new", new Date(new Date(today).setHours(0, 30, 0, 0)), { totalBytes: 74 * GB }),
          scan("first", new Date(new Date(yesterday).setHours(8, 0, 0, 0))),
        ]}
      />,
    );
    const rows = screen.getAllByRole("row").slice(1);
    expect(rows.map((r) => r.textContent?.split("~")[0])).toEqual([
      "Today",
      "12:30 AM",
      "Yesterday",
      "9:00 AM",
      "8:00 AM",
    ]);
    expect(within(rows[1] ?? document.body).getByText(/^\+4\.0 GB$/)).toBeInTheDocument();
    expect(within(rows[3] ?? document.body).getByText("Cancelled")).toBeInTheDocument();
    expect(screen.queryByText("Completed")).toBeNull();
  });
});
