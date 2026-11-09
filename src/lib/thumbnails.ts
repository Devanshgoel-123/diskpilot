import { api, errorMessage } from "@/lib/api";

/**
 * Quick Look thumbnails as object URLs, by `path@size`; `null` when macOS
 * has no preview for that kind of file. Row thumbnails and large previews are
 * kept apart, each least recently used first, with a limit so browsing
 * thousands of photos does not hold every image in memory; evicted URLs are
 * released. Every operation is constant time: lookups happen while rendering
 * lists of 2,000 rows.
 */
const small = new Map<string, string | null>();
const large = new Map<string, string | null>();
/** Row thumbnails are small; the large preview is up to 1024 px. */
const LARGE_FROM = 128;
/** Enough for every row of the longest list (2,000 files per type). */
const MAX_SMALL = 2500;
const MAX_LARGE = 24;

type Job = { large: boolean; run: () => void };
/** Large previews in the order they were opened. */
const waitingLarge: Job[] = [];
/** Row thumbnails, newest first: the rows just scrolled to are on screen. */
const waitingSmall: Job[] = [];
let running = 0;
/** Quick Look work is shared with Finder; keep a few requests in flight. */
const MAX_PARALLEL = 4;

const keyOf = (path: string, size: number) => `${path}@${size}`;
const storeFor = (size: number) => (size >= LARGE_FROM ? large : small);

function next() {
  while (running < MAX_PARALLEL) {
    const job = waitingLarge.shift() ?? waitingSmall.pop();
    if (!job) return;
    running += 1;
    job.run();
  }
}

function remember(size: number, key: string, url: string | null) {
  const store = storeFor(size);
  store.delete(key);
  store.set(key, url);
  const limit = store === large ? MAX_LARGE : MAX_SMALL;
  while (store.size > limit) {
    const oldest = store.keys().next().value as string;
    const u = store.get(oldest);
    if (u) URL.revokeObjectURL(u);
    store.delete(oldest);
  }
}

/** Forget every thumbnail, for example after a new scan: files may have changed. */
export function clearThumbnails() {
  for (const store of [small, large]) {
    for (const url of store.values()) if (url) URL.revokeObjectURL(url);
    store.clear();
  }
  waitingSmall.length = 0;
}

/** A thumbnail already loaded: a URL, `null` for none, `undefined` if not yet loaded. */
export function cachedThumbnail(path: string, size: number): string | null | undefined {
  const store = storeFor(size);
  const key = keyOf(path, size);
  if (!store.has(key)) return undefined;
  const url = store.get(key) ?? null;
  // Mark it recently used: delete and set again moves it to the end.
  store.delete(key);
  store.set(key, url);
  return url;
}

/**
 * Loads a Quick Look thumbnail, queued so long lists stay responsive. A large
 * preview someone just opened goes ahead of row thumbnails, and the newest
 * row thumbnails go first, so scrolling quickly fills the rows in view
 * before the ones scrolled past. Only "macOS has
 * no preview" is remembered; other failures (a timeout, a file that moved)
 * are tried again next time.
 */
export function loadThumbnail(path: string, size: number): Promise<string | null> {
  const key = keyOf(path, size);
  const known = cachedThumbnail(path, size);
  if (known !== undefined) return Promise.resolve(known);
  return new Promise((resolve) => {
    const job: Job = {
      large: size >= LARGE_FROM,
      run: () => {
        api
          .fileThumbnail(path, size)
          .then((bytes) => {
            const url = URL.createObjectURL(new Blob([bytes], { type: "image/png" }));
            remember(size, key, url);
            resolve(url);
          })
          .catch((e: unknown) => {
            if (errorMessage(e).includes("could not make a preview")) remember(size, key, null);
            resolve(null);
          })
          .finally(() => {
            running -= 1;
            next();
          });
      },
    };
    (job.large ? waitingLarge : waitingSmall).push(job);
    next();
  });
}
