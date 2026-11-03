# Releasing DiskPilot

Releases are built on GitHub Actions from a tag, never from a local machine.

## Steps

1. Make sure `main` is green in CI.
2. Bump the version in `package.json`, `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`, and refresh `src-tauri/Cargo.lock` (`cargo check` in `src-tauri`).
3. Write the release notes in `docs/releases/vX.Y.Z.md`. Describe what changed for the person using the app, not the code.
4. Commit as `release: X.Y.Z`, tag `vX.Y.Z`, and push `main` and the tag.
5. The Release workflow builds the universal app, checks its signature and architectures, and publishes `DiskPilot.dmg` with its SHA-256 checksum and the notes.
6. Download the published DMG, check the checksum, and open it on a Mac to confirm it launches.

The landing page links to `releases/latest/download/DiskPilot.dmg`, so it picks up the new release without changes.

## Signing and notarization

Without an Apple Developer ID the app is signed ad hoc, and macOS asks people to confirm in **System Settings > Privacy & Security** the first time they open it. To remove that step, join the Apple Developer Program, then add these repository secrets. The Release workflow uses them automatically when they are present.

| Secret | Value |
| --- | --- |
| `APPLE_CERTIFICATE` | Your **Developer ID Application** certificate exported as `.p12`, base64-encoded (`base64 -i cert.p12 \| pbcopy`) |
| `APPLE_CERTIFICATE_PASSWORD` | The password you set when exporting the `.p12` |
| `APPLE_SIGNING_IDENTITY` | The certificate name, for example `Developer ID Application: Your Name (TEAMID)` |
| `APPLE_API_ISSUER` | Issuer ID of an App Store Connect API key (Users and Access > Integrations) |
| `APPLE_API_KEY` | The key ID of that API key |
| `APPLE_API_KEY_P8` | The contents of the downloaded `AuthKey_<id>.p8` file |

With the API key secrets present the build is notarized and stapled, and the workflow verifies that Gatekeeper accepts it. Never commit any of these values.
