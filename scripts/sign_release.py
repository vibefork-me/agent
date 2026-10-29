#!/usr/bin/env python3
"""Add the signed update manifest to a draft GitHub Release made by `.github/workflows/release.yml`.

    python3 scripts/sign_release.py <tag> <release-subkey-secret-file> [--repo OWNER/NAME] [--days N]

Downloads every file of the draft `<tag>` (`v<version>`) with `gh` into a temporary directory,
checks them against `SHA256SUMS.txt`, signs the directory with `vibefork-sign release` (the secret
key is read only by `vibefork-sign`, from the given path), checks the result with
`vibefork-sign verify` and the release key built into the agent, and uploads `manifest.json` and
`manifest.sig` to the draft. A published release is refused. The draft stays a draft.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SIGNED = ("manifest.json", "manifest.sig")


def gh(*args: str) -> str:
    return subprocess.run(["gh", *args], check=True, capture_output=True, text=True).stdout


def vibefork_sign(*args: str) -> None:
    cmd = ["cargo", "run", "--quiet", "--release", "--locked", "-p", "vf-sign", "--", *args]
    subprocess.run(cmd, check=True, cwd=ROOT)


def check_sums(directory: Path) -> int:
    count = 0
    for line in (directory / "SHA256SUMS.txt").read_text(encoding="utf-8").splitlines():
        if not line.strip():
            continue
        digest, name = line.split(maxsplit=1)
        name = name.lstrip("*")
        actual = hashlib.sha256((directory / name).read_bytes()).hexdigest()
        if actual != digest:
            raise SystemExit(f"sign_release: {name} does not match SHA256SUMS.txt")
        count += 1
    return count


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("tag", help="release tag, v<version>")
    parser.add_argument("key", type=Path, help="secret key file of the certified release subkey")
    parser.add_argument("--repo", default="vibefork-me/agent", help="GitHub repository (default: %(default)s)")
    parser.add_argument("--days", type=int, default=90, help="manifest validity in days (default: %(default)s)")
    args = parser.parse_args()
    if not args.tag.startswith("v") or len(args.tag) < 2:
        parser.error("the tag is v<version>")
    version = args.tag[1:]
    if not args.key.is_file():
        parser.error(f"no key file {args.key}")

    info = json.loads(gh("release", "view", args.tag, "--repo", args.repo, "--json", "isDraft,assets"))
    if not info["isDraft"]:
        raise SystemExit(f"sign_release: {args.tag} is already published")

    with tempfile.TemporaryDirectory(prefix="vibefork-release-") as tmp:
        directory = Path(tmp)
        gh("release", "download", args.tag, "--repo", args.repo, "--dir", tmp)
        for name in SIGNED:
            (directory / name).unlink(missing_ok=True)
        print(f"{check_sums(directory)} files match SHA256SUMS.txt")
        vibefork_sign("release", tmp, str(args.key.resolve()), version, "--days", str(args.days))
        vibefork_sign("verify", tmp)
        gh("release", "upload", args.tag, "--repo", args.repo, "--clobber",
           *(str(directory / name) for name in SIGNED))
    print(f"{args.tag}: manifest.json and manifest.sig uploaded to the draft")
    return 0


if __name__ == "__main__":
    sys.exit(main())
