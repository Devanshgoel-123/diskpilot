import { render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { setOs } from "@/lib/platform";
import type { VirtualDisk, WslOverview } from "@/lib/types";

const { wslDisks, navigate } = vi.hoisted(() => ({ wslDisks: vi.fn(), navigate: vi.fn() }));
vi.mock("@/lib/api", () => ({
  api: { wslDisks: () => wslDisks() },
  errorMessage: (e: unknown) => String(e),
}));
vi.mock("@/lib/store", () => {
  const state = { env: { homeDir: "C:\\Users\\me" }, navigate };
  const useApp = (select: (s: typeof state) => unknown) => select(state);
  useApp.getState = () => state;
  return { useApp };
});

import { WindowsSections } from "../sections";
import { VirtualDisksSummary } from "./VirtualDisks";
import { useWsl } from "./wslStore";

function disk(over: Partial<VirtualDisk> = {}): VirtualDisk {
  return {
    id: "distro:{1}",
    kind: "distro",
    name: "Ubuntu",
    distro: "Ubuntu",
    path: "C:\\Users\\me\\AppData\\Local\\wsl\\{1}\\ext4.vhdx",
    fileBytes: 40e9,
    allocatedBytes: 40e9,
    sparse: false,
    running: true,
    isDefault: true,
    usedBytes: 12e9,
    reclaimableBytes: 28e9,
    usageNote: "Measured with df",
    ...over,
  };
}

function overview(disks: VirtualDisk[]): WslOverview {
  return {
    supported: true,
    disks,
    totalBytes: disks.reduce((a, d) => a + d.allocatedBytes, 0),
    reclaimableBytes: disks.reduce((a, d) => a + (d.reclaimableBytes ?? 0), 0),
    message: null,
  };
}

beforeEach(() => {
  wslDisks.mockReset();
  useWsl.setState({ overview: null, loading: false, error: null });
});
afterEach(() => setOs("macos"));

describe("Virtual disks", () => {
  it("shows nothing and asks nothing on macOS", () => {
    setOs("macos");
    const { container } = render(<WindowsSections page="developer" />);
    expect(container).toBeEmptyDOMElement();
    expect(wslDisks).not.toHaveBeenCalled();
  });

  it("lists each disk on Windows, offering sparse only for distributions that are not sparse", async () => {
    setOs("windows");
    wslDisks.mockResolvedValue(
      overview([
        disk(),
        disk({ id: "distro:{2}", name: "Debian", distro: "Debian", sparse: true, running: false }),
        disk({
          id: "docker:docker_data.vhdx",
          kind: "dockerData",
          name: "Docker Desktop data",
          distro: null,
          running: null,
          isDefault: false,
          usedBytes: null,
          reclaimableBytes: null,
          usageNote: "Start Docker Desktop to measure the space in use.",
        }),
      ]),
    );
    render(<WindowsSections page="developer" />);
    await screen.findByText("Ubuntu");
    expect(screen.getByText("WSL distribution · Default · Running")).toBeInTheDocument();
    expect(screen.getByText(/Shrinks by itself/)).toBeInTheDocument();
    expect(screen.getByText("Docker data")).toBeInTheDocument();
    expect(
      screen.getByText("Start Docker Desktop to measure the space in use."),
    ).toBeInTheDocument();
    expect(screen.getAllByRole("button", { name: "Compact…" })).toHaveLength(3);
    expect(screen.getAllByRole("button", { name: "Make it shrink by itself…" })).toHaveLength(1);
  });

  it("summarizes on the Overview only when at least 1 GB can be returned", async () => {
    setOs("windows");
    useWsl.setState({ overview: overview([disk({ reclaimableBytes: 0.5e9 })]) });
    const { container, rerender } = render(<VirtualDisksSummary />);
    expect(container).toBeEmptyDOMElement();

    useWsl.setState({ overview: overview([disk()]) });
    rerender(<VirtualDisksSummary />);
    expect(screen.getByText("28.0 GB")).toBeInTheDocument();
    screen.getByRole("button", { name: "Review" }).click();
    expect(navigate).toHaveBeenCalledWith("developer");
  });
});
