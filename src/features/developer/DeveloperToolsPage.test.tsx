import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import type { ToolReport } from "@/lib/types";

vi.mock("@/lib/api", () => ({
  api: { getAppBadges: () => Promise.resolve({}) },
  errorMessage: (e: unknown) => String(e),
}));

import { TooltipProvider } from "@/components/ui/tooltip";
import { useApp } from "@/lib/store";

import { DeveloperToolsPage } from "./DeveloperToolsPage";

const tool = (id: string, name: string): ToolReport => ({
  id,
  name,
  detected: true,
  version: null,
  totalBytes: 1_000_000,
  summary: `${name} summary`,
  sections: [],
});

describe("Developer Tools", () => {
  it("leaves AI agents to the AI Agents page and links there", () => {
    useApp.setState({
      devtools: {
        generatedAtMs: 1,
        reclaimableBytes: 0,
        tools: [
          tool("claude-code", "Claude Code"),
          tool("copilot", "GitHub Copilot"),
          tool("node", "Node.js"),
        ],
      },
      analyzingDevTools: false,
      recommendations: [],
      page: "developer",
    });
    render(
      <TooltipProvider>
        <DeveloperToolsPage />
      </TooltipProvider>,
    );
    expect(screen.getByText("Node.js")).toBeVisible();
    expect(screen.queryByText("Claude Code summary")).toBeNull();
    expect(screen.queryByText("GitHub Copilot")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: /AI agents are on the AI Agents page/ }));
    expect(useApp.getState().page).toBe("agents");
  });
});
