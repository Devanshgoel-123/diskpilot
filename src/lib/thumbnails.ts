import { api, errorMessage } from "@/lib/api";

/**
 * Quick Look thumbnails as object URLs, by `path@size`; `null` when macOS
 * has no preview for that kind of file. Kept least recently used first, with
 * a limit per size so browsing hundreds of photos does not hold every image
 * in memory; evicted URLs are released.
 */
const cache = new Map<string, string | null>();
/** Row thumbnails are small; the large preview is up to 1024 px. */
const LARGE_FROM = 128;
const MAX_SMALL = 400;
const MAX_LARGE = 24;

type Job = { large: boolean; run: () => void };
const waiting: Job[] = [];
let running = 0;
/** Quick Look work is shared with Finder; keep a few requests in flight. */
const MAX_PARALLEL = 4;

const keyOf = (path: string, size: number) => `${path}@${size}`;
const isLarge = (key: string) => Number(key.slice(key.lastIndexOf("@") + 1)) >= LARGE_FROM;

function next() {
  while (running < MAX_PARALLEL && waiting.length > 0) {
    running += 1;
    waiting.shift()?.run();
  }
}

function remember(key: string, url: string | null) {
  cache.delete(key);
  cache.set(key, url);
  const large = isLarge(key);
  const limit = large ? MAX_LARGE : MAX_SMALL;
  const same = [...cache.keys()].filter((k) => isLarge(k) === large);
  for (const old of same.slice(0, Math.max(0, same.length - limit))) {
    const u = cache.get(old);
    if (u) URL.revokeObjectURL(u);
    cache.delete(old);
  }
}

/** Forget every thumbnail, for example after a new scan: files may have changed. */
export function clearThumbnails() {
  for (const url of cache.values()) if (url) URL.revokeObjectURL(url);
  cache.clear();
}

/** A thumbnail already loaded: a URL, `null` for none, `undefined` if not yet loaded. */
export function cachedThumbnail(path: string, size: number): string | null | undefined {
  const key = keyOf(path, size);
  if (!cache.has(key)) return undefined;
  const url = cache.get(key) ?? null;
  remember(key, url);
  return url;
}

/**
 * Loads a Quick Look thumbnail, queued so long lists stay responsive. A large
 * preview someone just opened goes ahead of row thumbnails. Only "macOS has
 * no preview" is remembered; other failures (a timeout, a file that moved)
 * are tried again next time.
 */
export function loadThumbnail(path: string, size: number): Promise<string | null> {
  const key = keyOf(path, size);
  const known = cachedThumbnail(path, size);
  if (known !== undefined) return Promise.resolve(known);
  return new Promise((resolve) => {
    const job: Job = {
      large: isLarge(key),
      run: () => {
        api
          .fileThumbnail(path, size)
          .then((bytes) => {
            const url = URL.createObjectURL(new Blob([bytes], { type: "image/png" }));
            remember(key, url);
            resolve(url);
          })
          .catch((e: unknown) => {
            if (errorMessage(e).includes("could not make a preview")) remember(key, null);
            resolve(null);
          })
          .finally(() => {
            running -= 1;
            next();
          });
      },
    };
    if (job.large) waiting.unshift(job);
    else waiting.push(job);
    next();
  });
}
