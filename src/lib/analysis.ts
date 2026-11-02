/**
 * Pure derivations over scan results. Kept separate from components so the
 * numbers shown in the UI are unit-tested.
 */
import type {
  Category,
  CategoryTotal,
  CleanupMethod,
  DetectedLocation,
  Recommendation,
} from "./types";

/** True if `child` is strictly inside `parent` (component-wise). */
export function isInside(child: string, parent: string): boolean {
  if (child === parent) return false;
  const p = parent.endsWith("/") ? parent : parent + "/";
  return child.startsWith(p);
}

/** Drop items nested inside another item of the list, so sizes can be summed. */
export function outermost<T extends { path: string }>(items: T[]): T[] {
  return items.filter((a) => !items.some((b) => isInside(a.path, b.path)));
}

export function sumBytes(items: { bytes: number }[]): number {
  return items.reduce((acc, i) => acc + i.bytes, 0);
}

/** Categories that are usually regenerable. Still always subject to review. */
export const REGENERABLE: readonly Category[] = ["cache", "buildArtifacts"];

export function regenerableBytes(categories: CategoryTotal[]): number {
  return sumBytes(categories.filter((c) => REGENERABLE.includes(c.category)));
}

/** Specific (non-container) detections, largest first. */
export function topConsumers(locations: DetectedLocation[], limit: number): DetectedLocation[] {
  return locations
    .filter((l) => !l.container)
    .sort((a, b) => b.bytes - a.bytes)
    .slice(0, limit);
}

export interface ToolGroup {
  id: string;
  name: string;
  description: string;
  locations: DetectedLocation[];
  /** Total without double-counting nested detections. */
  bytes: number;
}

const TOOL_DEFS: { id: string; name: string; description: string; prefixes: string[] }[] = [
  {
    id: "docker",
    name: "Docker",
    description: "Container VM disks and CLI configuration.",
    prefixes: ["docker."],
  },
  {
    id: "xcode",
    name: "Xcode & Apple tools",
    description: "DerivedData, archives, device support and simulators.",
    prefixes: ["xcode.", "apple.", "cocoapods."],
  },
  {
    id: "node",
    name: "Node.js",
    description: "node_modules, package manager caches, framework builds and runtimes.",
    prefixes: ["node.", "npm.", "pnpm.", "yarn.", "bun.", "nvm."],
  },
  {
    id: "cursor",
    name: "Cursor",
    description: "App data, workspace storage, extensions and agent data.",
    prefixes: ["cursor."],
  },
  {
    id: "ai-agents",
    name: "AI coding agents",
    description: "Claude Code, Codex and Gemini CLI installs, sessions and data.",
    prefixes: ["ai."],
  },
  {
    id: "vscode",
    name: "VS Code",
    description: "App data, workspace storage and extensions.",
    prefixes: ["vscode."],
  },
  {
    id: "rust",
    name: "Rust",
    description: "Cargo build output, registry cache and toolchains.",
    prefixes: ["rust.", "cargo.", "rustup."],
  },
  {
    id: "python",
    name: "Python",
    description: "Virtual environments and bytecode caches.",
    prefixes: ["python."],
  },
  {
    id: "other",
    name: "Other developer storage",
    description: "Gradle, Maven, Android, Homebrew, CLI caches and Git data.",
    prefixes: ["gradle.", "maven.", "android.", "homebrew.", "xdg.", "git."],
  },
];

export function groupByTool(locations: DetectedLocation[]): ToolGroup[] {
  return TOOL_DEFS.map((def) => {
    const matched = locations
      .filter((l) => def.prefixes.some((p) => l.classification.ruleId.startsWith(p)))
      .sort((a, b) => b.bytes - a.bytes);
    return {
      id: def.id,
      name: def.name,
      description: def.description,
      locations: matched,
      bytes: sumBytes(outermost(matched)),
    };
  }).sort((a, b) => b.bytes - a.bytes);
}

export interface SelectionSummary {
  count: number;
  bytes: number;
  /** Methods every selected item allows (permanent delete only if all do). */
  methods: CleanupMethod[];
}

export function summarizeSelection(recs: Recommendation[], ids: Set<string>): SelectionSummary {
  const selected = recs.filter((r) => ids.has(r.id) && r.methods.length > 0);
  const methods: CleanupMethod[] = selected.length
    ? (["trash", "delete"] as const).filter((m) => selected.every((r) => r.methods.includes(m)))
    : [];
  return {
    count: selected.length,
    bytes: sumBytes(selected.map((r) => ({ bytes: r.estimatedReclaimBytes }))),
    methods,
  };
}
