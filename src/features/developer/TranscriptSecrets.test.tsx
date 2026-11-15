import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { SecretsReport } from "@/lib/types";

const remove = vi.fn();
let report: SecretsReport;

vi.mock("@/lib/api", () => ({
  api: {
    checkAiTranscripts: () => Promise.resolve(report),
    removeTranscriptSecrets: (...a: unknown[]) => remove(...a),
    revealInFinder: vi.fn(),
  },
  errorMessage: (e: unknown) => String(e),
}));

import { TooltipProvider } from "@/components/ui/tooltip";

import { TranscriptSecrets } from "./TranscriptSecrets";

const finding = (path: string, modifiedMs: number) => ({
  agent: "Claude Code",
  path,
  bytes: 10,
  modifiedMs,
  matches: [
    { kind: "Private key", masked: "-----BEGIN … PRIVATE KEY-----", line: 3, occurrences: 1 },
  ],
});

beforeEach(() => {
  remove.mockReset();
  report = {
    generatedAtMs: Date.now(),
    filesChecked: 2,
    bytesChecked: 20,
    agents: ["Claude Code"],
    findings: [finding("/h/.claude/projects/a/old.jsonl", Date.now() - 86_400_000)],
    cancelled: false,
  };
});

const check = async () => {
  render(
    <TooltipProvider>
      <TranscriptSecrets />
    </TooltipProvider>,
  );
  await act(async () => {
    fireEvent.click(screen.getByRole("button", { name: "Check transcripts" }));
  });
};

describe("Credentials in AI transcripts", () => {
  it("removes the keys from a transcript after confirming", async () => {
    remove.mockResolvedValue({ ...report, findings: [] });
    await check();
    fireEvent.click(screen.getByRole("button", { name: "Remove keys" }));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText(/can’t be undone/)).toBeVisible();
    await act(async () => {
      fireEvent.click(within(dialog).getByRole("button", { name: "Remove keys" }));
    });
    expect(remove).toHaveBeenCalledWith("/h/.claude/projects/a/old.jsonl");
    expect(await screen.findByText(/No known credential formats/)).toBeVisible();
  });

  it("won't remove keys from a conversation that may still be open", async () => {
    report.findings = [finding("/h/.claude/projects/a/now.jsonl", Date.now())];
    await check();
    expect(screen.getByRole("button", { name: "Remove keys" })).toBeDisabled();
  });
});
