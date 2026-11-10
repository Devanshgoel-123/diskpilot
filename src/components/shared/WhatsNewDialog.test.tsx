import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { shortcutsNewIn, useShortcutsDialog } from "@/lib/shortcuts";

import { WhatsNewDialog } from "./WhatsNewDialog";

const notes = [
  "DiskPilot 0.6.3 is faster.",
  "",
  "## What's changed",
  "",
  "### Faster",
  "",
  "- Quick.",
  "",
  "### Keyboard shortcuts",
  "",
  "- ⌘R scans again.",
].join("\n");

describe("WhatsNewDialog", () => {
  it("shows the version's new shortcuts in their own panel instead of as a list", () => {
    render(<WhatsNewDialog open title="t" notes={notes} version="0.6.3" onClose={() => {}} />);
    const panel = screen.getByRole("region", { name: "New keyboard shortcuts" });
    expect(panel).toHaveTextContent("Go to a page in the sidebar");
    expect(panel).toHaveTextContent("Scan again");
    expect(screen.queryByText("⌘R scans again.")).toBeNull();
    expect(screen.queryByText("What's changed")).toBeNull();
    expect(screen.getByText("DiskPilot 0.6.3 is faster.")).toBeInTheDocument();
  });

  it("opens the full shortcuts list from the panel", () => {
    const onClose = vi.fn();
    render(<WhatsNewDialog open title="t" notes={notes} version="0.6.3" onClose={onClose} />);
    fireEvent.click(screen.getByRole("button", { name: /See all/ }));
    expect(onClose).toHaveBeenCalled();
    expect(useShortcutsDialog.getState().open).toBe(true);
    useShortcutsDialog.setState({ open: false });
  });

  it("keeps the notes as written for a version without new shortcuts", () => {
    expect(shortcutsNewIn("0.5.0")).toHaveLength(0);
    render(<WhatsNewDialog open title="t" notes={notes} version="0.5.0" onClose={() => {}} />);
    expect(screen.queryByRole("region", { name: "New keyboard shortcuts" })).toBeNull();
    expect(screen.getByText("⌘R scans again.")).toBeInTheDocument();
  });
});
