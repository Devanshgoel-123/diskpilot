# Releasing DiskPilot

Releases are built on GitHub Actions from a tag, never from a local machine.

## Where work happens

Features are developed in a private repository, `Devanshgoel-123/diskpilot-dev`, and reach this public repository when they are released. In a local clone, `origin` is the public repository and `private` is the development one, with `remote.pushDefault` set to `private`, so a plain `git push` never publishes anything.

- Day to day: branch, commit and `git push` (to `private`). Open pull requests in `diskpilot-dev`.
- The Rust checks in CI and the Release workflow run only in the public repository. In the private one, run `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` and `cargo test` in `src-tauri` before merging.
- Contributions to the public repository: merge them there, then bring them into the private one with `git pull origin main` and `git push`.
- The full history becomes public with each release, so never commit anything to the private repository that should stay private, such as keys or customer data.

## Steps

1. Make sure `main` is green in CI.
2. Bump the version in `package.json`, `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`, and refresh `src-tauri/Cargo.lock` (`cargo check` in `src-tauri`).
3. Write the release notes in `docs/releases/vX.Y.Z.md`. Describe what changed for the person using the app, not the code.
4. Commit as `release: X.Y.Z` and tag `vX.Y.Z`. Push to `private` first, then publish: `git push origin main vX.Y.Z`. This is the step that makes the release's code public.
5. The Release workflow first checks that the tag matches the version in all four files and that the tag holds its release notes (`scripts/check_release_versions.py`); it stops before building otherwise. It then builds the universal app, checks that the update archive was signed with the key in `tauri.conf.json` (`scripts/check_update_key.py`), checks the app's signature and architectures, records a signed build provenance attestation, and creates a **draft** release with `DiskPilot.dmg`, its SHA-256 checksum and the notes. Only maintainers can see a draft.
6. Download the draft's DMG (`gh release download vX.Y.Z -R Devanshgoel-123/diskpilot`), check the checksum and run `gh attestation verify DiskPilot.dmg -R Devanshgoel-123/diskpilot`, then open it on a Mac to confirm it launches without a warning.
7. Publish the draft (`gh release edit vX.Y.Z --draft=false -R Devanshgoel-123/diskpilot`), then re-run the workflow's "Update the Homebrew tap" job so the cask points at the new version.

A published DMG is never replaced: the workflow fails if the release already has one. To rebuild a release, delete it on GitHub first, on purpose.

### Repository settings that protect releases

These are set on GitHub, not in the code:

- Settings > General > Releases: turn on **immutable releases**, so a published release's files cannot be changed.
- Settings > Rules > Rulesets: add a tag ruleset for `v*` that blocks updates and deletions.
- Settings > Actions > General: require actions to be pinned to a full-length commit SHA.

The landing page links to `releases/latest/download/DiskPilot.dmg`, so it picks up the new release without changes.

## Updates

DiskPilot updates itself from 0.6 on. The app reads `latest.json` from the latest published release, downloads `DiskPilot.app.tar.gz`, verifies it against the public key in `tauri.conf.json` and installs it. Drafts are invisible to the app, so nobody is offered an update before the release is published.

Create the key pair once, on your own Mac, and keep the private key safe: losing it means existing installs can no longer update.

```sh
pnpm tauri signer generate -w ~/.tauri/diskpilot-updater.key
```

Put the contents of `~/.tauri/diskpilot-updater.key.pub` in `plugins.updater.pubkey` in `src-tauri/tauri.conf.json`, and add two repository secrets: `TAURI_SIGNING_PRIVATE_KEY` (the contents of the private key file) and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. Without the secret, releases are built without an update archive; with it, the workflow refuses to build while the public key is still the placeholder.

## Signing and notarization

Without an Apple Developer ID the app is signed ad hoc, and macOS asks people to confirm in **System Settings > Privacy & Security** the first time they open it. To remove that step, join the Apple Developer Program, then add these repository secrets. The Release workflow uses them automatically when they are present.

| Secret                       | Value                                                                                                             |
| ---------------------------- | ----------------------------------------------------------------------------------------------------------------- |
| `APPLE_CERTIFICATE`          | Your **Developer ID Application** certificate exported as `.p12`, base64-encoded (`base64 -i cert.p12 \| pbcopy`) |
| `APPLE_CERTIFICATE_PASSWORD` | The password you set when exporting the `.p12`                                                                    |
| `APPLE_SIGNING_IDENTITY`     | The certificate name, for example `Developer ID Application: Your Name (TEAMID)`                                  |
| `APPLE_API_ISSUER`           | Issuer ID of an App Store Connect API key (Users and Access > Integrations)                                       |
| `APPLE_API_KEY`              | The key ID of that API key                                                                                        |
| `APPLE_API_KEY_P8`           | The contents of the downloaded `AuthKey_<id>.p8` file                                                             |

With these secrets present the build is signed, notarized and stapled, and the workflow verifies that Gatekeeper accepts it. Set all six or none: if only some are set, or one is empty, the release fails instead of quietly shipping an ad hoc build. Never commit any of these values.
