# ADR 0004: Measure allocated bytes; keep the scan index in memory

**Status:** accepted

## Context

Logical file size overstates sparse files (Docker's `Docker.raw` reports hundreds of GB but uses a few) and understates small files.

## Decision

- Sizes are `st_blocks × 512`. Files with `nlink > 1` are counted once per inode. Symlinks are not followed and other volumes are skipped.
- The scan produces a summary (sent to the UI) and a path → size index kept in memory for the Explorer. Only scan metadata is persisted (SQLite).
- Disk capacity comes from `statfs`; "available" excludes purgeable space, and the UI says so.

## Consequences

- Numbers match what freeing the space would actually return, closer than Finder's logical sizes.
- APFS clones are still counted per copy (no public API exposes sharing cheaply).
- Memory is roughly 200 MB for ~1M files; browsing after an app restart requires a rescan. Persisting the index is a possible later optimization.
