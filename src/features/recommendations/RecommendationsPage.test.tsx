import { cleanup, render } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import type { Recommendation, ScanSummary } from "@/lib/types";

vi.mock("@/lib/api", () => ({
  api: new Proxy({}, { get: () => () => Promise.resolve(null) }),
  errorMessage: (e: unknown) => String(e),
  onScanProgress: () => Promise.resolve(() => {}),
}));

import { useApp } from "@/lib/store";

import { RecommendationsPage } from "./RecommendationsPage";

function rec(id: string, bytes: number): Recommendation {
  return {
    id,
    title: id,
    path: id,
    bytes,
    fileCount: 1,
    tier: "likelySafe",
    confidence: "high",
    classification: {
      category: "cache",
      label: id,
      explanation: "",
      ruleId: "test",
      confidence: "high",
      matchedPath: null,
    },
    detectedBecause: "",
    safeBecause: "",
    risks: "",
    proposedAction: "",
    methods: ["trash"],
    alternative: null,
    estimatedReclaimBytes: bytes,
    lastActivityMs: null,
    notes: [],
    source: "scan",
    clearInApp: null,
  };
}

afterEach(() => {
  cleanup();
  useApp.setState({ preselect: null });
});

describe("items handed over from another page", () => {
  it("are selected and scrolled into view", () => {
    const scrolled: string[] = [];
    Element.prototype.scrollIntoView = vi.fn(function (this: Element) {
      scrolled.push((this as HTMLElement).dataset.recId ?? "");
    });
    useApp.setState({
      summary: { root: "/home" } as unknown as ScanSummary,
      recommendations: [rec("big", 3e9), rec("middle", 2e9), rec("picked", 1e9)],
      preselect: ["picked"],
    });
    const { container } = render(<RecommendationsPage />);
    expect(scrolled).toEqual(["picked"]);
    const row = container.querySelector('[data-rec-id="picked"] input[type="checkbox"]');
    expect((row as HTMLInputElement).checked).toBe(true);
  });

  it("do not move the list when nothing was handed over", () => {
    const spy = vi.fn();
    Element.prototype.scrollIntoView = spy;
    useApp.setState({
      summary: { root: "/home" } as unknown as ScanSummary,
      recommendations: [rec("big", 3e9)],
      preselect: null,
    });
    render(<RecommendationsPage />);
    expect(spy).not.toHaveBeenCalled();
  });
});

describe("small Developer Tools leftovers", () => {
  const tiny = (id: string) => ({ ...rec(id, 4096), source: "windsurf" });

  it("are not listed until handed over from Developer Tools", () => {
    Element.prototype.scrollIntoView = vi.fn();
    useApp.setState({
      summary: { root: "/home" } as unknown as ScanSummary,
      recommendations: [rec("big", 3e9), tiny("ws1"), tiny("ws2")],
      preselect: null,
    });
    const { container, getByText } = render(<RecommendationsPage />);
    expect(container.querySelectorAll("[data-rec-id]")).toHaveLength(1);
    getByText(/2 small leftovers under/);
  });

  it("are listed when Developer Tools hands them over", () => {
    Element.prototype.scrollIntoView = vi.fn();
    useApp.setState({
      summary: { root: "/home" } as unknown as ScanSummary,
      recommendations: [rec("big", 3e9), tiny("ws1"), tiny("ws2")],
      preselect: ["ws1"],
    });
    const { container } = render(<RecommendationsPage />);
    expect(container.querySelector('[data-rec-id="ws1"]')).not.toBeNull();
    expect(container.querySelector('[data-rec-id="ws2"]')).toBeNull();
  });
});
