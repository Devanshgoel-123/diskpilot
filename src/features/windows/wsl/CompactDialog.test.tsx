import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { CompactPreview, CompactResult, VirtualDisk } from "@/lib/types";

const wslCompactPreview = vi.fn();
const wslCompactExecute = vi.fn();
vi.mock("@/lib/api", () => ({
  api: {
    wslCompactPreview: (id: string, method: string) => wslCompactPreview(id, method),
    wslCompactExecute: (id: string, typed: string) => wslCompactExecute(id, typed),
  },
  errorMessage: (e: unknown) => (e instanceof Error ? e.message : String(e)),
}));
const refreshHistory = vi.fn();
const refreshDisk = vi.fn();
vi.mock("@/lib/store", () => ({
  useApp: { getState: () => ({ refreshHistory, refreshDisk }) },
}));

import { CompactDialog } from "./CompactDialog";

const disk: VirtualDisk = {
  id: "distro:{1}",
  kind: "distro",
  name: "Ubuntu",
  distro: "Ubuntu",
  path: "C:\\Users\\me\\AppData\\Local\\wsl\\{1}\\ext4.vhdx",
  fileBytes: 40e9,
  allocatedBytes: 40e9,
  sparse: false,
  running: false,
  isDefault: true,
  usedBytes: 12e9,
  reclaimableBytes: 28e9,
  usageNote: "Measured with df",
};

function preview(over: Partial<CompactPreview> = {}): CompactPreview {
  return {
    id: "wsl-1",
    disk,
    method: "compact",
    tool: "diskpart",
    title: "Compact Ubuntu",
    steps: [
      { label: "Stop WSL", command: ["wsl.exe", "--shutdown"], elevated: false, script: null },
      {
        label: "Compact the disk with diskpart (administrator)",
        command: ["diskpart.exe", "/s", "<script>"],
        elevated: true,
        script: "select vdisk file=ext4.vhdx\ncompact vdisk",
      },
    ],
    effects: ["The .vhdx file shrinks."],
    warnings: ["wsl --shutdown stops every WSL distribution."],
    blocked: null,
    confirmName: "Ubuntu",
    beforeBytes: 40e9,
    estimatedReclaimBytes: 28e9,
    ...over,
  };
}

function result(over: Partial<CompactResult> = {}): CompactResult {
  return {
    preview: preview(),
    succeeded: true,
    beforeBytes: 40e9,
    afterBytes: 12e9,
    freedBytes: 28e9,
    output: "DiskPart successfully compacted the virtual disk file.",
    error: null,
    record: {
      id: "wsl-1",
      kind: "wsl",
      title: "Compact Ubuntu",
      startedAtMs: 0,
      finishedAtMs: 1,
      status: "succeeded",
      bytesFreed: 28e9,
      bytesTrashed: 0,
      items: [],
    },
    ...over,
  };
}

beforeEach(() => {
  wslCompactPreview.mockReset();
  wslCompactExecute.mockReset();
  refreshHistory.mockReset();
  refreshDisk.mockReset();
});

describe("CompactDialog", () => {
  it("shows every step, the script and elevation, and runs only after the name is typed", async () => {
    wslCompactPreview.mockResolvedValue(preview());
    wslCompactExecute.mockResolvedValue(result());
    const onDone = vi.fn();
    render(<CompactDialog disk={disk} method="compact" onClose={() => {}} onDone={onDone} />);

    await screen.findByText("wsl.exe --shutdown");
    expect(screen.getByText("diskpart.exe /s <script>")).toBeInTheDocument();
    expect(screen.getByText(/compact vdisk/)).toBeInTheDocument();
    expect(screen.getAllByText("Needs administrator approval")).toHaveLength(1);
    expect(screen.getByText("wsl --shutdown stops every WSL distribution.")).toBeInTheDocument();

    const confirm = screen.getByRole("button", { name: "Compact" });
    expect(confirm).toBeDisabled();
    const input = screen.getByLabelText("Type Ubuntu to continue");
    fireEvent.change(input, { target: { value: "ubuntu" } });
    expect(confirm).toBeDisabled();
    fireEvent.change(input, { target: { value: "Ubuntu" } });
    expect(confirm).toBeEnabled();

    await act(async () => {
      fireEvent.click(confirm);
    });
    expect(wslCompactExecute).toHaveBeenCalledWith("wsl-1", "Ubuntu");
    await screen.findByText("28.0 GB freed");
    expect(onDone).toHaveBeenCalled();
    expect(refreshHistory).toHaveBeenCalled();
    expect(refreshDisk).toHaveBeenCalled();
  });

  it("shows the reason instead of a confirm button when blocked", async () => {
    wslCompactPreview.mockResolvedValue(
      preview({ steps: [], blocked: "This disk is already sparse.", method: "sparse" }),
    );
    render(<CompactDialog disk={disk} method="sparse" onClose={() => {}} onDone={() => {}} />);
    await screen.findByText("This disk is already sparse.");
    expect(screen.queryByRole("button", { name: "Turn on" })).toBeNull();
    expect(screen.queryByLabelText(/to continue/)).toBeNull();
  });

  it("offers a fresh review after the engine refuses a preview", async () => {
    wslCompactPreview.mockResolvedValueOnce(preview());
    wslCompactPreview.mockResolvedValueOnce(preview({ id: "wsl-2" }));
    wslCompactExecute.mockRejectedValue(new Error("This preview has expired. Review it again."));
    render(<CompactDialog disk={disk} method="compact" onClose={() => {}} onDone={() => {}} />);
    await screen.findByText("wsl.exe --shutdown");
    fireEvent.change(screen.getByLabelText("Type Ubuntu to continue"), {
      target: { value: "Ubuntu" },
    });
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Compact" }));
    });
    await screen.findByText("This preview has expired. Review it again.");
    expect(screen.queryByRole("button", { name: "Compact" })).toBeNull();

    fireEvent.click(screen.getByRole("button", { name: "Review again" }));
    await waitFor(() => expect(wslCompactPreview).toHaveBeenCalledTimes(2));
    const input = await screen.findByLabelText("Type Ubuntu to continue");
    expect(input).toHaveValue("");
    fireEvent.change(input, { target: { value: "Ubuntu" } });
    wslCompactExecute.mockResolvedValue(result());
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Compact" }));
    });
    expect(wslCompactExecute).toHaveBeenLastCalledWith("wsl-2", "Ubuntu");
  });

  it("asks for a new review once the preview expires", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    try {
      wslCompactPreview.mockResolvedValue(preview());
      render(<CompactDialog disk={disk} method="compact" onClose={() => {}} onDone={() => {}} />);
      await screen.findByText("wsl.exe --shutdown");
      act(() => {
        vi.advanceTimersByTime(10 * 60 * 1000);
      });
      expect(screen.getByText(/This review has expired/)).toBeInTheDocument();
      expect(screen.getByRole("button", { name: "Review again" })).toBeInTheDocument();
    } finally {
      vi.useRealTimers();
    }
  });

  it("shows the error and output when compaction fails", async () => {
    wslCompactPreview.mockResolvedValue(preview());
    wslCompactExecute.mockResolvedValue(
      result({
        succeeded: false,
        afterBytes: 40e9,
        freedBytes: 0,
        error: "The disk could not be compacted (exit code 1).",
        output: "DiskPart has encountered an error.",
      }),
    );
    render(<CompactDialog disk={disk} method="compact" onClose={() => {}} onDone={() => {}} />);
    await screen.findByText("wsl.exe --shutdown");
    fireEvent.change(screen.getByLabelText("Type Ubuntu to continue"), {
      target: { value: "Ubuntu" },
    });
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Compact" }));
    });
    await screen.findByText("The disk could not be compacted (exit code 1).");
    expect(screen.getByText("No space was freed")).toBeInTheDocument();
    expect(screen.getByText("Output")).toBeInTheDocument();
  });
});
