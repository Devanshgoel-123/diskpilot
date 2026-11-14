import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { KeyFile, KeysReport } from "@/lib/types";

const file = (over: Partial<KeyFile>): KeyFile => ({
  path: "/Users/me/.ssh/id_ed25519",
  name: "id_ed25519",
  kind: "sshKey",
  kindLabel: "SSH key",
  bytes: 400,
  modifiedMs: 1_800_000_000_000,
  label: null,
  risks: [],
  canFixPermissions: false,
  ...over,
});

let report: KeysReport;
const setLabel = vi.fn();
const makePrivate = vi.fn();
const moveToTrash = vi.fn();
const reveal = vi.fn();

vi.mock("@/lib/api", () => ({
  api: {
    keysReport: () => Promise.resolve(report),
    keysProviders: () => Promise.resolve(["AWS", "GitHub", "Other"]),
    keysSetLabel: (...a: unknown[]) => setLabel(...a),
    keysMakePrivate: (...a: unknown[]) => makePrivate(...a),
    keysMoveToTrash: (...a: unknown[]) => moveToTrash(...a),
    revealInFinder: (...a: unknown[]) => reveal(...a),
  },
  errorMessage: (e: unknown) => String(e),
}));

import { TooltipProvider } from "@/components/ui/tooltip";
import { useApp } from "@/lib/store";

import { KeysPage } from "./KeysPage";
import { useKeys } from "./keysStore";

beforeEach(() => {
  for (const f of [setLabel, makePrivate, moveToTrash, reveal]) f.mockReset();
  useKeys.setState({ report: null, loading: false, busy: null, error: null });
  useApp.setState({ env: { homeDir: "/Users/me" } as never, desktop: true });
  report = {
    generatedAtMs: 1_800_000_000_000,
    scanned: true,
    scannedAtMs: 1_800_000_000_000,
    files: [
      file({
        path: "/Users/me/Downloads/prod.pem",
        name: "prod.pem",
        kind: "privateKey",
        kindLabel: "Private key",
        canFixPermissions: true,
        risks: [
          { kind: "othersCanRead", text: "Other accounts on this Mac can read it.", serious: true },
          { kind: "copies", text: "2 identical copies on this Mac.", serious: false },
        ],
      }),
      file({ label: { provider: "GitHub", note: "work laptop" } }),
    ],
  };
});

const show = () =>
  render(
    <TooltipProvider>
      <KeysPage />
    </TooltipProvider>,
  );

describe("Keys & secrets", () => {
  it("lists keys with their risks and labels, riskiest first", async () => {
    show();
    expect(await screen.findByText("prod.pem")).toBeVisible();
    expect(screen.getByText("Other accounts on this Mac can read it.")).toBeVisible();
    expect(screen.getByText("GitHub")).toBeVisible();
    expect(screen.queryByRole("button", { name: /export/i })).toBeNull();
  });

  it("finds a key by its label", async () => {
    show();
    await screen.findByText("prod.pem");
    fireEvent.change(screen.getByLabelText("Search keys"), { target: { value: "work laptop" } });
    expect(screen.getByText("id_ed25519")).toBeVisible();
    expect(screen.queryByText("prod.pem")).toBeNull();
  });

  it("filters to the files that need attention", async () => {
    show();
    await screen.findByText("prod.pem");
    fireEvent.click(screen.getByRole("button", { name: /Need attention/ }));
    expect(screen.queryByText("id_ed25519")).toBeNull();
  });

  it("saves a label for a file by its path", async () => {
    setLabel.mockResolvedValue(report);
    show();
    await screen.findByText("prod.pem");
    fireEvent.click(screen.getAllByRole("button", { name: "Add label" })[0] as HTMLElement);
    const dialog = await screen.findByRole("dialog");
    fireEvent.click(within(dialog).getByRole("button", { name: "AWS" }));
    fireEvent.change(within(dialog).getByLabelText(/Note/), { target: { value: "deploys" } });
    await act(async () => {
      fireEvent.click(within(dialog).getByRole("button", { name: "Save label" }));
    });
    expect(setLabel).toHaveBeenCalledWith("/Users/me/Downloads/prod.pem", "AWS", "deploys");
  });

  it("makes a readable key private in one click", async () => {
    makePrivate.mockResolvedValue(report);
    show();
    await screen.findByText("prod.pem");
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Make private" }));
    });
    expect(makePrivate).toHaveBeenCalledWith("/Users/me/Downloads/prod.pem");
  });

  it("asks to scan when no scan has looked for keys", async () => {
    report = { ...report, scanned: false, scannedAtMs: null, files: [] };
    show();
    await waitFor(() => expect(screen.getByText("Scan to find your keys")).toBeVisible());
  });
});
