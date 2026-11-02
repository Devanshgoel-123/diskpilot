## What does this change?

<!-- One or two sentences. Link related issues with "Fixes #123". -->

## How was it tested?

- [ ] `pnpm lint && pnpm typecheck && pnpm test`
- [ ] `cd src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`
- [ ] Ran the app (`pnpm app:dev`) and checked the affected screens

## Safety checklist

- [ ] No new code path deletes, moves or modifies user files outside `safety::authorize_destructive` and the planned executor
- [ ] No shell strings are built from paths (use `Command::arg`)
- [ ] New classification rules include an explanation and a test
- [ ] No metric is shown in the UI that does not come from the scanner or the OS
- [ ] No secrets, personal data or machine-specific paths are committed
