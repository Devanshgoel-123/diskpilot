import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { WindowedList } from "./WindowedList";

const items = Array.from({ length: 2000 }, (_, i) => `item-${i}`);

function list(n = items.length) {
  return render(
    <WindowedList
      items={items.slice(0, n)}
      rowHeight={50}
      maxHeight={500}
      label="Items"
      getKey={(s) => s}
      renderRow={(s) => <span>{s}</span>}
    />,
  );
}

describe("WindowedList", () => {
  it("renders only the rows in view for a long list", () => {
    list();
    const rows = screen.getAllByRole("listitem");
    // 10 rows fit, plus the overscan below.
    expect(rows.length).toBeLessThan(30);
    expect(rows[0]).toHaveAttribute("aria-setsize", "2000");
    expect(screen.getByText("item-0")).toBeInTheDocument();
    expect(screen.queryByText("item-1999")).toBeNull();
  });

  it("follows the scroll position", () => {
    const { container } = list();
    const scroller = container.firstElementChild as HTMLElement;
    scroller.scrollTop = 50 * 1500;
    fireEvent.scroll(scroller);
    expect(screen.getByText("item-1500")).toBeInTheDocument();
    expect(screen.queryByText("item-0")).toBeNull();
  });

  it("renders a short list in full", () => {
    list(12);
    expect(screen.getAllByRole("listitem")).toHaveLength(12);
  });
});
