import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const check = vi.fn();
const relaunch = vi.fn();
vi.mock("@tauri-apps/plugin-updater", () => ({ check: () => check() }));
vi.mock("@tauri-apps/plugin-process", () => ({ relaunch: () => relaunch() }));

type DownloadEvent =
  | { event: "Started"; data: { contentLength?: number } }
  | { event: "Progress"; data: { chunkLength: number } }
  | { event: "Finished" };

/** An update whose download reports the given events, then succeeds or throws. */
function fakeUpdate(events: DownloadEvent[], fail?: Error) {
  return {
    version: "0.6.1",
    body: "notes",
    downloadAndInstall: vi.fn(async (onEvent: (e: DownloadEvent) => void) => {
      for (const e of events) onEvent(e);
      if (fail) throw fail;
    }),
  };
}

/** A fresh copy of the module, so its saved setting and timers start clean. */
async function load() {
  vi.resetModules();
  return import("./updates");
}

beforeEach(() => {
  check.mockReset();
  relaunch.mockReset();
  localStorage.clear();
});

afterEach(() => {
  vi.useRealTimers();
});

describe("checking for updates", () => {
  it("keeps a newer version it finds", async () => {
    const { useUpdates } = await load();
    const update = fakeUpdate([]);
    check.mockResolvedValue(update);
    await useUpdates.getState().checkNow();
    const s = useUpdates.getState();
    expect(s.update).toBe(update);
    expect(s.upToDate).toBe(false);
    expect(s.checking).toBe(false);
    expect(s.lastCheckedMs).not.toBeNull();
  });

  it("says up to date only after a check the person asked for", async () => {
    const { useUpdates } = await load();
    check.mockResolvedValue(null);
    await useUpdates.getState().checkNow();
    expect(useUpdates.getState().upToDate).toBe(false);
    await useUpdates.getState().checkNow(true);
    expect(useUpdates.getState().upToDate).toBe(true);
  });

  it("fails quietly when checking on its own, and says why when asked", async () => {
    const { useUpdates } = await load();
    check.mockRejectedValue(new Error("offline"));
    await useUpdates.getState().checkNow();
    expect(useUpdates.getState().error).toBeNull();
    await useUpdates.getState().checkNow(true);
    expect(useUpdates.getState().error).toBe("offline");
    expect(useUpdates.getState().checking).toBe(false);
  });

  it("clears an earlier error when checking again", async () => {
    const { useUpdates } = await load();
    check.mockRejectedValueOnce(new Error("offline")).mockResolvedValueOnce(null);
    await useUpdates.getState().checkNow(true);
    await useUpdates.getState().checkNow(true);
    expect(useUpdates.getState().error).toBeNull();
    expect(useUpdates.getState().upToDate).toBe(true);
  });

  it("does not start a second check while one is running", async () => {
    const { useUpdates } = await load();
    let finish: (v: null) => void = () => {};
    check.mockReturnValue(new Promise((r) => (finish = r)));
    const first = useUpdates.getState().checkNow();
    await useUpdates.getState().checkNow(true);
    expect(check).toHaveBeenCalledTimes(1);
    finish(null);
    await first;
  });
});

describe("installing an update", () => {
  it("tracks the download, then restarts", async () => {
    const { useUpdates } = await load();
    const seen: Array<{ done: number; total: number | null } | null> = [];
    const update = fakeUpdate([
      { event: "Started", data: { contentLength: 100 } },
      { event: "Progress", data: { chunkLength: 40 } },
      { event: "Progress", data: { chunkLength: 60 } },
      { event: "Finished" },
    ]);
    check.mockResolvedValue(update);
    await useUpdates.getState().checkNow();
    const unsubscribe = useUpdates.subscribe((s) => seen.push(s.progress));
    await useUpdates.getState().install();
    unsubscribe();
    expect(seen).toContainEqual({ done: 40, total: 100 });
    expect(useUpdates.getState().progress).toEqual({ done: 100, total: 100 });
    expect(relaunch).toHaveBeenCalledTimes(1);
  });

  it("copes with a download of unknown size", async () => {
    const { useUpdates } = await load();
    check.mockResolvedValue(
      fakeUpdate([
        { event: "Started", data: {} },
        { event: "Progress", data: { chunkLength: 10 } },
      ]),
    );
    await useUpdates.getState().checkNow();
    await useUpdates.getState().install();
    expect(useUpdates.getState().progress).toEqual({ done: 10, total: null });
  });

  it("does not restart when the download or signature check fails", async () => {
    const { useUpdates } = await load();
    check.mockResolvedValue(fakeUpdate([], new Error("signature mismatch")));
    await useUpdates.getState().checkNow();
    await useUpdates.getState().install();
    const s = useUpdates.getState();
    expect(relaunch).not.toHaveBeenCalled();
    expect(s.installError).toMatch(/could not be verified/);
    expect(s.unverified).toBe(true);
    // An install failure is not a failed check.
    expect(s.error).toBeNull();
    expect(s.installing).toBe(false);
    expect(s.progress).toBeNull();
    expect(s.update).not.toBeNull();
    // Retrying an unverified update cannot help, so it is not attempted.
    await useUpdates.getState().install();
    expect(s.update?.downloadAndInstall).toHaveBeenCalledTimes(1);
  });

  it("lets a network failure be retried", async () => {
    const { useUpdates } = await load();
    const update = fakeUpdate([], new Error("error sending request for url"));
    check.mockResolvedValue(update);
    await useUpdates.getState().checkNow();
    await useUpdates.getState().install();
    expect(useUpdates.getState().installError).toMatch(/Check your connection/);
    expect(useUpdates.getState().unverified).toBe(false);
    await useUpdates.getState().install();
    expect(update.downloadAndInstall).toHaveBeenCalledTimes(2);
  });

  it("gives a newer version a fresh chance after one failed verification", async () => {
    const { useUpdates } = await load();
    check.mockResolvedValueOnce(fakeUpdate([], new Error("signature mismatch")));
    await useUpdates.getState().checkNow();
    await useUpdates.getState().install();
    expect(useUpdates.getState().unverified).toBe(true);
    check.mockResolvedValueOnce({ ...fakeUpdate([]), version: "0.6.2" });
    await useUpdates.getState().checkNow();
    expect(useUpdates.getState().unverified).toBe(false);
    expect(useUpdates.getState().installError).toBeNull();
  });

  it("installs only once when pressed twice", async () => {
    const { useUpdates } = await load();
    const update = fakeUpdate([]);
    check.mockResolvedValue(update);
    await useUpdates.getState().checkNow();
    await Promise.all([useUpdates.getState().install(), useUpdates.getState().install()]);
    expect(update.downloadAndInstall).toHaveBeenCalledTimes(1);
  });

  it("does nothing without an update", async () => {
    const { useUpdates } = await load();
    await useUpdates.getState().install();
    expect(useUpdates.getState().installing).toBe(false);
    expect(relaunch).not.toHaveBeenCalled();
  });

  it("does not check while installing", async () => {
    const { useUpdates } = await load();
    let finish: () => void = () => {};
    const update = {
      ...fakeUpdate([]),
      downloadAndInstall: vi.fn(() => new Promise<void>((r) => (finish = r))),
    };
    check.mockResolvedValue(update);
    await useUpdates.getState().checkNow();
    const installing = useUpdates.getState().install();
    await useUpdates.getState().checkNow(true);
    expect(check).toHaveBeenCalledTimes(1);
    finish();
    await installing;
  });
});

describe("automatic checks", () => {
  it("are on unless turned off, and the choice is remembered", async () => {
    let { useUpdates } = await load();
    expect(useUpdates.getState().autoCheck).toBe(true);
    useUpdates.getState().setAutoCheck(false);
    ({ useUpdates } = await load());
    expect(useUpdates.getState().autoCheck).toBe(false);
    useUpdates.getState().setAutoCheck(true);
    ({ useUpdates } = await load());
    expect(useUpdates.getState().autoCheck).toBe(true);
  });

  it("stay on when storage cannot be read", async () => {
    const spy = vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new Error("blocked");
    });
    const { useUpdates } = await load();
    expect(useUpdates.getState().autoCheck).toBe(true);
    spy.mockRestore();
  });

  it("run shortly after launch and then once a day", async () => {
    vi.useFakeTimers();
    const { startUpdateChecks } = await load();
    check.mockResolvedValue(null);
    startUpdateChecks();
    startUpdateChecks(); // a second call must not add more timers
    await vi.advanceTimersByTimeAsync(9_999);
    expect(check).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(1);
    expect(check).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(24 * 60 * 60 * 1000);
    expect(check).toHaveBeenCalledTimes(2);
  });

  it("never run when turned off", async () => {
    vi.useFakeTimers();
    const { startUpdateChecks, useUpdates } = await load();
    useUpdates.getState().setAutoCheck(false);
    startUpdateChecks();
    await vi.advanceTimersByTimeAsync(2 * 24 * 60 * 60 * 1000);
    expect(check).not.toHaveBeenCalled();
  });
});

describe("installFailure", () => {
  it.each([
    ["The signature verification failed", /could not be verified/, true],
    ["MissingSignedVersion", /could not be verified/, true],
    ["Read-only file system (os error 30)", /drag it into Applications/, false],
    ["TempDirNotOnSameMountPoint", /drag it into Applications/, false],
    ["dns error: failed to lookup address", /Check your connection/, false],
    ["something odd", /could not be installed: something odd/, false],
  ])("explains %s", async (raw, message, unverified) => {
    const { installFailure } = await load();
    const r = installFailure(raw);
    expect(r.message).toMatch(message);
    expect(r.unverified).toBe(unverified);
  });
});
