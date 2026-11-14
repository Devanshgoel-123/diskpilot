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
import { isInside } from "./paths";
import { isWindows } from "./platform";

/** True if `child` is strictly inside `parent` (component-wise, either separator). */
export { isInside };

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
    name: "Docker & containers",
    description: "Docker, OrbStack, Colima, Podman and Minikube storage.",
    prefixes: ["docker.", "podman.", "minikube."],
  },
  {
    id: "xcode",
    name: "Xcode & Apple tools",
    description: "DerivedData, archives, device support and simulators.",
    prefixes: ["xcode.", "apple.", "cocoapods.", "swiftpm."],
  },
  {
    id: "node",
    name: "Node.js",
    description: "node_modules, package manager caches, framework builds and runtimes.",
    prefixes: ["node.", "npm.", "pnpm.", "yarn.", "bun.", "deno.", "nvm.", "puppeteer."],
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
    id: "windsurf",
    name: "Windsurf",
    description: "App data, workspace storage, extensions and AI data.",
    prefixes: ["windsurf."],
  },
  {
    id: "flutter",
    name: "Flutter & Dart",
    description: "Pub package cache, build output and .dart_tool folders.",
    prefixes: ["flutter."],
  },
  {
    id: "game-engines",
    name: "Unity & Unreal",
    description: "Unity Library folders and Unreal Derived Data Caches.",
    prefixes: ["unity.", "unreal."],
  },
  {
    id: "rust",
    name: "Rust",
    description: "Cargo build output, registry cache and toolchains.",
    prefixes: ["rust.", "cargo.", "rustup."],
  },
  {
    id: "go",
    name: "Go",
    description: "Module cache, build cache and GOPATH.",
    prefixes: ["go."],
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
    get description() {
      return isWindows()
        ? "Gradle, Maven, NuGet, Android, CLI caches and Git data."
        : "Gradle, Maven, NuGet, Android, Homebrew, CLI caches and Git data.";
    },
    prefixes: ["gradle.", "maven.", "nuget.", "android.", "homebrew.", "xdg.", "git."],
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

/** Bytes of Likely safe, actionable recommendations, per category. */
export function recoverableByCategory(recs: Recommendation[]): Map<Category, number> {
  const out = new Map<Category, number>();
  for (const r of recs) {
    if (r.tier !== "likelySafe" || r.methods.length === 0) continue;
    const c = r.classification.category;
    out.set(c, (out.get(c) ?? 0) + r.estimatedReclaimBytes);
  }
  return out;
}

/**
 * The few items most worth a look: the largest actionable recommendations,
 * Likely safe first, with the single largest Review first item included.
 */
export function pickStartHere(recs: Recommendation[], limit = 5): Recommendation[] {
  const actionable = recs.filter((r) => r.methods.length > 0 && r.tier !== "keep");
  const bySize = (x: Recommendation, y: Recommendation) =>
    y.estimatedReclaimBytes - x.estimatedReclaimBytes;
  const safe = actionable.filter((r) => r.tier === "likelySafe").sort(bySize);
  const review = actionable.filter((r) => r.tier === "reviewFirst").sort(bySize);
  return [...safe.slice(0, limit - 1), ...review.slice(0, 1)].sort(bySize).slice(0, limit);
}
