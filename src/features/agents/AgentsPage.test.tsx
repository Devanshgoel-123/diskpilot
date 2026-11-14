import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { AgentReport, AgentsReport } from "@/lib/types";

const now = 1_800_000_000_000;
const day = 86_400_000;

function agent(over: Partial<AgentReport>): AgentReport {
  return {
    id: "claude-code",
    name: "Claude Code",
    detected: true,
    version: "2.1.325",
    permissions: [],
    connections: [],
    plugins: [],
    skills: [],
    cleanup: [],
    usageNote: "Skill use comes from Claude Code's own record.",
    totalBytes: 2_700_000_000,
    ...over,
  };
}

const skill = (name: string, over: Partial<AgentReport["skills"][number]> = {}) => ({
  id: `claude-code:/h/.claude/skills/${name}`,
  name,
  description: `${name} skill`,
  path: `/h/.claude/skills/${name}`,
  bytes: 4096,
  origin: "user" as const,
  plugin: null,
  lastUsedMs: now - day,
  uses: 3,
  state: "active" as const,
  recommendationId: null,
  ...over,
});

let report: AgentsReport;
const setConnection = vi.fn();
const setPlugin = vi.fn();
const removePermission = vi.fn();

vi.mock("@/lib/api", () => ({
  api: {
    getAgentsReport: () => Promise.resolve(null),
    analyzeAgents: () => Promise.resolve(report),
    getRecommendations: () =>
      Promise.resolve([{ id: "/h/.claude/skills/old", methods: ["trash"] }]),
    agentSetConnection: (...a: unknown[]) => setConnection(...a),
    agentSetPlugin: (...a: unknown[]) => setPlugin(...a),
    agentRemovePermissions: (...a: unknown[]) => removePermission(...a),
    agentReconnect: vi.fn(),
    agentForgetDisconnected: vi.fn(),
    getAppBadges: () => Promise.resolve({}),
  },
  errorMessage: (e: unknown) => (e as { message?: string }).message ?? String(e),
}));

import { useApp } from "@/lib/store";

import { useAgents } from "./agentsStore";
import { AgentsPage } from "./AgentsPage";

beforeEach(() => {
  setConnection.mockReset();
  setPlugin.mockReset();
  removePermission.mockReset();
  useAgents.setState({
    report: null,
    loading: false,
    busy: null,
    error: null,
    selected: null,
    tab: "permissions",
  });
  report = {
    generatedAtMs: now,
    disconnected: [],
    agents: [
      agent({
        permissions: [
          {
            id: "claude-code|.claude/settings.json|allow|Read",
            kind: "allow",
            label: "Read",
            value: "Read",
            source: "~/.claude/settings.json",
            risk: null,
          },
          {
            id: "claude-code|.claude/settings.json|allow|Bash(npm test)",
            kind: "allow",
            label: "Bash",
            value: "Bash(npm test)",
            source: "~/.claude/settings.json",
            risk: null,
          },
          {
            id: "claude-code|.claude/settings.json|allow|Bash(git status)",
            kind: "allow",
            label: "Bash",
            value: "Bash(git status)",
            source: "~/.claude/settings.json",
            risk: null,
          },
          {
            id: "claude-code|.claude/settings.json|allow|Bash(*)",
            kind: "allow",
            label: "Bash",
            value: "Bash(*)",
            source: "~/.claude/settings.json",
            risk: "Runs any shell command without asking.",
          },
        ],
        connections: [
          {
            id: "claude-code|user|github",
            name: "github",
            transport: "Local command",
            target: "npx @x/github",
            scope: "All projects",
            enabled: true,
            source: "~/.claude.json",
          },
        ],
        plugins: [
          {
            id: "pm@market",
            name: "pm",
            publisher: "market",
            version: "1.0",
            enabled: true,
            bytes: 1000,
            lastUsedMs: null,
            path: null,
            kind: "plugin",
          },
        ],
        skills: [
          skill("fresh"),
          skill("old", {
            state: "inactive",
            lastUsedMs: now - 60 * day,
            recommendationId: "/h/.claude/skills/old",
          }),
          ...Array.from({ length: 30 }, (_, i) =>
            skill(`pm-${i}`, {
              origin: "plugin",
              plugin: "pm",
              state: "inactive",
              lastUsedMs: null,
            }),
          ),
        ],
      }),
      agent({ id: "antigravity", name: "Antigravity", detected: false }),
    ],
  };
});

async function open() {
  render(<AgentsPage />);
  await screen.findByRole("heading", { name: "Claude Code" });
}

describe("AI Agents page", () => {
  it("shows only installed agents and puts risky permissions first", async () => {
    await open();
    expect(screen.queryByText("Antigravity")).toBeNull();
    expect(screen.queryByRole("radiogroup", { name: "Agent" })).toBeNull();
    expect(screen.getByRole("tab", { name: /Permissions.*1 needs a look/ })).toBeVisible();
    expect(screen.getByText("Needs a look")).toBeVisible();
    expect(screen.getByText("Runs any shell command without asking.")).toBeVisible();
  });

  it("explains approved commands in plain words and clears one-time approvals together", async () => {
    removePermission.mockResolvedValue(report);
    await open();
    expect(screen.getByText("Commands you approved once")).toBeVisible();
    expect(screen.getByText("npm test")).toBeVisible();
    expect(screen.getAllByText("Terminal").length).toBeGreaterThan(0);
    fireEvent.click(screen.getByRole("button", { name: "Clear all 2" }));
    const dialog = await screen.findByRole("dialog");
    await act(async () => {
      fireEvent.click(within(dialog).getByRole("button", { name: "Clear approvals" }));
    });
    expect(removePermission).toHaveBeenCalledWith("claude-code", [
      "claude-code|.claude/settings.json|allow|Bash(npm test)",
      "claude-code|.claude/settings.json|allow|Bash(git status)",
    ]);
  });

  it("asks before removing a rule and sends only the rule's id", async () => {
    removePermission.mockResolvedValue(report);
    await open();
    fireEvent.click(screen.getByRole("button", { name: "Remove rule Bash(*)" }));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText(/will ask you again before doing this/)).toBeVisible();
    expect(removePermission).not.toHaveBeenCalled();
    await act(async () => {
      fireEvent.click(within(dialog).getByRole("button", { name: "Remove" }));
    });
    expect(removePermission).toHaveBeenCalledWith("claude-code", [
      "claude-code|.claude/settings.json|allow|Bash(*)",
    ]);
  });

  it("disconnects a server after confirming and shows a failure without closing the page", async () => {
    setConnection.mockRejectedValue({ message: "~/.claude.json kept changing" });
    await open();
    fireEvent.click(screen.getByRole("tab", { name: /MCP servers/ }));
    fireEvent.click(screen.getByRole("button", { name: "Disconnect github" }));
    const dialog = await screen.findByRole("dialog");
    await act(async () => {
      fireEvent.click(within(dialog).getByRole("button", { name: "Disconnect" }));
    });
    expect(setConnection).toHaveBeenCalledWith("claude-code", "claude-code|user|github", false);
    await waitFor(() => expect(screen.getByRole("alert")).toHaveTextContent("kept changing"));
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("groups inactive plugin skills by plugin instead of listing each one", async () => {
    await open();
    fireEvent.click(screen.getByRole("tab", { name: /Skills/ }));
    expect(screen.getByRole("tab", { name: /Inactive\s*31/ })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    expect(screen.getByText("old")).toBeVisible();
    expect(screen.queryByText("pm-0")).toBeNull();
    expect(screen.getByText("None of its 30 skills used in 30 days")).toBeVisible();
    expect(screen.getByRole("button", { name: "Turn off plugin" })).toBeVisible();
  });

  it("sends inactive skills to the regular review instead of deleting them", async () => {
    await open();
    await waitFor(() => expect(useApp.getState().recommendations.length).toBe(1));
    fireEvent.click(screen.getByRole("tab", { name: /Skills/ }));
    fireEvent.click(screen.getByRole("button", { name: /Review 1 inactive skill/ }));
    expect(useApp.getState().page).toBe("recommendations");
    expect(useApp.getState().preselect).toEqual(["/h/.claude/skills/old"]);
  });
});
