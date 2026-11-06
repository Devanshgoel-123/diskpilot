"""Fail unless an update archive was signed with the key the app trusts.

A release signed with another key passes every other check, and then
every installed copy refuses the update.

Usage: python3 scripts/check_update_key.py src-tauri/tauri.conf.json DiskPilot.app.tar.gz.sig

Both files are base64-encoded minisign files; the second line of each is
base64 of: 2-byte algorithm, 8-byte key ID (shown little-endian, as minisign prints it), then the key or signature.
"""
import base64, json, sys

def key_id(b64_file_text: str) -> str:
    second = base64.b64decode(b64_file_text.strip()).decode().splitlines()[1]
    return base64.b64decode(second)[2:10][::-1].hex().upper()

conf, sig_path = sys.argv[1], sys.argv[2]
trusted = key_id(json.load(open(conf))["plugins"]["updater"]["pubkey"])
signed = key_id(open(sig_path).read())
if trusted != signed:
    print(f"::error::The update was signed with key {signed}, but the app trusts {trusted}. "
          "Check the TAURI_SIGNING_PRIVATE_KEY secret.")
    sys.exit(1)
print(f"Update signed with the trusted key {trusted}.")
