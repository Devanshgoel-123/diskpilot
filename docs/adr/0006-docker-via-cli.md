# ADR 0006: Docker through the CLI with predefined actions

**Status:** accepted

## Context

Docker Desktop, OrbStack and Colima all ship the `docker` CLI; their sockets and APIs differ. Docker storage contains volumes that often hold databases.

## Decision

- Use the Docker CLI, found at known absolute locations, run without a shell, with timeouts.
- Read everything from `docker system df -v` (one call) plus the summary for reclaimable totals.
- Actions are a closed enum: start, stop, remove stopped container, remove unused image, remove unused build cache. IDs are validated (hex / `sha256:`), there is no `--force`, and there is **no volume removal**.
- Every action has a preview with the exact command; execution re-reads Docker state and re-checks preconditions.

## Consequences

- Works with any CLI-compatible engine without linking a Docker SDK.
- Output parsing depends on Docker's human-readable sizes; parsers are unit-tested against recorded output.
