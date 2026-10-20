#!/usr/bin/env python3
"""Fail if a crate depends on a crate of the same or a higher layer (see layers.toml, ARCHITECTURE.md)."""
import json
import subprocess
import sys
import tomllib
from pathlib import Path

root = Path(__file__).resolve().parent.parent
layers = tomllib.loads((root / "layers.toml").read_text(encoding="utf-8"))["layers"]
meta = json.loads(subprocess.run(
    ["cargo", "metadata", "--format-version", "1", "--no-deps"],
    cwd=root, check=True, capture_output=True, text=True,
).stdout)

errors = []
for pkg in meta["packages"]:
    name = pkg["name"]
    if name not in layers:
        errors.append(f"{name}: not listed in layers.toml")
        continue
    for dep in pkg["dependencies"]:
        if dep["name"] in layers and layers[dep["name"]] >= layers[name]:
            errors.append(f"{name} (layer {layers[name]}) -> {dep['name']} (layer {layers[dep['name']]})")

if errors:
    print("Layer violations:\n  " + "\n  ".join(errors))
    sys.exit(1)
print(f"layers ok: {len(meta['packages'])} crates")
