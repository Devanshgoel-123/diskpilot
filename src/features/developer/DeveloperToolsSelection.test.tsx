import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import type { DevToolsReport, Recommendation } from "@/lib/types";

vi.mock("@/lib/api", () => ({
  api: new Proxy({}, { get: () => () => Promise.resolve(null) }),
  errorMessage: (e: unknown) => String(e),
}));

import { TooltipProvider } from "@/components/ui/tooltip";
import { useApp } from "@/lib/store";

import { DeveloperToolsPage } from "./DeveloperToolsPage";

function rec(id: string): Recommendation {
  return {
    id,
    title: id,
    path: `/home/me/ws/${id}`,
    bytes: 1000,
    fileCount: 1,
    tier: "reviewFirst",
    confidence: "high",
    classification: {
      category: "applicationData",
      label: id,
      explanation: "",
      ruleId: "windsurf.workspace-storage",
      confidence: "high",
      matchedPath: null,
    },
    detectedBecause: "",
    safeBecause: "",
    risks: "",
    proposedAction: "",
    methods: ["trash"],
    alternative: null,
    estimatedReclaimBytes: 1000,
    lastActivityMs: null,
    notes: [],
    source: "windsurf",
    clearInApp: null,
  };
}

const report = {
  generatedAtMs: 0,
  tools: [
    {
      id: "windsurf",
      name: "Windsurf",
      detected: true,
      version: null,
      totalBytes: 4000,
      summary: "3 leftovers.",
      sections: [
        {
          title: "Workspace storage",
          description: "",
          items: ["a", "b", "c"].map((id) => ({
            label: `project ${id}`,
            path: `/home/me/ws/${id}`,
            bytes: 1000,
            detail: "",
            state: "orphaned" as const,
            lastUsedMs: null,
            recommendationId: id,
          })),
        },
      ],
    },
  ],
} as unknown as DevToolsReport;

afterEach(() => {
  cleanup();
  useApp.setState({ preselect: null });
});

describe("choosing what to review in Developer Tools", () => {
  it("selects all, lets some be unticked, and reviews only the rest", () => {
    useApp.setState({
      devtools: report,
      recommendations: ["a", "b", "c"].map(rec),
      page: "developer",
    });
    render(
      <TooltipProvider>
        <DeveloperToolsPage />
      </TooltipProvider>,
    );
    fireEvent.click(screen.getByRole("button", { name: "Details" }));

    fireEvent.click(screen.getByRole("checkbox", { name: "Select all 3 in Workspace storage" }));
    expect(screen.getByText(/selected,/)).toHaveTextContent("3 selected");

    fireEvent.click(screen.getByRole("checkbox", { name: "Select project b" }));
    const all = screen.getByRole("checkbox", { name: "Select all 3 in Workspace storage" });
    expect((all as HTMLInputElement).indeterminate).toBe(true);

    fireEvent.click(screen.getByRole("button", { name: "Review 2 selected" }));
    expect(useApp.getState().preselect).toEqual(["a", "c"]);
    expect(useApp.getState().page).toBe("recommendations");
  });
});
