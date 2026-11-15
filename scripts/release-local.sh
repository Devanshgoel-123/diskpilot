#!/usr/bin/env bash
# Build, sign, notarize and (optionally) publish a macOS release from this Mac,
# without GitHub's macOS runners. Produces the same files as the Release
# workflow: DiskPilot.dmg, DiskPilot.app.tar.gz (+ .sig inside latest.json),
# their .sha256 files and latest.json.
#
#   scripts/release-local.sh v0.6.6              # build and check only
#   scripts/release-local.sh v0.6.6 --publish    # also upload a draft release
#   scripts/release-local.sh v0.6.6 --homebrew   # after publishing: bump the cask
#
# Needs: the Developer ID certificate in the login keychain, the notarytool
# keychain profile "diskpilot-notary", the updater key at
# ~/.tauri/diskpilot-updater.key (asks for its password, or reads
# TAURI_SIGNING_PRIVATE_KEY_PASSWORD), and `gh` signed in.
# A local build has no GitHub build attestation; the release notes say so.
set -euo pipefail

TAG="${1:?usage: scripts/release-local.sh vX.Y.Z [--publish|--homebrew]}"
MODE="${2:-}"
REPO="Devanshgoel-123/diskpilot"
IDENTITY="Developer ID Application: Devansh Goel (8HS6L7V3S9)"
PROFILE="diskpilot-notary"
KEY="${TAURI_UPDATER_KEY:-$HOME/.tauri/diskpilot-updater.key}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="$ROOT/dist-release/$TAG"
TARGET="${CARGO_TARGET_DIR:-$ROOT/src-tauri/target}"
BUNDLE="$TARGET/universal-apple-darwin/release/bundle"
APP="$BUNDLE/macos/DiskPilot.app"
VERSION="${TAG#v}"

say() { printf '\n== %s\n' "$*"; }
die() { printf 'error: %s\n' "$*" >&2; exit 1; }
retry() { for i in 1 2 3 4; do "$@" && return 0; echo "retrying in 15 s ($i)"; sleep 15; done; "$@"; }

cd "$ROOT"

if [ "$MODE" = "--homebrew" ]; then
  sha=$(cut -d' ' -f1 "$OUT/DiskPilot.dmg.sha256")
  [[ "$sha" =~ ^[0-9a-f]{64}$ ]] || die "no checksum in $OUT"
  [ "$(gh api "repos/$REPO/releases/tags/$TAG" --jq .draft 2>/dev/null)" = "false" ] ||
    die "$TAG is not published yet; publish the draft first"
  tap=$(mktemp -d)
  gh repo clone Devanshgoel-123/homebrew-tap "$tap" -- --depth 1 -q
  sed -i '' -E "s/^  version \".*\"$/  version \"${VERSION}\"/; s/^  sha256 \".*\"$/  sha256 \"${sha}\"/" "$tap/Casks/diskpilot.rb"
  git -C "$tap" diff --exit-code --quiet && { echo "Cask already at $VERSION"; exit 0; }
  git -C "$tap" diff
  git -C "$tap" commit -qam "diskpilot ${VERSION}"
  git -C "$tap" push -q
  echo "Homebrew cask now at $VERSION"
  exit 0
fi

say "Checks"
[ -z "$(git status --porcelain --untracked-files=no)" ] || die "uncommitted changes"
git rev-parse -q --verify "refs/tags/$TAG" >/dev/null || die "tag $TAG does not exist locally"
[ "$(git rev-parse HEAD)" = "$(git rev-parse "$TAG^{commit}")" ] || die "HEAD is not $TAG; check it out first"
python3 scripts/check_release_versions.py "$TAG"
security find-identity -v -p codesigning | grep -qF "$IDENTITY" || die "signing identity not in keychain"
[ -f "$KEY" ] || die "updater key not found at $KEY"
grep -q REPLACE_WITH_UPDATER_PUBLIC_KEY src-tauri/tauri.conf.json && die "updater public key not set"
notes="$ROOT/docs/releases/$TAG.md"
[ -f "$notes" ] || die "missing $notes"

say "Build (universal)"
pnpm install --frozen-lockfile
APPLE_SIGNING_IDENTITY="$IDENTITY" pnpm tauri build --target universal-apple-darwin --bundles app
lipo -archs "$APP/Contents/MacOS/diskpilot" | grep -q "x86_64 arm64" || die "not a universal binary"

say "Notarize the app"
work=$(mktemp -d)
ditto -c -k --keepParent "$APP" "$work/app.zip"
xcrun notarytool submit "$work/app.zip" --keychain-profile "$PROFILE" --wait | tee "$work/notary.txt"
grep -q "status: Accepted" "$work/notary.txt" || die "app notarization was not accepted"
xcrun stapler staple "$APP"

say "Update archive (from the stapled app) and its signature"
rm -f "$BUNDLE/macos/DiskPilot.app.tar.gz" "$BUNDLE/macos/DiskPilot.app.tar.gz.sig"
COPYFILE_DISABLE=1 tar -czf "$BUNDLE/macos/DiskPilot.app.tar.gz" -C "$BUNDLE/macos" DiskPilot.app
if [ -z "${TAURI_SIGNING_PRIVATE_KEY_PASSWORD:-}" ]; then
  read -rsp "Updater key password: " TAURI_SIGNING_PRIVATE_KEY_PASSWORD; echo
fi
pnpm tauri signer sign -f "$KEY" -p "$TAURI_SIGNING_PRIVATE_KEY_PASSWORD" "$BUNDLE/macos/DiskPilot.app.tar.gz" >/dev/null
test -s "$BUNDLE/macos/DiskPilot.app.tar.gz.sig"
python3 scripts/check_update_key.py src-tauri/tauri.conf.json "$BUNDLE/macos/DiskPilot.app.tar.gz.sig"

say "Disk image"
venv="${TMPDIR:-/tmp}/diskpilot-dmgbuild"
[ -x "$venv/bin/dmgbuild" ] || { python3 -m venv "$venv" && "$venv/bin/pip" install --quiet dmgbuild==1.6.5; }
rm -rf "$OUT" && mkdir -p "$OUT"
"$venv/bin/dmgbuild" -s scripts/dmg/settings.py -D app="$APP" \
  -D background="$ROOT/scripts/dmg/background.tiff" DiskPilot "$OUT/DiskPilot.dmg" >/dev/null
codesign --sign "$IDENTITY" --timestamp "$OUT/DiskPilot.dmg"
xcrun notarytool submit "$OUT/DiskPilot.dmg" --keychain-profile "$PROFILE" --wait | tee "$work/notary-dmg.txt"
grep -q "status: Accepted" "$work/notary-dmg.txt" || die "disk image notarization was not accepted"
xcrun stapler staple "$OUT/DiskPilot.dmg"

say "Verify"
codesign --verify --deep --strict --verbose=2 "$APP"
retry xcrun stapler validate "$APP"
retry spctl --assess --type execute --verbose=2 "$APP"
retry xcrun stapler validate "$OUT/DiskPilot.dmg"
retry spctl --assess --type open --context context:primary-signature --verbose=2 "$OUT/DiskPilot.dmg"
mkdir -p "$work/update-check"
tar -xzf "$BUNDLE/macos/DiskPilot.app.tar.gz" -C "$work/update-check"
codesign --verify --deep --strict "$work/update-check/DiskPilot.app"
retry xcrun stapler validate "$work/update-check/DiskPilot.app"
plist_version=$(/usr/libexec/PlistBuddy -c "Print CFBundleShortVersionString" "$APP/Contents/Info.plist")
[ "$plist_version" = "$VERSION" ] || die "app says $plist_version, tag says $VERSION"

say "Assets"
cp "$BUNDLE/macos/DiskPilot.app.tar.gz" "$OUT/"
python3 - "$TAG" "$REPO" "$BUNDLE/macos/DiskPilot.app.tar.gz.sig" "$notes" > "$OUT/latest.json" <<'PYEOF'
import datetime, json, sys
tag, repo, sig_path, notes_path = sys.argv[1:5]
url = f"https://github.com/{repo}/releases/download/{tag}/DiskPilot.app.tar.gz"
entry = {"signature": open(sig_path).read().strip(), "url": url}
print(json.dumps({
    "version": tag.lstrip("v"),
    "notes": open(notes_path).read(),
    "pub_date": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
    "platforms": {"darwin-aarch64": entry, "darwin-x86_64": entry},
}, indent=2))
PYEOF
(cd "$OUT" && shasum -a 256 DiskPilot.dmg > DiskPilot.dmg.sha256 && shasum -a 256 DiskPilot.app.tar.gz > DiskPilot.app.tar.gz.sha256)
{
  cat "$notes"
  echo
  echo "## Verify the download"
  echo
  echo "SHA-256 of \`DiskPilot.dmg\`:"
  echo
  echo '```'
  cut -d' ' -f1 "$OUT/DiskPilot.dmg.sha256"
  echo '```'
  echo
  echo "Check it with \`shasum -a 256 DiskPilot.dmg\`. The app and the disk image are signed with a Developer ID and notarized by Apple."
} > "$OUT/notes.md"
ls -la "$OUT"

if [ "$MODE" = "--publish" ]; then
  say "Publish a draft release"
  if gh release view "$TAG" -R "$REPO" >/dev/null 2>&1; then
    gh release view "$TAG" -R "$REPO" --json assets --jq '.assets[].name' | grep -qx DiskPilot.dmg &&
      die "$TAG already has DiskPilot.dmg; published files are never replaced"
    (cd "$OUT" && gh release upload "$TAG" -R "$REPO" DiskPilot.dmg DiskPilot.dmg.sha256 \
      DiskPilot.app.tar.gz DiskPilot.app.tar.gz.sha256 latest.json)
    gh release edit "$TAG" -R "$REPO" --notes-file "$OUT/notes.md"
  else
    (cd "$OUT" && gh release create "$TAG" -R "$REPO" --verify-tag --draft \
      --title "DiskPilot $VERSION" --notes-file notes.md \
      DiskPilot.dmg DiskPilot.dmg.sha256 DiskPilot.app.tar.gz DiskPilot.app.tar.gz.sha256 latest.json)
  fi
  echo "Draft created. Download DiskPilot.dmg from it, check it opens, then publish it on GitHub,"
  echo "then run: scripts/release-local.sh $TAG --homebrew"
fi
