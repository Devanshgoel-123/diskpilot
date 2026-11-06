"""Fail unless every version DiskPilot ships with matches the release tag.

The update manifest (latest.json) takes its version from the tag, while the
app and the signature of its update archive take theirs from tauri.conf.json.
If they differ, the updater refuses every download, and a published release
cannot be changed. The tag's own commit must also hold its release notes,
because the app bundles "What's new" from there.

Usage: python3 scripts/check_release_versions.py v0.6.0
"""
import json
import re
import sys
from pathlib import Path

root = Path(__file__).resolve().parent.parent
tag = sys.argv[1]
want = tag.removeprefix("v")

found = {
    "package.json": json.loads((root / "package.json").read_text())["version"],
    "src-tauri/tauri.conf.json": json.loads((root / "src-tauri/tauri.conf.json").read_text())["version"],
    "src-tauri/Cargo.toml": re.search(
        r'^version = "([^"]+)"', (root / "src-tauri/Cargo.toml").read_text(), re.M
    ).group(1),
    "src-tauri/Cargo.lock": re.search(
        r'^name = "diskpilot"\nversion = "([^"]+)"', (root / "src-tauri/Cargo.lock").read_text(), re.M
    ).group(1),
}

ok = True
for path, version in found.items():
    if version != want:
        print(f"::error file={path}::{path} says {version}, but the tag is {tag}.")
        ok = False
notes = root / "docs/releases" / f"{tag}.md"
if not notes.is_file():
    print(f"::error::{notes.relative_to(root)} is missing from the tagged commit; the app bundles it as What's new.")
    ok = False
if not ok:
    sys.exit(1)
print(f"All versions are {want}, and the release notes are in the tag.")
