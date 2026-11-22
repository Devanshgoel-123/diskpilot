import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { setOs } from "@/lib/platform";
import type { DriveInfo, ScanProgress } from "@/lib/types";

const { startScan, relaunchElevated } = vi.hoisted(() => ({
  startScan: vi.fn(),
  relaunchElevated: vi.fn(),
}));
vi.mock("@/lib/api", () => ({
  api: {
    startScan: (root?: string, fast?: boolean) => startScan(root, fast),
    relaunchElevated: () => relaunchElevated(),
  },
  errorMessage: (e: unknown) => (e instanceof Error ? e.message : String(e)),
  inDesktopApp: () => false,
}));

import { TooltipProvider } from "@/components/ui/tooltip";
import { useApp } from "@/lib/store";

import { ScanControl } from "./ScanControl";

const drive: DriveInfo = {
  root: "C:\\",
  label: "",
  fileSystem: "NTFS",
  kind: "fixed",
  totalBytes: 500e9,
  availableBytes: 100e9,
  usedBytes: 400e9,
};

const renderControl = () =>
  render(
    <TooltipProvider>
      <ScanControl />
    </TooltipProvider>,
  );

function progress(files: number): ScanProgress {
  return {
    scanId: "scan-1",
    filesScanned: files,
    dirsScanned: 0,
    bytesScanned: files * 1000,
    currentPath: "C:\\Windows",
    elapsedMs: 0,
    containersDone: 0,
    containersTotal: 0,
  };
}

beforeEach(() => {
  startScan.mockReset();
  relaunchElevated.mockReset();
  startScan.mockResolvedValue("scan-1");
  useApp.setState({
    desktop: true,
    scanning: false,
    fastScan: false,
    progress: null,
    summary: null,
    page: "overview",
    drives: [drive],
    scanRoot: "C:\\",
    elevated: false,
    env: null,
  });
});
afterEach(() => {
  setOs("macos");
});

describe("Fast scan", () => {
  it("is never offered on macOS", () => {
    setOs("macos");
    renderControl();
    expect(screen.queryByRole("button", { name: /Faster whole-drive scan/ })).toBeNull();
  });

  it("is offered for a whole drive when not administrator, and restarts after confirming", async () => {
    setOs("windows");
    renderControl();
    fireEvent.click(screen.getByRole("button", { name: /Faster whole-drive scan/ }));
    expect(screen.getByText(/dragging files in from File Explorer may not work/)).toBeVisible();
    relaunchElevated.mockRejectedValue(new Error("Administrator rights were not granted."));
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Restart as administrator" }));
    });
    expect(relaunchElevated).toHaveBeenCalled();
    expect(screen.getByText("Administrator rights were not granted.")).toBeInTheDocument();
  });

  it("is not offered for the user folder", () => {
    setOs("windows");
    useApp.setState({ scanRoot: null });
    renderControl();
    expect(screen.queryByRole("button", { name: /Faster whole-drive scan/ })).toBeNull();
  });

  it("runs automatically as administrator, and says the file table is being read", async () => {
    setOs("windows");
    useApp.setState({ elevated: true });
    renderControl();
    expect(screen.queryByRole("button", { name: /Faster whole-drive scan/ })).toBeNull();
    expect(screen.getByText("Fast scan")).toBeInTheDocument();
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Scan" }));
    });
    expect(startScan).toHaveBeenCalledWith("C:\\", true);

    act(() => useApp.setState({ progress: progress(0) }));
    expect(screen.getByText("Reading the drive's file table…")).toBeInTheDocument();
    act(() => useApp.setState({ progress: progress(1200) }));
    expect(screen.queryByText("Reading the drive's file table…")).toBeNull();
    expect(screen.getByText(/1,200 files/)).toBeInTheDocument();
  });

  it("is not used for a folder, even as administrator", async () => {
    setOs("windows");
    useApp.setState({ elevated: true, scanRoot: "C:\\Users\\me\\Projects" });
    renderControl();
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Scan" }));
    });
    expect(startScan).toHaveBeenCalledWith("C:\\Users\\me\\Projects", false);
  });
});
