#!/usr/bin/env python3
"""Copy the shared language list and glossary (vibefork-me/shared packages/i18n) into
crates/vf-core/locales/shared and record where they come from in SOURCE.json (commit and sha256 of
each file), which check_i18n.py holds the copy to.

Usage: python scripts/sync_i18n.py [path to shared/packages/i18n]   (or `cargo xtask sync-i18n [path]`)
The source must be a clean git checkout, so the copy matches a commit of the shared repository.
"""
import hashlib
import json
import subprocess
import sys
from pathlib import Path

FILES = ["locales.json", "glossary.json"]
root = Path(__file__).resolve().parent.parent
target = root / "crates" / "vf-core" / "locales" / "shared"

candidates = [root.parent / "shared" / "packages" / "i18n", root.parent.parent / "shared" / "packages" / "i18n"]
source = Path(sys.argv[1]) if len(sys.argv) > 1 else next((c for c in candidates if c.exists()), None)
if source is None or not source.exists():
    sys.exit("sync-i18n: no shared/packages/i18n next to the agent; pass its path")


def git(*args: str) -> str:
    return subprocess.run(["git", "-C", str(source), *args], check=True, capture_output=True, text=True).stdout.strip()


dirty = git("status", "--porcelain", "--", *FILES)
if dirty:
    sys.exit(f"sync-i18n: {source} has uncommitted changes:\n{dirty}")

files = {}
for name in FILES:
    data = (source / name).read_bytes()
    (target / name).write_bytes(data)
    files[name] = hashlib.sha256(data).hexdigest()
record = {"repository": "vibefork-me/shared", "commit": git("rev-parse", "HEAD"), "files": files}
(target / "SOURCE.json").write_text(json.dumps(record, indent=2) + "\n", encoding="utf-8", newline="\n")
print(f"synced from {source} at {record['commit']}")
