#!/usr/bin/env python3
"""Fail if a dependency could come from somewhere other than crates.io or vibefork-me on GitHub.

- No `[patch]` / `[replace]` in any manifest; git dependencies only from github.com/vibefork-me.
- No `paths`, `patch`, `source` or `registries` overrides in `.cargo/config*`.
- `Cargo.lock` is current (`cargo metadata --locked`), and every locked package is a workspace
  crate, a crates.io crate or a vibefork-me git crate.
"""
import json
import subprocess
import sys
import tomllib
from pathlib import Path

root = Path(__file__).resolve().parent.parent
CRATES_IO = "registry+https://github.com/rust-lang/crates.io-index"
OWN_GIT = "https://github.com/vibefork-me/"
CONFIG_OVERRIDES = ("paths", "patch", "source", "registries", "registry")
DEP_TABLES = ("dependencies", "dev-dependencies", "build-dependencies")

errors = []


def dep_tables(manifest):
    """Every dependency table of a manifest, including `[target.*]` and `[workspace]` ones."""
    for name in DEP_TABLES:
        yield name, manifest.get(name, {})
    yield "workspace.dependencies", manifest.get("workspace", {}).get("dependencies", {})
    for target, table in manifest.get("target", {}).items():
        for name in DEP_TABLES:
            yield f"target.{target}.{name}", table.get(name, {})


manifests = [root / "Cargo.toml", *sorted(root.glob("crates/*/Cargo.toml")), *sorted(root.glob("xtask/Cargo.toml"))]
for path in manifests:
    rel = path.relative_to(root).as_posix()
    manifest = tomllib.loads(path.read_text(encoding="utf-8"))
    for key in ("patch", "replace"):
        if key in manifest:
            errors.append(f"{rel}: [{key}] overrides dependency sources")
    for table_name, table in dep_tables(manifest):
        for dep, spec in table.items():
            if not isinstance(spec, dict):
                continue
            git = spec.get("git")
            if git is not None and not git.startswith(OWN_GIT):
                errors.append(f"{rel} [{table_name}] {dep}: git dependency outside vibefork-me: {git}")
            if "registry" in spec or "registry-index" in spec:
                errors.append(f"{rel} [{table_name}] {dep}: registry other than crates.io")

for path in sorted(root.glob(".cargo/config*")):
    rel = path.relative_to(root).as_posix()
    config = tomllib.loads(path.read_text(encoding="utf-8"))
    for key in CONFIG_OVERRIDES:
        if key in config:
            errors.append(f"{rel}: `{key}` overrides dependency sources")

meta = subprocess.run(
    ["cargo", "metadata", "--locked", "--format-version", "1"],
    cwd=root, capture_output=True, text=True,
)
if meta.returncode != 0:
    errors.append("Cargo.lock is missing or out of date (cargo metadata --locked):\n    " + meta.stderr.strip())
else:
    packages = json.loads(meta.stdout)["packages"]
    for pkg in packages:
        source = pkg["source"]
        if source is None or source == CRATES_IO or source.startswith("git+" + OWN_GIT):
            continue
        errors.append(f"{pkg['name']} {pkg['version']}: source {source}")

if errors:
    print("Dependency source violations:\n  " + "\n  ".join(errors))
    sys.exit(1)
print(f"sources ok: {len(manifests)} manifests, {len(packages)} packages")
