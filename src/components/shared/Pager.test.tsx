import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { usePage } from "@/lib/usePage";

import { Pager } from "./Pager";

function List({ n, filter = "" }: { n: number; filter?: string }) {
  const items = Array.from({ length: n }, (_, i) => `item-${i}`);
  const page = usePage(items, 100, filter);
  return (
    <>
      <ul>
        {page.items.map((s) => (
          <li key={s}>{s}</li>
        ))}
      </ul>
      <Pager page={page} label="files" />
    </>
  );
}

describe("Pager", () => {
  it("shows one page at a time and moves between pages", () => {
    render(<List n={2000} />);
    expect(screen.getAllByRole("listitem")).toHaveLength(100);
    expect(screen.getByText("1–100 of 2,000")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Previous page" })).toBeDisabled();

    fireEvent.click(screen.getByRole("button", { name: "Next page" }));
    expect(screen.getByText("101–200 of 2,000")).toBeInTheDocument();
    expect(screen.getByText("item-100")).toBeInTheDocument();
    expect(screen.queryByText("item-0")).toBeNull();
  });

  it("ends on a short last page", () => {
    render(<List n={250} />);
    fireEvent.click(screen.getByRole("button", { name: "Next page" }));
    fireEvent.click(screen.getByRole("button", { name: "Next page" }));
    expect(screen.getByText("201–250 of 250")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Next page" })).toBeDisabled();
  });

  it("goes back to the first page when the filter changes", () => {
    const { rerender } = render(<List n={2000} filter="a" />);
    fireEvent.click(screen.getByRole("button", { name: "Next page" }));
    rerender(<List n={2000} filter="b" />);
    expect(screen.getByText("1–100 of 2,000")).toBeInTheDocument();
  });

  it("stays on a page that still exists when the list shrinks", () => {
    const { rerender } = render(<List n={500} />);
    for (let i = 0; i < 4; i++) fireEvent.click(screen.getByRole("button", { name: "Next page" }));
    rerender(<List n={150} />);
    expect(screen.getByText("101–150 of 150")).toBeInTheDocument();
  });

  it("is hidden for a single page", () => {
    render(<List n={40} />);
    expect(screen.queryByRole("navigation")).toBeNull();
  });
});
