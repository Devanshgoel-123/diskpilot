# ADR 0005: Trash-first cleanup through single-use plans

**Status:** accepted

## Context

Phase 2 introduces deletion. Users must see exactly what happens, be able to recover from mistakes where possible, and the UI must not be able to point the executor at arbitrary paths.

## Decision

- The UI sends recommendation **ids**, never paths. The planner resolves paths from the engine's own recommendations and authorizes each one.
- A plan is a preview: exact paths, sizes, warnings, rejected items. It is stored in the engine, single-use, and expires after 10 minutes. Execution takes only the plan id.
- The default method is **Move to Trash** (recoverable until the Trash is emptied). **Delete permanently** is allowed only when every selected item's policy permits it (regenerable data such as caches and build output).
- Every path is re-authorized immediately before it is touched, so a folder swapped for a symlink after planning is skipped.
- Tools with their own cleanup commands (pnpm store, nvm, rustup, simctl) get explanations and a command for the user to run; DiskPilot does not run third-party cleanup commands.

## Consequences

- Moving to the Trash does not free space until the Trash is emptied; the UI says so every time.
- Results (including failures) are recorded per item in SQLite.
