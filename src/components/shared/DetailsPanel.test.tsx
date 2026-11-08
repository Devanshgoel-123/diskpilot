import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

vi.mock("@/lib/api", () => ({
  api: {
    getItemDetails: () => new Promise(() => undefined),
    revealInFinder: () => Promise.resolve(null),
    getAppBadges: () => Promise.resolve({}),
  },
  errorMessage: (e: unknown) => String(e),
}));

import { setOs } from "@/lib/platform";
import { useApp } from "@/lib/store";

import { DetailsPanel } from "./DetailsPanel";

afterEach(() => {
  cleanup();
  setOs("macos");
});

describe("DetailsPanel on both platforms", () => {
  it("reveals in Finder and shows ~/ paths on macOS", () => {
    useApp.setState({ env: { homeDir: "/Users/me" } as never });
    render(<DetailsPanel path="/Users/me/Downloads/big.iso" onClose={() => undefined} />);
    expect(screen.getByRole("button", { name: /Reveal in Finder/ })).toBeVisible();
    expect(screen.getByText("~/Downloads/big.iso")).toBeVisible();
    expect(screen.getByText("big.iso")).toBeVisible();
  });

  it("shows in File Explorer and shows ~\\ paths on Windows", () => {
    setOs("windows");
    useApp.setState({ env: { homeDir: "C:\\Users\\Me" } as never });
    render(<DetailsPanel path="C:\Users\Me\Downloads\big.iso" onClose={() => undefined} />);
    expect(screen.getByRole("button", { name: /Show in File Explorer/ })).toBeVisible();
    expect(screen.getByText("~\\Downloads\\big.iso")).toBeVisible();
    expect(screen.getByText("big.iso")).toBeVisible();
  });
});
