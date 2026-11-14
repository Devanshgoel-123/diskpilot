import type { AgentPermission } from "@/lib/types";

/** What part of the computer a rule is about, in plain words. */
export function ruleArea(rule: string): string {
  const tool = rule.split("(")[0] ?? rule;
  if (tool.startsWith("mcp__")) return `MCP · ${tool.slice(5).split("__")[0]}`;
  switch (tool) {
    case "Bash":
    case "Shell":
      return "Terminal";
    case "Read":
    case "Glob":
    case "Grep":
      return "Read files";
    case "Edit":
    case "Write":
    case "MultiEdit":
    case "NotebookEdit":
      return "Edit files";
    case "WebFetch":
      return "Websites";
    case "WebSearch":
      return "Web search";
    case "Skill":
      return "Skill";
    default:
      return tool;
  }
}

/** The part inside `Tool(...)`, or null for a rule that covers the whole tool. */
function inner(rule: string): string | null {
  const open = rule.indexOf("(");
  if (open < 0 || !rule.endsWith(")")) return null;
  return rule.slice(open + 1, -1).trim();
}

/** The rule without its tool name, with wildcards written as "…". */
export function ruleText(rule: string): string {
  const i = inner(rule);
  if (i === null || i === "*" || i === "**") return "Anything";
  return i
    .replace(/^domain:/, "")
    .replace(/:\*$/, " …")
    .replace(/ \*$/, " …")
    .replace(/\*\*/g, "…")
    .replace(/\*/g, "…");
}

/**
 * An exact command approved once ("Yes, don't ask again"), as opposed to a
 * pattern or a whole tool. These pile up and are the safest to clear.
 */
export function isOneTime(p: AgentPermission): boolean {
  if (p.kind !== "allow") return false;
  const i = inner(p.value);
  if (i === null || i.startsWith("domain:")) return false;
  if (!i.includes("*")) return true;
  // `git add *` is a real pattern. `git commit -m 'Fix the header *` was
  // approved for one command and only ends in a wildcard.
  const before = i.slice(0, i.indexOf("*")).trim();
  return before.split(/\s+/).length > 3 || /['"]/.test(before);
}

/** Settings values like `disabled` or `bypassPermissions`, in plain words. */
export function modeValue(value: string): string {
  const v = value.trim();
  const known: Record<string, string> = {
    disabled: "Off",
    off: "Off",
    none: "Off",
    enabled: "On",
    true: "On",
    false: "Off",
    allowlist: "Asks unless allowed",
    default: "Asks first",
    acceptEdits: "Edits files without asking",
    plan: "Plans only",
    bypassPermissions: "Never asks",
    never: "Never asks",
    "on-request": "Asks when it needs to",
    "on-failure": "Asks when a command fails",
    untrusted: "Asks for anything risky",
    "read-only": "Read only",
    "workspace-write": "Can edit the project",
    "danger-full-access": "Full access",
    user_config_with_defaults: "Your settings",
  };
  return known[v] ?? v.replace(/[_-]+/g, " ");
}
