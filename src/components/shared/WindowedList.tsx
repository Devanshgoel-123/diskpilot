import { type ReactNode, useEffect, useRef, useState } from "react";

import { cn } from "@/lib/utils";

/**
 * A scrolling list that only renders the rows in view (plus a few either
 * side), so a review of thousands of items opens instantly. Every row must be
 * exactly `rowHeight` pixels tall. Short lists render in full.
 */
export function WindowedList<T>({
  items,
  rowHeight,
  maxHeight,
  getKey,
  renderRow,
  className,
  label,
  overscan = 8,
  windowAbove = 60,
}: {
  items: T[];
  rowHeight: number;
  /**
   * Height of the scrolling area once the list is longer than it, or "fill"
   * to take the height of the parent (which must give it one).
   */
  maxHeight: number | "fill";
  getKey: (item: T) => string;
  renderRow: (item: T, index: number) => ReactNode;
  className?: string;
  label?: string;
  overscan?: number;
  /** Lists up to this many rows render in full. */
  windowAbove?: number;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [scrollTop, setScrollTop] = useState(0);
  const [filled, setFilled] = useState(600);
  const fill = maxHeight === "fill";
  const windowed = fill || items.length > windowAbove;
  const total = items.length * rowHeight;
  const height = fill ? filled : Math.min(total, maxHeight);

  // In fill mode, follow the parent's height as the window resizes.
  useEffect(() => {
    const el = ref.current;
    if (!fill || !el || typeof ResizeObserver === "undefined") return;
    const ro = new ResizeObserver(() => setFilled(el.clientHeight || 600));
    ro.observe(el);
    return () => ro.disconnect();
  }, [fill]);

  // A new, shorter list must not stay scrolled past its end.
  useEffect(() => {
    const el = ref.current;
    if (el && el.scrollTop > Math.max(0, total - height)) {
      el.scrollTop = 0;
      setScrollTop(0);
    }
  }, [total, height]);

  if (!windowed) {
    return (
      <ul aria-label={label} className={className}>
        {items.map((item, i) => (
          <li key={getKey(item)} style={{ height: rowHeight }}>
            {renderRow(item, i)}
          </li>
        ))}
      </ul>
    );
  }

  const first = Math.max(0, Math.floor(scrollTop / rowHeight) - overscan);
  const last = Math.min(items.length, Math.ceil((scrollTop + height) / rowHeight) + overscan);
  return (
    <div
      ref={ref}
      className={cn("overflow-y-auto", fill && "h-full", className)}
      style={fill ? undefined : { height }}
      onScroll={(e) => setScrollTop(e.currentTarget.scrollTop)}
    >
      <ul aria-label={label} className="relative" style={{ height: total }}>
        {items.slice(first, last).map((item, i) => (
          <li
            key={getKey(item)}
            aria-setsize={items.length}
            aria-posinset={first + i + 1}
            className="absolute inset-x-0"
            style={{ top: (first + i) * rowHeight, height: rowHeight }}
          >
            {renderRow(item, first + i)}
          </li>
        ))}
      </ul>
    </div>
  );
}
