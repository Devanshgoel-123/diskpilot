import { afterEach, describe, expect, it, vi } from "vitest";

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke, isTauri: () => true }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));

import { api } from "./api";
import { setOs } from "./platform";

afterEach(() => {
  invoke.mockReset();
  setOs("macos");
});

describe("Full Disk Access", () => {
  it("is asked on macOS", async () => {
    setOs("macos");
    invoke.mockResolvedValue("denied");
    expect(await api.getFullDiskAccess()).toBe("denied");
    expect(invoke).toHaveBeenCalledWith("get_full_disk_access");
  });

  it("is never asked on Windows, which has no such setting", async () => {
    setOs("windows");
    expect(await api.getFullDiskAccess()).toBe("unknown");
    await api.openFullDiskAccessSettings();
    expect(invoke).not.toHaveBeenCalled();
  });
});

describe("startScan", () => {
  it("passes the fast flag, off by default", async () => {
    invoke.mockResolvedValue("scan-1");
    await api.startScan("C:\\");
    expect(invoke).toHaveBeenLastCalledWith("start_scan", { root: "C:\\", fast: false });
    await api.startScan("C:\\", true);
    expect(invoke).toHaveBeenLastCalledWith("start_scan", { root: "C:\\", fast: true });
  });
});
