/**
 * Batched, cached lookup of the installed app a path belongs to. Components
 * ask for single paths; requests made in the same tick go to the engine in
 * one call, and every answer (including "no app") is cached.
 */
import { useSyncExternalStore } from "react";

import { api } from "./api";
import type { AppBadge } from "./types";

const cache = new Map<string, AppBadge | null>();
const pending = new Set<string>();
const listeners = new Set<() => void>();
let scheduled = false;

function flush() {
  scheduled = false;
  const paths = [...pending];
  pending.clear();
  if (!paths.length) return;
  api
    .getAppBadges(paths)
    .then((found) => {
      for (const p of paths) cache.set(p, found[p] ?? null);
    })
    .catch(() => {
      for (const p of paths) cache.set(p, null);
    })
    .finally(() => listeners.forEach((l) => l()));
}

function request(path: string) {
  if (cache.has(path) || pending.has(path)) return;
  pending.add(path);
  if (!scheduled) {
    scheduled = true;
    queueMicrotask(flush);
  }
}

function subscribe(listener: () => void) {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

/** The app that owns `path`, or null when none is installed or known. */
export function useAppBadge(path: string | null | undefined): AppBadge | null {
  if (path) request(path);
  return useSyncExternalStore(subscribe, () => (path ? (cache.get(path) ?? null) : null));
}
