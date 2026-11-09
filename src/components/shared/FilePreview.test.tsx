import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

const openListedFile = vi.fn();
vi.mock("@/lib/api", () => ({
  api: {
    openListedFile: (p: string) => openListedFile(p),
    revealInFinder: vi.fn(),
  },
  errorMessage: (e: unknown) => (e instanceof Error ? e.message : String(e)),
}));
vi.mock("@/lib/thumbnails", () => ({
  cachedThumbnail: () => "blob:thumb",
  loadThumbnail: () => Promise.resolve("blob:thumb"),
}));

import { FilePreview, type PreviewFile } from "./FilePreview";

function show(files: PreviewFile[], onToggleSelect?: (p: string) => void) {
  return render(
    <FilePreview
      files={files}
      index={0}
      onIndexChange={() => {}}
      onClose={() => {}}
      onToggleSelect={onToggleSelect}
    />,
  );
}

beforeEach(() => {
  openListedFile.mockReset();
  openListedFile.mockResolvedValue(null);
});

describe("FilePreview", () => {
  it("opens the file when the image is clicked", () => {
    show([{ path: "/Users/me/Desktop/beach.png" }]);
    fireEvent.click(screen.getByRole("button", { name: "Open beach.png" }));
    expect(openListedFile).toHaveBeenCalledWith("/Users/me/Desktop/beach.png");
  });

  it("says why a file could not be opened", async () => {
    openListedFile.mockRejectedValueOnce(new Error("macOS has no app to open it"));
    show([{ path: "/Users/me/a.png" }]);
    fireEvent.click(screen.getByRole("button", { name: "Open a.png" }));
    await waitFor(() => expect(screen.getByRole("alert")).toHaveTextContent("no app"));
  });

  it("selects and unselects the file for the Trash", () => {
    const toggle = vi.fn();
    const { rerender } = show([{ path: "/Users/me/a.png", selected: false }], toggle);
    fireEvent.click(screen.getByRole("button", { name: "Select for Trash" }));
    expect(toggle).toHaveBeenCalledWith("/Users/me/a.png");

    rerender(
      <FilePreview
        files={[{ path: "/Users/me/a.png", selected: true }]}
        index={0}
        onIndexChange={() => {}}
        onClose={() => {}}
        onToggleSelect={toggle}
      />,
    );
    const button = screen.getByRole("button", { name: "Selected for Trash" });
    expect(button).toHaveAttribute("aria-pressed", "true");
    fireEvent.keyDown(button, { key: "Backspace", metaKey: true });
    expect(toggle).toHaveBeenCalledTimes(2);
  });

  it("offers no Trash button for the copy being kept", () => {
    show([{ path: "/Users/me/a.png", selectable: false }], vi.fn());
    expect(screen.queryByRole("button", { name: /for Trash/ })).toBeNull();
  });

  it("offers no Trash button when the page does not allow selecting", () => {
    show([{ path: "/Users/me/a.png" }]);
    expect(screen.queryByRole("button", { name: /for Trash/ })).toBeNull();
  });
});
