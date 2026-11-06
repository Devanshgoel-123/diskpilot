import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { DockerAction, DockerActionPreview } from "@/lib/types";

const dockerPreview = vi.fn();
const dockerExecute = vi.fn();
vi.mock("@/lib/api", () => ({
  api: {
    dockerPreview: (a: DockerAction) => dockerPreview(a),
    dockerExecute: (id: string, typed: string) => dockerExecute(id, typed),
  },
  errorMessage: (e: unknown) => (e instanceof Error ? e.message : String(e)),
}));

import { DockerActionDialog } from "./DockerActionDialog";

const action: DockerAction = { type: "removeContainer", id: "c1", stopFirst: false, volumes: [] };

function preview(over: Partial<DockerActionPreview> = {}): DockerActionPreview {
  return {
    id: "docker-1",
    action,
    title: "Remove container db-1",
    steps: [{ command: ["docker", "rm", "c1"], target: "db-1", bytes: 10 }],
    volumeChoices: [
      { name: "pgdata", bytes: 100, composeProject: null, selected: false, keptBecause: null },
    ],
    destructive: true,
    affected: [{ label: "db-1", detail: "postgres", bytes: 10 }],
    effects: [],
    warnings: [],
    blocked: null,
    reclaimBytes: 10,
    confirmName: null,
    ...over,
  };
}

beforeEach(() => {
  dockerPreview.mockReset();
  dockerExecute.mockReset();
});

describe("DockerActionDialog", () => {
  it("runs the reviewed preview by id, after the engine's name is typed", async () => {
    dockerPreview.mockResolvedValueOnce(preview());
    dockerPreview.mockResolvedValueOnce(
      preview({
        id: "docker-2",
        steps: [
          { command: ["docker", "rm", "c1"], target: "db-1", bytes: 10 },
          { command: ["docker", "volume", "rm", "pgdata"], target: "pgdata", bytes: 100 },
        ],
        confirmName: "db-1",
      }),
    );
    dockerExecute.mockResolvedValue({
      preview: preview(),
      succeeded: true,
      output: "",
      error: null,
      record: null,
    });
    render(<DockerActionDialog action={action} onClose={() => {}} onDone={() => {}} />);
    await screen.findByText("docker rm c1");

    fireEvent.click(screen.getByRole("checkbox"));
    // The old preview is gone until the new one, with the volume, arrives.
    expect(screen.queryByText("docker rm c1")).toBeNull();
    await screen.findByText("docker volume rm pgdata");

    const confirm = screen.getByRole("button", { name: /Remove container \+ 1 volume/ });
    expect(confirm).toBeDisabled();
    fireEvent.change(screen.getByLabelText("Type db-1 to confirm"), { target: { value: "db" } });
    expect(confirm).toBeDisabled();
    fireEvent.change(screen.getByLabelText("Type db-1 to confirm"), { target: { value: "db-1" } });
    expect(confirm).toBeEnabled();
    await act(async () => {
      fireEvent.click(confirm);
    });
    expect(dockerExecute).toHaveBeenCalledWith("docker-2", "db-1");
  });

  it("shows a fresh preview after a run is refused", async () => {
    dockerPreview.mockResolvedValue(preview());
    dockerExecute.mockRejectedValue(new Error("Docker changed since you reviewed this."));
    render(<DockerActionDialog action={action} onClose={() => {}} onDone={() => {}} />);
    await screen.findByText("docker rm c1");
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Remove container" }));
    });
    await screen.findByText("Docker changed since you reviewed this.");
    await waitFor(() => expect(dockerPreview).toHaveBeenCalledTimes(2));
    await screen.findByText("docker rm c1");
  });
});
