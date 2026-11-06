import { api } from "@/lib/api";

/** Object URLs by `path@size`; `null` when macOS had no preview. */
const cache = new Map<string, string | null>();
const waiting: Array<() => void> = [];
let running = 0;
/** Quick Look work is shared with Finder; keep a few requests in flight. */
const MAX_PARALLEL = 4;

const keyOf = (path: string, size: number) => `${path}@${size}`;

function next() {
  while (running < MAX_PARALLEL && waiting.length > 0) {
    running += 1;
    waiting.shift()?.();
  }
}

/** A thumbnail already loaded: a URL, `null` for none, `undefined` if not yet loaded. */
export function cachedThumbnail(path: string, size: number): string | null | undefined {
  return cache.get(keyOf(path, size));
}

/** Loads a Quick Look thumbnail once, queued so long lists stay responsive. */
export function loadThumbnail(path: string, size: number): Promise<string | null> {
  const key = keyOf(path, size);
  if (cache.has(key)) return Promise.resolve(cache.get(key) ?? null);
  return new Promise((resolve) => {
    waiting.push(() => {
      api
        .fileThumbnail(path, size)
        .then((bytes) => URL.createObjectURL(new Blob([bytes], { type: "image/png" })))
        .catch(() => null)
        .then((url) => {
          cache.set(key, url);
          resolve(url);
        })
        .finally(() => {
          running -= 1;
          next();
        });
    });
    next();
  });
}
