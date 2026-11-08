import { afterEach, describe, expect, it, vi } from "vitest";

import { DISK_REFRESH_MS, watchDisk } from "./store";

function setVisibility(state: DocumentVisibilityState) {
  Object.defineProperty(document, "visibilityState", { configurable: true, value: state });
}

afterEach(() => {
  vi.useRealTimers();
  setVisibility("visible");
});

describe("free space stays current", () => {
  it("refreshes every 30 seconds while the window is visible", () => {
    vi.useFakeTimers();
    setVisibility("visible");
    const refresh = vi.fn(() => Promise.resolve());
    watchDisk(refresh);
    vi.advanceTimersByTime(DISK_REFRESH_MS * 2);
    expect(refresh).toHaveBeenCalledTimes(2);
  });

  it("does not refresh while hidden, then refreshes when shown again", () => {
    vi.useFakeTimers();
    setVisibility("hidden");
    const refresh = vi.fn(() => Promise.resolve());
    watchDisk(refresh);
    vi.advanceTimersByTime(DISK_REFRESH_MS * 3);
    expect(refresh).not.toHaveBeenCalled();
    setVisibility("visible");
    document.dispatchEvent(new Event("visibilitychange"));
    expect(refresh).toHaveBeenCalledTimes(1);
  });

  it("refreshes when the window regains focus", () => {
    setVisibility("visible");
    const refresh = vi.fn(() => Promise.resolve());
    watchDisk(refresh);
    window.dispatchEvent(new Event("focus"));
    expect(refresh).toHaveBeenCalledTimes(1);
  });
});
