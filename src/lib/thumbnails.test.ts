import { beforeEach, describe, expect, it, vi } from "vitest";

const fileThumbnail = vi.fn();
vi.mock("@/lib/api", () => ({
  api: { fileThumbnail: (path: string, size: number) => fileThumbnail(path, size) },
  errorMessage: (e: unknown) => (e instanceof Error ? e.message : String(e)),
}));

import { cachedThumbnail, clearThumbnails, loadThumbnail } from "./thumbnails";

let made = 0;
const revoked: string[] = [];

beforeEach(() => {
  clearThumbnails();
  fileThumbnail.mockReset();
  fileThumbnail.mockResolvedValue(new ArrayBuffer(4));
  made = 0;
  revoked.length = 0;
  URL.createObjectURL = vi.fn(() => `blob:${++made}`);
  URL.revokeObjectURL = vi.fn((u: string) => void revoked.push(u));
});

describe("thumbnails", () => {
  it("loads each thumbnail once", async () => {
    expect(await loadThumbnail("/a.jpg", 28)).toBe("blob:1");
    expect(await loadThumbnail("/a.jpg", 28)).toBe("blob:1");
    expect(fileThumbnail).toHaveBeenCalledTimes(1);
    expect(cachedThumbnail("/a.jpg", 28)).toBe("blob:1");
  });

  it("remembers that macOS has no preview, but retries other failures", async () => {
    fileThumbnail.mockRejectedValueOnce(new Error("macOS could not make a preview of this file"));
    expect(await loadThumbnail("/a.bin", 28)).toBeNull();
    expect(cachedThumbnail("/a.bin", 28)).toBeNull();

    fileThumbnail.mockRejectedValueOnce(new Error("This file moved since it was listed"));
    expect(await loadThumbnail("/b.jpg", 28)).toBeNull();
    expect(cachedThumbnail("/b.jpg", 28)).toBeUndefined();
    expect(await loadThumbnail("/b.jpg", 28)).toBe("blob:1");
  });

  it("keeps at most 24 large previews and releases the oldest", async () => {
    for (let i = 0; i < 26; i++) await loadThumbnail(`/p${i}.jpg`, 512);
    expect(revoked).toEqual(["blob:1", "blob:2"]);
    expect(cachedThumbnail("/p0.jpg", 512)).toBeUndefined();
    expect(cachedThumbnail("/p25.jpg", 512)).toBe("blob:26");
  });

  it("keeps a recently viewed preview over older ones", async () => {
    for (let i = 0; i < 24; i++) await loadThumbnail(`/p${i}.jpg`, 512);
    cachedThumbnail("/p0.jpg", 512); // viewed again
    await loadThumbnail("/new.jpg", 512);
    expect(cachedThumbnail("/p0.jpg", 512)).toBe("blob:1");
    expect(revoked).toEqual(["blob:2"]);
  });

  it("releases everything when cleared", async () => {
    await loadThumbnail("/a.jpg", 28);
    await loadThumbnail("/a.jpg", 512);
    clearThumbnails();
    expect(revoked.sort()).toEqual(["blob:1", "blob:2"]);
    expect(cachedThumbnail("/a.jpg", 28)).toBeUndefined();
  });

  it("puts a large preview ahead of queued row thumbnails", async () => {
    const order: string[] = [];
    const pending: Array<() => void> = [];
    fileThumbnail.mockImplementation((path: string) => {
      order.push(path);
      return new Promise<ArrayBuffer>((r) => pending.push(() => r(new ArrayBuffer(4))));
    });
    const rows = Array.from({ length: 6 }, (_, i) => loadThumbnail(`/row${i}.jpg`, 28));
    const big = loadThumbnail("/open.jpg", 512);
    // Four rows started; the large preview is next, before rows 4 and 5.
    while (pending.length) {
      pending.shift()?.();
      await Promise.resolve();
      await new Promise((r) => setTimeout(r, 0));
    }
    await Promise.all([...rows, big]);
    expect(order.slice(0, 5)).toEqual([
      "/row0.jpg",
      "/row1.jpg",
      "/row2.jpg",
      "/row3.jpg",
      "/open.jpg",
    ]);
  });

  it("loads the newest row thumbnails first", async () => {
    const order: string[] = [];
    const pending: Array<() => void> = [];
    fileThumbnail.mockImplementation((path: string) => {
      order.push(path);
      return new Promise<ArrayBuffer>((r) => pending.push(() => r(new ArrayBuffer(4))));
    });
    const rows = Array.from({ length: 8 }, (_, i) => loadThumbnail(`/row${i}.jpg`, 28));
    while (pending.length) {
      pending.shift()?.();
      await new Promise((r) => setTimeout(r, 0));
    }
    await Promise.all(rows);
    // Four start at once; of the rest, the most recently requested goes first.
    expect(order.slice(4)).toEqual(["/row7.jpg", "/row6.jpg", "/row5.jpg", "/row4.jpg"]);
  });

  it("keeps a whole 2,000-row list, and looking rows up stays cheap", async () => {
    for (let i = 0; i < 2000; i++) await loadThumbnail(`/r${i}.jpg`, 28);
    expect(revoked).toEqual([]);
    const start = performance.now();
    for (let pass = 0; pass < 10; pass++) {
      for (let i = 0; i < 2000; i++) cachedThumbnail(`/r${i}.jpg`, 28);
    }
    // 20,000 lookups; the old cache copied and filtered every key on each one.
    expect(performance.now() - start).toBeLessThan(100);
    expect(cachedThumbnail("/r0.jpg", 28)).toBe("blob:1");
  });
});
