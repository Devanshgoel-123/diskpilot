import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { Recommendation } from "@/lib/types";

const openOwnerApp = vi.fn();
/** What opening the app does next; kept out of the spy so a refusal is not reported twice. */
let opening: () => Promise<null> = () => Promise.resolve(null);
vi.mock("@/lib/api", () => ({
  api: {
    openOwnerApp: (ruleId: string) => {
      openOwnerApp(ruleId);
      return opening();
    },
    revealInFinder: () => Promise.resolve(null),
    getAppBadges: () => Promise.resolve({}),
  },
  errorMessage: (e: unknown) => (e instanceof Error ? e.message : String(e)),
}));

import { RecommendationRow } from "./RecommendationRow";

function telegram(over: Partial<Recommendation> = {}): Recommendation {
  return {
    id: "/Users/me/tg",
    title: "Telegram media cache",
    path: "/Users/me/tg",
    bytes: 1e9,
    fileCount: 1200,
    tier: "reviewFirst",
    confidence: "high",
    classification: {
      category: "cache",
      label: "Telegram media cache",
      explanation: "",
      ruleId: "chat.telegram-media",
      confidence: "high",
      matchedPath: null,
    },
    detectedBecause: "",
    safeBecause: "",
    risks: "",
    proposedAction: "In Telegram, open Settings › Data and Storage › Storage Usage",
    methods: [],
    alternative: null,
    estimatedReclaimBytes: 1e9,
    lastActivityMs: null,
    notes: [],
    source: "scan",
    clearInApp: "Telegram",
    ...over,
  };
}

function renderRow(rec: Recommendation) {
  render(
    <ul>
      <RecommendationRow
        rec={rec}
        max={rec.bytes}
        selected={false}
        expanded
        onToggleSelect={() => {}}
        onToggleExpand={() => {}}
      />
    </ul>,
  );
}

beforeEach(() => {
  openOwnerApp.mockReset();
  opening = () => Promise.resolve(null);
});
afterEach(cleanup);

describe("items cleared in their own app", () => {
  it("opens the app and offers no way to remove the files", async () => {
    renderRow(telegram());
    expect(screen.queryByRole("checkbox")).toBeNull();
    expect(screen.getByText(/Clear it in Telegram/)).toBeInTheDocument();
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Open Telegram" }));
    });
    expect(openOwnerApp).toHaveBeenCalledWith("chat.telegram-media");
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("says so when the app cannot be opened", async () => {
    opening = () => Promise.reject(new Error("Telegram is not installed on this Mac."));
    renderRow(telegram());
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Open Telegram" }));
    });
    expect(screen.getByRole("alert")).toHaveTextContent("Telegram is not installed");
  });

  it("shows no app button for other items", () => {
    renderRow(telegram({ clearInApp: null, methods: ["trash"] }));
    expect(screen.queryByRole("button", { name: "Open Telegram" })).toBeNull();
  });
});
