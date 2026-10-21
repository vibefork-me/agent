#!/usr/bin/env python3
"""Render platform bootstrap installers with hashes of the supplied release binaries."""

from __future__ import annotations

import argparse
import hashlib
import re
from pathlib import Path

CHANNEL = "https://code.vibefork.me/releases/stable"
TARGETS = (
    ("windows", "x86_64", ".exe"),
    ("windows", "aarch64", ".exe"),
    ("macos", "x86_64", ""),
    ("macos", "aarch64", ""),
    ("linux", "x86_64", ""),
    ("linux", "aarch64", ""),
)


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def render(template: str, replacements: dict[str, str]) -> str:
    for token, value in replacements.items():
        template = template.replace(token, value)
    if re.search(r"@@[A-Z0-9_]+@@", template):
        raise ValueError("template contains an unfilled placeholder")
    return template


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("version", help="release version embedded in the installer")
    parser.add_argument("release_dir", type=Path, help="directory containing release binaries")
    parser.add_argument("output_dir", type=Path, help="directory for generated install scripts")
    args = parser.parse_args()
    if not re.fullmatch(r"[0-9A-Za-z][0-9A-Za-z.+-]*", args.version):
        parser.error("version contains unsupported characters")

    scripts_dir = Path(__file__).resolve().parent
    hashes: dict[tuple[str, str, str], str] = {}
    for os_name, arch, extension in TARGETS:
        for binary in ("vibefork", "vibefork-tray"):
            asset = f"{binary}-{os_name}-{arch}{extension}"
            path = args.release_dir / asset
            if path.is_symlink():
                parser.error(f"release asset must not be a symlink: {asset}")
            hashes[(binary, os_name, arch)] = sha256(path) if path.is_file() else ""
    if not hashes[("vibefork", "windows", "x86_64")]:
        parser.error("release_dir must contain vibefork-windows-x86_64.exe")

    windows_entries = []
    for os_name, arch, extension in TARGETS[:2]:
        for binary in ("vibefork", "vibefork-tray"):
            digest = hashes[(binary, os_name, arch)]
            if digest:
                asset = f"{binary}-{os_name}-{arch}{extension}"
                windows_entries.append(f"    '{asset}' = '{digest}'")

    unix_cases = []
    for os_name, arch, _ in TARGETS[2:]:
        agent = hashes[("vibefork", os_name, arch)]
        tray = hashes[("vibefork-tray", os_name, arch)]
        unix_cases.append(
            f"  {os_name}-{arch}) agent_hash='{agent}'; tray_hash='{tray}' ;;"
        )

    replacements = {
        "@@CHANNEL@@": CHANNEL,
        "@@VERSION@@": args.version,
        "@@WINDOWS_HASH_ENTRIES@@": "\n".join(windows_entries),
        "@@UNIX_HASH_CASES@@": "\n".join(unix_cases),
    }
    args.output_dir.mkdir(parents=True, exist_ok=True)
    for template_name, output_name in (
        ("install.ps1.in", "install.ps1"),
        ("install.sh.in", "install.sh"),
    ):
        template = (scripts_dir / "templates" / template_name).read_text(encoding="utf-8")
        output = render(template, replacements)
        (args.output_dir / output_name).write_text(output, encoding="utf-8", newline="\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
