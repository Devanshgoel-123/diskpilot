# Performance baseline

Measured on the development Mac (Apple silicon, 8 GB) on 12 Nov 2026, against
`main` at 4789d0c. Each change is measured again after its commit; see
[Changes](#changes).

## How to measure

| What                     | Command                                                          | Notes                                                                                                                                                                                                                              |
| ------------------------ | ---------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Home-folder scan         | `cargo run --release --example _bench_scan -- ~ 3`               | Read-only. Real disk, so it varies with what else runs.                                                                                                                                                                            |
| Scan of a generated tree | `cargo run --release --example _bench_fixture -- <dir> 200000 5` | Deterministic tree (175k files counted, 3.7k folders), median of 5 after a warm-up. Used by CI.                                                                                                                                    |
| Thumbnails               | `cargo run --release --example _preview_thumbs -- 512 <files>`   | Local probe (gitignored). 40 real photos, screenshots, PDFs and videos.                                                                                                                                                            |
| Saved scan load          | `cargo run --release --example _preview_snapshot`                | Local probe (gitignored). Loads `last-scan.bin` and serialises the summary the UI receives.                                                                                                                                        |
| UI                       | `pnpm perf:ui`                                                   | Production build of the app against synthetic data (`perf/fixture.ts`), headless Chrome, CPU slowed 4x. The baseline and the first rows below used the median of 5 runs; the CI gate uses the best of the runs, which is steadier. |

## CI gate

The `perf` job in `.github/workflows/ci.yml` runs on every pull request on a
`macos-15-intel` runner. It checks out the base branch next to the pull
request, copies this pull request's measuring tools into it, and measures both
on the same machine:

- the scan of the generated tree (`_bench_fixture`, 200,000 files, median of 5);
- the size of the built JavaScript, in total and at startup;
- the UI harness (`perf/ui.mjs`, 3 runs).

`perf/compare.mjs` fails the job when a metric is more than 10% worse than the
base branch (5% for JavaScript size) and also worse by more than a small
absolute amount (40 ms for first render, which varies by about 20% between
runs of the same commit; 15 ms for page switches and previews), or when it goes
over a limit in `docs/perf/budgets.json`. The
table goes to the job summary and, for pull requests from this repository, to
a comment on the pull request.

## Baseline

| Metric                                                       | Value                                            |
| ------------------------------------------------------------ | ------------------------------------------------ |
| Home scan, full (986k files, 166k folders)                   | 15.3 s / 16.6 s / 20.1 s (median 16.6 s)         |
| Home scan, first partial results                             | 5.8–6.2 s                                        |
| Generated tree scan (175k files)                             | 622 ms                                           |
| Saved scan load (`last-scan.bin`, 1.6 MB)                    | 84 ms                                            |
| Scan summary sent to the UI                                  | 1,249 KB JSON, 1.5 ms to serialise               |
| Large preview (512 pt), Quick Look cache warm                | 55 ms, 928 KB per image                          |
| Large preview, first pass                                    | 62–154 ms                                        |
| Row thumbnail (32 pt)                                        | 3.5 ms, 5.0 KB                                   |
| JS bundle                                                    | 653 KB (197 KB gzip), one chunk                  |
| UI: first render (navigation to Overview with results)       | 130 ms                                           |
| UI: page switch, slowest (Storage, 1,000 rows)               | 187 ms                                           |
| UI: page switch, others                                      | 4–43 ms (Duplicates 43, Cleanup 17, Settings 14) |
| UI: scrolling Storage and Files by type                      | 60 fps                                           |
| UI: preview open to image shown (15 ms simulated Quick Look) | 85 ms                                            |
| UI: long tasks (> 50 ms) per run                             | 6                                                |

The UI numbers come from Chrome with the CPU slowed 4x, not the app's WebKit
view, so compare them with each other rather than with the real app.

## Changes

| Change                                                                             | Metric                                      | Before         | After                                 |
| ---------------------------------------------------------------------------------- | ------------------------------------------- | -------------- | ------------------------------------- |
| Opaque thumbnails as JPEG (6809c6c)                                                | Large preview, warm                         | 55 ms, 928 KB  | 14 ms, 138 KB                         |
|                                                                                    | Large preview, first pass                   | 62 ms          | 26 ms                                 |
|                                                                                    | Row thumbnail                               | 3.5 ms, 5.0 KB | 3.1 ms, 2.2 KB                        |
| Storage table renders only visible rows                                            | Switch to Storage (1,000 rows)              | 187 ms         | 70 ms                                 |
|                                                                                    | Long tasks per run                          | 6              | 2                                     |
|                                                                                    | JS bundle                                   | 653 KB         | 667 KB (+2%, @tanstack/react-virtual) |
| Row thumbnail as a placeholder in the preview                                      | Preview open to first image                 | 66 ms          | 39 ms                                 |
|                                                                                    | Preview open to full image                  | 66 ms          | 66 ms (unchanged)                     |
| Rarely used pages load on first use (Docker, Developer Tools, Settings, Assistant) | JavaScript at startup                       | 667 KB         | 585 KB (−12%)                         |
|                                                                                    | First render (two sets of 7 runs)           | 157 / 149 ms   | 107 / 143 ms                          |
|                                                                                    | Switch to those pages (preloaded once idle) | 2–5 ms         | 2–4 ms                                |
| Previous scan loads in the background at startup | Main-thread time in setup (load + recommendations) | 138–191 ms | 0 ms |
| | App setup finished (real app, 3 launches) | 277–285 ms | 137–245 ms |
| | First scan summary in the UI (real app, Vite dev server) | 611–621 ms | 445–583 ms |
