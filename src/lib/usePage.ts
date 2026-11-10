import { type RefObject, useEffect, useState } from "react";

export interface Page<T> {
  /** The items on this page. */
  items: T[];
  /** Index of the page's first item in the whole list. */
  start: number;
  page: number;
  pageCount: number;
  total: number;
  setPage: (page: number) => void;
}

/**
 * Splits a long list into pages of `size`. Going back to the first page when
 * `resetKey` changes (a new filter or sort) keeps the list from opening on a
 * page that no longer exists.
 */
export function usePage<T>(items: T[], size: number, resetKey: unknown = null): Page<T> {
  const [state, setState] = useState({ page: 0, key: resetKey });
  const pageCount = Math.max(1, Math.ceil(items.length / size));
  // A different list (or a shorter one) starts again at a page that exists.
  const page = state.key !== resetKey ? 0 : Math.min(state.page, pageCount - 1);
  const start = page * size;
  return {
    items: items.slice(start, start + size),
    start,
    page,
    pageCount,
    total: items.length,
    setPage: (p) => setState({ page: Math.max(0, Math.min(p, pageCount - 1)), key: resetKey }),
  };
}

/** Scrolls `ref` back to the top whenever the page changes. */
export function useScrollTopOnPage(ref: RefObject<HTMLElement | null>, page: number) {
  useEffect(() => {
    ref.current?.scrollTo?.({ top: 0 });
  }, [ref, page]);
}
