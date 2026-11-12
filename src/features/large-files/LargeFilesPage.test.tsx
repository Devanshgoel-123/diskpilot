import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { api } from "@/lib/api";
import { useApp } from "@/lib/store";
import type { Classification, ScanSummary } from "@/lib/types";

import { LargeFilesPage } from "./LargeFilesPage";

const cls: Classification = {
  category: "downloads",
  label: "Downloads",
  explanation: "",
  ruleId: "home.downloads",
  confidence: "high",
  matchedPath: null,
};

const summary = {
  id: "s",
  root: "/Users/t",
  startedAtMs: 0,
  finishedAtMs: 0,
  durationMs: 0,
  totalBytes: 0,
  fileCount: 0,
  dirCount: 0,
  symlinkCount: 0,
  skippedVolumes: 0,
  errorCount: 0,
  errorSamples: [],
  categories: [],
  largestFiles: [
    {
      path: "/Users/t/Downloads/big.zip",
      name: "big.zip",
      bytes: 9_000,
      logicalBytes: 9_000,
      modifiedMs: null,
      classification: cls,
    },
  ],
  detectedLocations: [],
  heavyDirectories: [
    { path: "/Users/t/Downloads", bytes: 9_000, fileCount: 3, classification: cls },
  ],
} as ScanSummary;

beforeEach(() => {
  vi.spyOn(api, "getItemDetails").mockReturnValue(new Promise(() => {}));
  useApp.setState({ summary, page: "explorer", storageView: "largest", explorerPath: null });
});

describe("LargeFilesPage", () => {
  it("opens a folder in Storage > Folders on double-click", () => {
    render(<LargeFilesPage />);
    const tabs = screen.getAllByRole("tab", { name: "Folders" });
    fireEvent.click(tabs[tabs.length - 1] as HTMLElement);
    fireEvent.doubleClick(screen.getByText("Downloads", { selector: "td" }));
    const s = useApp.getState();
    expect([s.storageView, s.explorerPath]).toEqual(["browse", "/Users/t/Downloads"]);
  });

  it("shows the folder that holds a file on Return", () => {
    render(<LargeFilesPage />);
    fireEvent.keyDown(screen.getByText("big.zip").closest("tr") as HTMLElement, { key: "Enter" });
    expect(useApp.getState().explorerPath).toBe("/Users/t/Downloads");
  });
});
