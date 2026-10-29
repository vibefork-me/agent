#!/usr/bin/env python3
"""Package release binaries into the files people download and install.

    package.py windows <version> <arch> <bin_dir> <out_dir>
        vibefork-windows-<arch>.msi            per-user installer (WiX, `scripts/package/vibefork.wxs`)
        vibefork-windows-<arch>-portable.zip   folder `Vibefork` that keeps its data next to the binaries
    package.py linux <version> <arch> <bin_dir> <out_dir> --appimagetool <file> --runtime <file>
        vibefork-linux-<arch>.AppImage (+ .zsync) and vibefork-linux-<arch>.tar.gz
    package.py macos <version> <bin_dir> <out_dir>
        vibefork-macos-universal.dmg           `Vibefork.app` (both processors), a link to Applications
    package.py icons <out_dir>
        vibefork.png, vibefork.ico, vibefork.icns

`bin_dir` holds the release binaries `vibefork[-tray]-<os>-<arch>[.exe]`. File names carry no
version: links and the agent's updater use them as they are. The icon is drawn here, the same round
mark as the tray's (`crates/vf-tray/src/icon.rs`), so the repository holds no image files.
"""

from __future__ import annotations

import argparse
import os
import shutil
import stat
import struct
import subprocess
import sys
import tarfile
import tempfile
import zipfile
import zlib
from pathlib import Path

PACKAGE_DIR = Path(__file__).resolve().parent / "package"
VIOLET = (124, 58, 237)
# The marker next to the binaries that tells the agent how it was installed (`vf_platform::install`).
KIND_MARKER = "vibefork.install"
WINDOWS_ARCH = {"x86_64": "x64", "aarch64": "arm64"}
# Where an installed AppImage finds its update (AppImageUpdate, zsync): the latest GitHub release.
APPIMAGE_UPDATE = "gh-releases-zsync|vibefork-me|agent|latest|vibefork-linux-{arch}.AppImage.zsync"


def png(size: int) -> bytes:
    """The round violet mark, `size` × `size` RGBA, as PNG."""
    half = size / 2 - 0.5
    radius = size / 2 - size / 32
    rows = bytearray()
    for y in range(size):
        rows.append(0)
        for x in range(size):
            d = ((x - half) ** 2 + (y - half) ** 2) ** 0.5
            alpha = max(0.0, min(1.0, radius - d))
            rows.extend((*VIOLET, round(alpha * 255)))

    def chunk(kind: bytes, data: bytes) -> bytes:
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))

    header = struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0)
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", header) + chunk(b"IDAT", zlib.compress(bytes(rows), 9)) + chunk(b"IEND", b"")


def ico(sizes: tuple[int, ...] = (16, 24, 32, 48, 64, 256)) -> bytes:
    """A Windows icon of PNG images."""
    images = [png(s) for s in sizes]
    out = struct.pack("<HHH", 0, 1, len(images))
    offset = 6 + 16 * len(images)
    for size, image in zip(sizes, images):
        side = 0 if size >= 256 else size
        out += struct.pack("<BBBBHHII", side, side, 0, 0, 1, 32, len(image), offset)
        offset += len(image)
    return out + b"".join(images)


def icns() -> bytes:
    """A macOS icon of PNG images (types ic07, ic08, ic09: 128, 256, 512 pixels)."""
    body = b"".join(
        kind + struct.pack(">I", 8 + len(image)) + image
        for kind, image in ((b"ic07", png(128)), (b"ic08", png(256)), (b"ic09", png(512)))
    )
    return b"icns" + struct.pack(">I", 8 + len(body)) + body


def icons(out: Path) -> None:
    out.mkdir(parents=True, exist_ok=True)
    (out / "vibefork.png").write_bytes(png(256))
    (out / "vibefork.ico").write_bytes(ico())
    (out / "vibefork.icns").write_bytes(icns())


def msi_version(version: str) -> str:
    """The installer's ProductVersion: the numeric part (`0.1.0-rc.1` → `0.1.0`)."""
    numbers = version.split("-")[0].split("+")[0].split(".")
    if len(numbers) != 3 or not all(n.isdigit() for n in numbers):
        raise SystemExit(f"version {version!r} is not X.Y.Z[-pre]")
    major, minor, patch = (int(n) for n in numbers)
    if major > 255 or minor > 255 or patch > 65535:
        raise SystemExit(f"version {version!r} does not fit an installer version (255.255.65535)")
    return f"{major}.{minor}.{patch}"


def run(*args: str | Path, cwd: Path | None = None) -> None:
    print("+", " ".join(str(a) for a in args), flush=True)
    subprocess.run([str(a) for a in args], check=True, cwd=cwd)


def binaries(bin_dir: Path, os_name: str, arch: str, ext: str = "") -> dict[str, Path]:
    found = {}
    for name in ("vibefork", "vibefork-tray"):
        path = bin_dir / f"{name}-{os_name}-{arch}{ext}"
        if not path.is_file():
            raise SystemExit(f"missing {path}")
        found[name] = path
    return found


def windows(version: str, arch: str, bin_dir: Path, out: Path, wix: str) -> None:
    bins = binaries(bin_dir, "windows", arch, ".exe")
    out.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory() as tmp:
        stage = Path(tmp)
        for name, path in bins.items():
            shutil.copyfile(path, stage / f"{name}.exe")
        (stage / KIND_MARKER).write_text("msi\n", encoding="ascii", newline="\n")
        (stage / "vibefork.ico").write_bytes(ico())
        msi = out / f"vibefork-windows-{arch}.msi"
        run(
            wix, "build", PACKAGE_DIR / "vibefork.wxs",
            "-arch", WINDOWS_ARCH[arch],
            "-ext", "WixToolset.UI.wixext", "-ext", "WixToolset.Util.wixext",
            "-d", f"Version={msi_version(version)}", "-d", f"Stage={stage}",
            "-o", msi,
        )
        msi.with_suffix(".wixpdb").unlink(missing_ok=True)
    portable = out / f"vibefork-windows-{arch}-portable.zip"
    with zipfile.ZipFile(portable, "w", zipfile.ZIP_DEFLATED, compresslevel=9) as z:
        for name, path in bins.items():
            z.write(path, f"Vibefork/{name}.exe")
        z.writestr("Vibefork/" + KIND_MARKER, "portable\n")
    print(f"{msi}\n{portable}")


def executable(path: Path) -> None:
    path.chmod(path.stat().st_mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)


def tar_filter(info: tarfile.TarInfo) -> tarfile.TarInfo:
    info.uid = info.gid = 0
    info.uname = info.gname = ""
    info.mode = 0o755 if info.isdir() or info.name.endswith(("vibefork", "vibefork-tray")) else 0o644
    return info


def linux(version: str, arch: str, bin_dir: Path, out: Path, appimagetool: Path, runtime: Path) -> None:
    bins = binaries(bin_dir, "linux", arch)
    out.mkdir(parents=True, exist_ok=True)
    tarball = out / f"vibefork-linux-{arch}.tar.gz"
    with tarfile.open(tarball, "w:gz", compresslevel=9) as t:
        for name, path in bins.items():
            t.add(path, f"vibefork/{name}", filter=tar_filter)
    with tempfile.TemporaryDirectory() as tmp:
        appdir = Path(tmp) / "Vibefork.AppDir"
        (appdir / "usr" / "bin").mkdir(parents=True)
        for name, path in bins.items():
            shutil.copyfile(path, appdir / "usr" / "bin" / name)
            executable(appdir / "usr" / "bin" / name)
        for name in ("AppRun", "vibefork.desktop"):
            shutil.copyfile(PACKAGE_DIR / name, appdir / name)
        executable(appdir / "AppRun")
        (appdir / "vibefork.png").write_bytes(png(256))
        image = out / f"vibefork-linux-{arch}.AppImage"
        env = dict(os.environ, ARCH=arch, VERSION=version, APPIMAGE_EXTRACT_AND_RUN="1")
        args = [str(appimagetool), "--no-appstream", "--runtime-file", str(runtime),
                "-u", APPIMAGE_UPDATE.format(arch=arch), str(appdir), str(image)]
        print("+", " ".join(args), flush=True)
        # appimagetool writes the .zsync next to the working directory's copy of the image.
        subprocess.run(args, check=True, env=env, cwd=out)
        executable(image)
    zsync = out / f"vibefork-linux-{arch}.AppImage.zsync"
    if not zsync.is_file():
        raise SystemExit(f"appimagetool did not write {zsync}")
    print(f"{tarball}\n{image}\n{zsync}")


def info_plist(version: str) -> str:
    """The app bundle's Info.plist; the same identity and URL scheme as `vibefork install` writes."""
    return f"""<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>me.vibefork.agent</string>
<key>CFBundleName</key><string>Vibefork</string>
<key>CFBundleDisplayName</key><string>Vibefork</string>
<key>CFBundleExecutable</key><string>vibefork-tray</string>
<key>CFBundleIconFile</key><string>vibefork</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>{msi_version(version)}</string>
<key>CFBundleVersion</key><string>{version}</string>
<key>LSUIElement</key><true/>
<key>LSMinimumSystemVersion</key><string>10.15</string>
<key>CFBundleURLTypes</key><array><dict><key>CFBundleURLName</key><string>vibefork</string><key>CFBundleURLSchemes</key><array><string>vibefork</string></array></dict></array>
</dict></plist>
"""


def macos(version: str, bin_dir: Path, out: Path) -> None:
    arm, intel = binaries(bin_dir, "macos", "aarch64"), binaries(bin_dir, "macos", "x86_64")
    out.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory() as tmp:
        stage = Path(tmp) / "Vibefork"
        app = stage / "Vibefork.app"
        (app / "Contents" / "MacOS").mkdir(parents=True)
        (app / "Contents" / "Resources").mkdir()
        for name in arm:
            target = app / "Contents" / "MacOS" / name
            run("lipo", "-create", "-output", target, arm[name], intel[name])
            executable(target)
        (app / "Contents" / "Info.plist").write_text(info_plist(version), encoding="utf-8")
        (app / "Contents" / "Resources" / "vibefork.icns").write_bytes(icns())
        # An ad-hoc signature: the bundle is consistent; it is not a Developer ID signature.
        run("codesign", "--force", "--deep", "--sign", "-", app)
        (stage / "Applications").symlink_to("/Applications")
        shutil.copyfile(PACKAGE_DIR / "macos-first-start.txt", stage / "If macOS blocks Vibefork.txt")
        dmg = out / "vibefork-macos-universal.dmg"
        dmg.unlink(missing_ok=True)
        run("hdiutil", "create", "-volname", "Vibefork", "-srcfolder", stage, "-format", "UDZO", "-ov", dmg)
    print(dmg)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="target", required=True)
    w = sub.add_parser("windows")
    w.add_argument("version")
    w.add_argument("arch", choices=sorted(WINDOWS_ARCH))
    w.add_argument("bin_dir", type=Path)
    w.add_argument("out_dir", type=Path)
    w.add_argument("--wix", default="wix", help="the `wix` command (WiX Toolset 5)")
    lx = sub.add_parser("linux")
    lx.add_argument("version")
    lx.add_argument("arch", choices=("x86_64", "aarch64"))
    lx.add_argument("bin_dir", type=Path)
    lx.add_argument("out_dir", type=Path)
    lx.add_argument("--appimagetool", type=Path, required=True)
    lx.add_argument("--runtime", type=Path, required=True, help="AppImage type 2 runtime of this processor")
    m = sub.add_parser("macos")
    m.add_argument("version")
    m.add_argument("bin_dir", type=Path)
    m.add_argument("out_dir", type=Path)
    i = sub.add_parser("icons")
    i.add_argument("out_dir", type=Path)
    a = parser.parse_args()
    if a.target == "windows":
        windows(a.version, a.arch, a.bin_dir, a.out_dir.resolve(), a.wix)
    elif a.target == "linux":
        linux(a.version, a.arch, a.bin_dir, a.out_dir.resolve(), a.appimagetool.resolve(), a.runtime.resolve())
    elif a.target == "macos":
        macos(a.version, a.bin_dir, a.out_dir.resolve())
    else:
        icons(a.out_dir)
    return 0


if __name__ == "__main__":
    sys.exit(main())
