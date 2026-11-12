// Deterministic, synthetic scan data for the performance harness. Nothing here
// comes from a real disk: names, sizes and dates are generated from a seed, so
// every run measures the same work.
import type { Category } from "../src/lib/types";

export const HOME = "/Users/perf";

function rng(seed: number) {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

const CATS: Category[] = [
  "cache",
  "buildArtifacts",
  "developerTools",
  "dockerStorage",
  "applicationData",
  "logs",
  "downloads",
  "userData",
  "binaryData",
  "systemFiles",
];
const WORDS = [
  "alpha",
  "build",
  "cache",
  "data",
  "export",
  "final",
  "draft",
  "photo",
  "clip",
  "notes",
];
const EXT: Record<string, string[]> = {
  videos: ["mov", "mp4"],
  images: ["jpg", "heic", "png"],
  audio: ["m4a", "wav"],
  archives: ["zip", "tar.gz"],
  installers: ["dmg", "pkg"],
  documents: ["pdf", "key"],
};
const NOW = Date.UTC(2026, 10, 1);

export function makeFixture(seed = 7) {
  const r = rng(seed);
  const pick = <T>(xs: readonly T[]) => xs[Math.floor(r() * xs.length)] as T;
  const word = () => `${pick(WORDS)}-${Math.floor(r() * 1e4)}`;
  const cls = (category: Category, i: number) => ({
    category,
    label: `Rule ${i % 40}`,
    explanation: "Synthetic location for performance measurements.",
    ruleId: `perf.rule-${i % 40}`,
    confidence: "high" as const,
    matchedPath: `${HOME}/area-${i % 40}`,
  });
  const when = () => NOW - Math.floor(r() * 400) * 86_400_000;

  const largestFiles = Array.from({ length: 500 }, (_, i) => {
    const name = `${word()}.${pick(EXT.videos!)}`;
    const b = Math.floor(9e9 / (i + 1));
    return {
      path: `${HOME}/Movies/${i}/${name}`,
      name,
      bytes: b,
      logicalBytes: b,
      modifiedMs: when(),
      classification: cls(pick(CATS), i),
    };
  });
  const heavyDirectories = Array.from({ length: 200 }, (_, i) => ({
    path: `${HOME}/Projects/${word()}/${i}`,
    bytes: Math.floor(5e10 / (i + 1)),
    fileCount: 100 + i * 37,
    classification: cls(pick(CATS), i),
  }));
  const detectedLocations = Array.from({ length: 300 }, (_, i) => ({
    path: `${HOME}/Library/Caches/${word()}-${i}`,
    bytes: Math.floor(2e10 / (i + 1)),
    fileCount: 50 + i * 11,
    container: i % 9 === 0,
    classification: cls(CATS[i % CATS.length]!, i),
  }));
  const fileKinds = Object.keys(EXT).map((kind, k) => {
    const files = Array.from({ length: 2000 }, (_, i) => {
      const name = `${word()}.${pick(EXT[kind]!)}`;
      const b = Math.floor(4e9 / (i + 1 + k));
      return {
        path: `${HOME}/${kind}/${Math.floor(i / 50)}/${name}`,
        name,
        bytes: b,
        logicalBytes: b,
        modifiedMs: when(),
        classification: cls("userData", i),
      };
    });
    return {
      kind,
      bytes: files.reduce((s, f) => s + f.bytes, 0),
      fileCount: 2000 + k * 500,
      files,
    };
  });
  const categories = CATS.map((category, i) => ({
    category,
    bytes: Math.floor(4e10 / (i + 1)),
    fileCount: 10_000 * (i + 1),
  }));
  const totalBytes = categories.reduce((s, c) => s + c.bytes, 0);
  const summary = {
    id: "perf-scan",
    root: HOME,
    startedAtMs: NOW - 16_000,
    finishedAtMs: NOW,
    durationMs: 16_000,
    totalBytes,
    fileCount: 986_000,
    dirCount: 166_000,
    symlinkCount: 900,
    skippedVolumes: 0,
    errorCount: 40,
    errorSamples: Array.from({ length: 40 }, (_, i) => ({
      path: `${HOME}/Library/Private/${i}`,
      message: "Operation not permitted",
    })),
    categories,
    largestFiles,
    detectedLocations,
    heavyDirectories,
    fileKinds,
  };

  /** A folder listing as the engine sends it: at most 1,000 entries. */
  const listing = (path: string, count = 1000) => {
    const lr = rng(path.length * 31 + count);
    const entries = Array.from({ length: count }, (_, i) => {
      const dir = lr() < 0.6;
      const name = `${WORDS[i % WORDS.length]}-${i}${dir ? "" : ".bin"}`;
      return {
        name,
        path: `${path}/${name}`,
        kind: dir ? "directory" : "file",
        bytes: Math.floor(3e10 / (i + 1)),
        fileCount: dir ? 10 + i : null,
        modifiedMs: NOW - Math.floor(lr() * 400) * 86_400_000,
        extension: dir ? null : "bin",
        classification: cls(CATS[i % CATS.length]!, i),
      };
    });
    return {
      path,
      parent: path === HOME ? null : path.slice(0, path.lastIndexOf("/")),
      scanRoot: HOME,
      bytes: entries.reduce((s, e) => s + e.bytes, 0),
      classification: cls("userData", 0),
      entries,
      omittedCount: 0,
      omittedBytes: 0,
    };
  };

  const recommendations = detectedLocations.slice(0, 80).map((l, i) => ({
    id: `rec-${i}`,
    title: `Clear ${l.path.split("/").pop()}`,
    path: l.path,
    bytes: l.bytes,
    estimatedReclaimBytes: l.bytes,
    fileCount: l.fileCount,
    tier: i % 3 === 0 ? "reviewFirst" : "likelySafe",
    methods: ["trash", "delete"],
    classification: l.classification,
    confidence: "high",
    source: "classifier",
    detectedBecause: "Synthetic.",
    safeBecause: "Synthetic.",
    risks: "Synthetic.",
    proposedAction: "Move to the Trash",
    alternative: null,
    lastActivityMs: null,
    notes: [],
  }));

  const disk = {
    mountPoint: "/",
    fileSystem: "apfs",
    totalBytes: 494e9,
    usedBytes: 400e9,
    availableBytes: 94e9,
  };
  return { summary, listing, recommendations, disk };
}
