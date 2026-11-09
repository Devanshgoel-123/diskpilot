#!/usr/bin/env python3
"""DiskPilot's public numbers from GitHub: DMG downloads per release, updates
installed from inside the app, stars and repository traffic. The apps send nothing; this reads GitHub only, using
your `gh` login (traffic needs push access to the repository).

    scripts/stats.py           print a summary
    scripts/stats.py --save    also append today's numbers to a history file
                               (GitHub keeps traffic for 14 days only)
"""
import datetime
import json
import os
import subprocess
import sys

REPO = os.environ.get("DISKPILOT_REPO", "Devanshgoel-123/diskpilot")
HISTORY = os.path.expanduser(os.environ.get("DISKPILOT_STATS_FILE", "~/.diskpilot/stats.jsonl"))


def count(release, name):
    return sum(a["download_count"] for a in release["assets"] if a["name"] == name)


def gh(path):
    out = subprocess.run(["gh", "api", path], capture_output=True, text=True)
    return json.loads(out.stdout) if out.returncode == 0 else None


releases = [
    {
        "tag": r["tag_name"],
        "published": (r.get("published_at") or "")[:10],
        "downloads": count(r, "DiskPilot.dmg"),
        # From 0.6.0: one archive download per Mac that installed the update,
        # and one latest.json download per daily update check.
        "updates": count(r, "DiskPilot.app.tar.gz"),
        "checks": count(r, "latest.json"),
    }
    for r in gh(f"repos/{REPO}/releases?per_page=100") or []
    if not r["draft"]
]
repo = gh(f"repos/{REPO}") or {}
views = gh(f"repos/{REPO}/traffic/views")
referrers = gh(f"repos/{REPO}/traffic/popular/referrers") or []

today = datetime.date.today().isoformat()
total = sum(r["downloads"] for r in releases)
updates = sum(r["updates"] for r in releases)
print(f"DiskPilot on {today}")
print(f"  Downloads: {total} in total (DMG, including Homebrew installs)")
for r in releases:
    extra = f", {r['updates']} updated in the app" if r["updates"] else ""
    print(f"    {r['tag']} ({r['published']}): {r['downloads']}{extra}")
if releases and releases[0]["checks"]:
    print(f"  Update checks for {releases[0]['tag']}: {releases[0]['checks']} (about one per running Mac a day)")
print(f"  Stars: {repo.get('stargazers_count', '?')}   Forks: {repo.get('forks_count', '?')}")
if views:
    print(f"  Repository, last 14 days: {views['uniques']} unique visitors, {views['count']} views")
else:
    print("  Repository traffic: needs push access")
if referrers:
    print("  Visitors from: " + ", ".join(f"{r['referrer']} {r['uniques']}" for r in referrers))

if "--save" in sys.argv[1:]:
    os.makedirs(os.path.dirname(HISTORY), exist_ok=True)
    row = {
        "date": today,
        "downloads": total,
        "by_release": {r["tag"]: r["downloads"] for r in releases},
        "updates": updates,
        "update_checks": releases[0]["checks"] if releases else None,
        "stars": repo.get("stargazers_count"),
        "forks": repo.get("forks_count"),
        "visitors_14d": views and views["uniques"],
        "views_14d": views and views["count"],
        "referrers": {r["referrer"]: r["uniques"] for r in referrers},
    }
    with open(HISTORY, "a") as f:
        f.write(json.dumps(row) + "\n")
    print(f"  Saved to {HISTORY}")
