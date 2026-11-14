import { describe, expect, it } from "vitest";

import type { AgentPermission } from "@/lib/types";

import { isOneTime, modeValue, ruleArea, ruleText } from "./permissionText";

const allow = (value: string): AgentPermission => ({
  id: value,
  kind: "allow",
  label: "",
  value,
  source: "",
  risk: null,
});

describe("permission rules in plain words", () => {
  it("names the area and writes wildcards as an ellipsis", () => {
    expect(ruleArea("Bash(git add *)")).toBe("Terminal");
    expect(ruleArea("Shell(ls)")).toBe("Terminal");
    expect(ruleArea("mcp__github__create_issue")).toBe("MCP · github");
    expect(ruleText("Bash(git add *)")).toBe("git add …");
    expect(ruleText("Bash(npm run test:*)")).toBe("npm run test …");
    expect(ruleText("WebFetch(domain:docs.rs)")).toBe("docs.rs");
    expect(ruleText("Edit")).toBe("Anything");
  });

  it("tells one-time approvals from real patterns", () => {
    expect(isOneTime(allow("Bash(curl -s http://localhost:3000/)"))).toBe(true);
    expect(isOneTime(allow("Bash(git commit -q -m 'Render the hero at half resolution *)"))).toBe(
      true,
    );
    expect(isOneTime(allow("Bash(git add *)"))).toBe(false);
    expect(isOneTime(allow("Bash(npm uninstall *)"))).toBe(false);
    expect(isOneTime(allow("Read"))).toBe(false);
    expect(isOneTime(allow("WebFetch(domain:docs.rs)"))).toBe(false);
  });

  it("reads settings values as words", () => {
    expect(modeValue("disabled")).toBe("Off");
    expect(modeValue("bypassPermissions")).toBe("Never asks");
    expect(modeValue("user_config_with_defaults")).toBe("Your settings");
  });
});
