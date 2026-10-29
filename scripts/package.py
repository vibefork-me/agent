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
    package.py wizard [--out <dir>]
        checks the texts of the Windows installer wizard in every language (and writes their .wxl)

`bin_dir` holds the release binaries `vibefork[-tray]-<os>-<arch>[.exe]`. File names carry no
version: links and the agent's updater use them as they are. The icon is drawn here, the same round
mark as the tray's (`crates/vf-tray/src/icon.rs`), so the repository holds no image files.

The Windows installer speaks the agent's interface languages. Its texts are the `msi-*` messages of
the agent's string catalog (`crates/vf-core/locales/*.ftl`) over the standard WiX texts of the
language. The package is built once per language; the English one is the installer, and the others
are stored in it as language transforms named by Windows language ID and listed in its summary.
Windows Installer applies the transform of the user's display language by itself, the wizard and
`/passive` alike; any other language gets English. `TRANSFORMS=:<language ID>` picks one by hand.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import stat
import struct
import subprocess
import sys
import tarfile
import tempfile
import uuid
import zipfile
import zlib
from pathlib import Path
from xml.sax.saxutils import escape

PACKAGE_DIR = Path(__file__).resolve().parent / "package"
LOCALES_DIR = PACKAGE_DIR.parent.parent / "crates" / "vf-core" / "locales"
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


# The wizard's languages, English (the installer itself) first; the same as
# `vf_core::catalog::Language::ALL`. Each: the WiX cultures of its standard texts (the first names
# the language), its Windows language ID and code page, and the other Windows languages that get it,
# as `Language::parse` picks a language from a system one: any Portuguese → pt-BR, Spanish of Spain
# → es-ES, any other Spanish → es-419, any German → de.
WIZARD_LANGUAGES = (
    ("en", ("en-US",), 1033, 1252, ()),
    ("ru", ("ru-RU",), 1049, 1251, (2073,)),
    ("pt-BR", ("pt-BR",), 1046, 1252, (2070,)),
    ("es-ES", ("es-ES",), 3082, 1252, (1034,)),
    ("es-419", ("es-419", "es-ES"), 22538, 1252,
     (2058, 4106, 5130, 6154, 7178, 8202, 9226, 10250, 11274, 12298, 13322, 14346, 15370, 16394,
      17418, 18442, 19466, 20490, 21514, 23562)),
    ("de", ("de-DE",), 1031, 1252, (2055, 3079, 4103, 5127)),
)
TITLE_FONT = "{\\WixUI_Font_Title}"
BIGGER_FONT = "{\\WixUI_Font_Bigger}"
# The standard dialogs of the wizard (WixUI_InstallDir), each titled "<product> Setup".
WIZARD_DIALOGS = (
    "BrowseDlg", "CancelDlg", "DiskCostDlg", "ErrorDlg", "ExitDialog", "FatalError", "FilesInUse",
    "InstallDirDlg", "InvalidDirDlg", "MaintenanceTypeDlg", "MaintenanceWelcomeDlg", "MsiRMFilesInUse",
    "OutOfDiskDlg", "OutOfRbDiskDlg", "PrepareDlg", "ProgressDlg", "ResumeDlg", "UserExit",
    "VerifyReadyDlg", "WaitForCostingDlg", "WelcomeDlg",
)
ACTIONS = ("install", "change", "repair", "remove", "update")
ACTIONS_ING = ("installing", "changing", "repairing", "removing", "updating")
# Catalog message → the localization strings it fills: `Vibefork*` ones of `vibefork.wxs`, the others
# of WixUI (with the font its text starts with). Buttons and the folder browser keep their standard
# translations; so do the rare messages of Windows Installer itself (errors, action names).
WIZARD_TEXTS: dict[str, tuple[str, ...]] = {
    "msi-setup": ("VibeforkSetup", *(f"{d}_Title" for d in WIZARD_DIALOGS)),
    "msi-newer-installed": ("VibeforkNewerInstalled",),
    "msi-feature-desktop": ("VibeforkFeatureDesktop",),
    "msi-feature-autostart": ("VibeforkFeatureAutostart",),
    "msi-start": ("VibeforkStart",),
    "msi-welcome-title": (BIGGER_FONT + "WelcomeDlgTitle", BIGGER_FONT + "PrepareDlgTitle",
                          BIGGER_FONT + "MaintenanceWelcomeDlgTitle"),
    "msi-welcome-text": ("WelcomeDlgDescription",),
    "msi-welcome-update-text": ("WelcomeUpdateDlgDescriptionUpdate",),
    "msi-maintenance-text": ("MaintenanceWelcomeDlgDescription",),
    "msi-prepare-text": ("PrepareDlgDescription",),
    "msi-folder-title": (TITLE_FONT + "InstallDirDlgTitle",),
    "msi-folder-text": ("InstallDirDlgDescription",),
    "msi-folder-label": ("InstallDirDlgFolderLabel",),
    "msi-folder-browse-title": (TITLE_FONT + "BrowseDlgTitle",),
    "msi-folder-browse-text": ("BrowseDlgDescription",),
    "msi-folder-invalid": ("InvalidDirDlgText",),
    "msi-shortcuts-title": ("VibeforkShortcutsTitle",),
    "msi-shortcuts-text": ("VibeforkShortcutsText",),
    "msi-shortcut-desktop": ("VibeforkShortcutDesktop",),
    "msi-shortcut-sign-in": ("VibeforkShortcutSignIn",),
    **{f"msi-ready-{a}-title": (TITLE_FONT + f"VerifyReadyDlg{a.title()}Title",) for a in ACTIONS},
    **{f"msi-ready-{a}-text": (f"VerifyReadyDlg{a.title()}Text",) for a in ACTIONS},
    **{f"msi-progress-{a}-title": (TITLE_FONT + f"ProgressDlgTitle{a.title()}",) for a in ACTIONS_ING},
    **{f"msi-progress-{a}-text": (f"ProgressDlgText{a.title()}",) for a in ACTIONS_ING},
    "msi-maintenance-type-title": (TITLE_FONT + "MaintenanceTypeDlgTitle",),
    "msi-maintenance-type-text": ("MaintenanceTypeDlgDescription",),
    "msi-maintenance-change-text": ("MaintenanceTypeDlgChangeText",),
    "msi-maintenance-change-disabled": ("MaintenanceTypeDlgChangeDisabledText",),
    "msi-maintenance-repair-text": ("MaintenanceTypeDlgRepairText",),
    "msi-maintenance-repair-disabled": ("MaintenanceTypeDlgRepairDisabledText",),
    "msi-maintenance-remove-text": ("MaintenanceTypeDlgRemoveText",),
    "msi-maintenance-remove-disabled": ("MaintenanceTypeDlgRemoveDisabledText",),
    "msi-resume-title": (BIGGER_FONT + "ResumeDlgTitle",),
    "msi-resume-text": ("ResumeDlgDescription",),
    "msi-exit-title": (BIGGER_FONT + "ExitDialogTitle",),
    "msi-exit-text": ("ExitDialogDescription", "UserExitDescription2", "FatalErrorDescription2"),
    "msi-user-exit-title": (BIGGER_FONT + "UserExitTitle",),
    "msi-user-exit-text": ("UserExitDescription1",),
    "msi-fatal-title": (BIGGER_FONT + "FatalErrorTitle",),
    "msi-fatal-text": ("FatalErrorDescription1",),
    "msi-cancel-question": ("CancelDlgText",),
    "msi-files-in-use-title": (TITLE_FONT + "FilesInUseTitle", TITLE_FONT + "MsiRMFilesInUseTitle"),
    "msi-files-in-use-description": ("FilesInUseDescription", "MsiRMFilesInUseDescription"),
    "msi-files-in-use-text": ("FilesInUseText",),
    "msi-files-in-use-close-text": ("MsiRMFilesInUseText",),
    "msi-files-in-use-close": ("MsiRMFilesInUseUseRM",),
    "msi-files-in-use-keep": ("MsiRMFilesInUseDontUseRM",),
    "msi-disk-title": (TITLE_FONT + "OutOfDiskDlgTitle", TITLE_FONT + "OutOfRbDiskDlgTitle"),
    "msi-disk-description": ("OutOfDiskDlgDescription", "OutOfRbDiskDlgDescription"),
    "msi-disk-cost-title": (TITLE_FONT + "DiskCostDlgTitle",),
    "msi-disk-cost-description": ("DiskCostDlgDescription",),
    "msi-disk-text": ("OutOfDiskDlgText", "OutOfRbDiskDlgText", "DiskCostDlgText"),
    "msi-disk-rollback": ("OutOfRbDiskDlgText2",),
    "msi-costing-text": ("WaitForCostingDlgText",),
}
WIZARD_MESSAGE = re.compile(r"^(msi-[a-z][a-z-]*) = (.+)$")


def wizard_texts(code: str, codepage: int) -> dict[str, str]:
    """The `msi-*` messages of a language: one line each, plain text the wizard can show."""
    path = LOCALES_DIR / f"{code}.ftl"
    lines = path.read_text(encoding="utf-8").split("\n")
    texts: dict[str, str] = {}
    for number, line in enumerate(lines):
        if not line.startswith("msi-"):
            continue
        where = f"{path.name}:{number + 1}"
        match = WIZARD_MESSAGE.match(line)
        following = lines[number + 1] if number + 1 < len(lines) else ""
        if not match or following.startswith((" ", "\t")):
            raise SystemExit(f"{where}: a wizard message must be one line")
        message_id, text = match.groups()
        if message_id not in WIZARD_TEXTS:
            raise SystemExit(f"{where}: {message_id} is not used by the wizard (WIZARD_TEXTS)")
        if re.search(r"[\[\]{}]", text):
            raise SystemExit(f"{where}: brackets and braces have a meaning in the wizard's texts")
        try:
            text.encode(f"cp{codepage}")
        except UnicodeEncodeError as e:
            raise SystemExit(f"{where}: {text[e.start:e.end]!r} is not in the wizard's code page {codepage}")
        texts[message_id] = text
    missing = sorted(set(WIZARD_TEXTS) - set(texts))
    if missing:
        raise SystemExit(f"{path.name}: missing wizard messages: {', '.join(missing)}")
    return texts


def wizard_wxl(code: str, culture: str, language: int, codepage: int) -> str:
    """The WiX localization of one wizard language."""
    lines = [
        '<?xml version="1.0" encoding="utf-8"?>',
        f"<!-- Generated by scripts/package.py from crates/vf-core/locales/{code}.ftl. -->",
        f'<WixLocalization Culture="{culture}" Codepage="{codepage}" '
        'xmlns="http://wixtoolset.org/schemas/v4/wxl">',
        f'  <String Id="VibeforkLanguage" Value="{language}" />',
    ]
    for message_id, text in wizard_texts(code, codepage).items():
        for target in WIZARD_TEXTS[message_id]:
            font, string_id = re.fullmatch(r"(\{\\[A-Za-z_]+\})?(\w+)", target).groups()
            value = escape((font or "") + text, {'"': "&quot;"})
            lines.append(f'  <String Id="{string_id}" Value="{value}" />')
    lines.append("</WixLocalization>")
    return "\n".join(lines) + "\n"


def wizard(out: Path | None) -> None:
    shared = json.loads((LOCALES_DIR / "shared" / "locales.json").read_text(encoding="utf-8"))
    ready = [locale["code"] for locale in shared["locales"] if locale["status"] == "ready"]
    if [code for code, *_ in WIZARD_LANGUAGES] != ready:
        raise SystemExit(f"the wizard's languages (WIZARD_LANGUAGES) are not the shared ready list {ready}")
    for code, cultures, language, codepage, _ in WIZARD_LANGUAGES:
        text = wizard_wxl(code, cultures[0], language, codepage)
        if out:
            out.mkdir(parents=True, exist_ok=True)
            (out / f"{code}.wxl").write_text(text, encoding="utf-8", newline="\n")
    print(f"wizard ok: {len(WIZARD_LANGUAGES)} languages, {len(WIZARD_TEXTS)} messages")


def store_transforms(msi: Path, transforms: dict[int, Path]) -> None:
    """Stores language transforms in a package as sub-storages named by Windows language ID and lists
    the languages in its summary (Template: `<platform>;<base>,<language>,…`)."""
    import ctypes
    from ctypes import byref, c_int, c_uint, c_ulong, c_void_p, c_wchar_p

    msidll = ctypes.windll.msi

    def check(status: int, what: str) -> None:
        if status:
            raise SystemExit(f"{msi.name}: {what}: Windows Installer error {status}")

    db, view, summary = c_ulong(), c_ulong(), c_ulong()
    # MSIDBOPEN_TRANSACT: changes are written by the commit.
    check(msidll.MsiOpenDatabaseW(c_wchar_p(str(msi)), c_void_p(1), byref(db)), "open")
    try:
        query = c_wchar_p("SELECT `Name`, `Data` FROM `_Storages`")
        check(msidll.MsiDatabaseOpenViewW(db, query, byref(view)), "storages")
        check(msidll.MsiViewExecute(view, c_ulong(0)), "storages")
        for language, path in transforms.items():
            record = c_ulong(msidll.MsiCreateRecord(c_uint(2)))
            check(msidll.MsiRecordSetStringW(record, c_uint(1), c_wchar_p(str(language))), "record")
            check(msidll.MsiRecordSetStreamW(record, c_uint(2), c_wchar_p(str(path))), "record")
            # MSIMODIFY_INSERT
            check(msidll.MsiViewModify(view, c_int(1), record), f"store transform {language}")
            msidll.MsiCloseHandle(record)
        msidll.MsiViewClose(view)
        msidll.MsiCloseHandle(view)
        check(msidll.MsiGetSummaryInformationW(db, None, c_uint(1), byref(summary)), "summary")
        kind, number, size = c_uint(), c_int(), c_uint(256)
        buffer = ctypes.create_unicode_buffer(size.value)
        check(msidll.MsiSummaryInfoGetPropertyW(summary, c_uint(7), byref(kind), byref(number), None,
                                                buffer, byref(size)), "summary template")
        platform, base = buffer.value.split(";")
        template = f"{platform};{','.join([base, *(str(language) for language in transforms)])}"
        # VT_LPSTR
        check(msidll.MsiSummaryInfoSetPropertyW(summary, c_uint(7), c_uint(30), c_int(0), None,
                                                c_wchar_p(template)), "summary template")
        check(msidll.MsiSummaryInfoPersist(summary), "summary")
        msidll.MsiCloseHandle(summary)
        check(msidll.MsiDatabaseCommit(db), "commit")
    finally:
        msidll.MsiCloseHandle(db)


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
        work = Path(tmp) / "work"
        work.mkdir()
        # One package per language from the same files: the cabinet is made once and reused. The
        # packages are one product: a transform changes texts and language only.
        product_code = "{" + str(uuid.uuid4()).upper() + "}"
        for code, cultures, language, codepage, _ in WIZARD_LANGUAGES:
            loc = work / f"{code}.wxl"
            loc.write_text(wizard_wxl(code, cultures[0], language, codepage), encoding="utf-8")
            run(
                wix, "build", PACKAGE_DIR / "vibefork.wxs",
                "-arch", WINDOWS_ARCH[arch],
                "-ext", "WixToolset.UI.wixext", "-ext", "WixToolset.Util.wixext",
                "-d", f"Version={msi_version(version)}", "-d", f"Stage={stage}",
                "-d", f"ProductCode={product_code}",
                *(arg for culture in cultures for arg in ("-culture", culture)), "-loc", loc,
                "-cc", work / "cabinets", "-pdbtype", "none",
                "-o", work / f"{code}.msi",
            )
        base = work / f"{WIZARD_LANGUAGES[0][0]}.msi"
        transforms: dict[int, Path] = {}
        for code, _, language, _, others in WIZARD_LANGUAGES[1:]:
            mst = work / f"{code}.mst"
            # The transform may change the code page (Russian: 1251); it carries that permission itself.
            run(wix, "msi", "transform", base, work / f"{code}.msi", "-t", "language", "-serr", "f", "-o", mst)
            transforms.update(dict.fromkeys((language, *others), mst))
        store_transforms(base, transforms)
        msi = out / f"vibefork-windows-{arch}.msi"
        shutil.copyfile(base, msi)
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
    wz = sub.add_parser("wizard")
    wz.add_argument("--out", type=Path, help="write the .wxl of each language here")
    a = parser.parse_args()
    if a.target == "windows":
        windows(a.version, a.arch, a.bin_dir, a.out_dir.resolve(), a.wix)
    elif a.target == "linux":
        linux(a.version, a.arch, a.bin_dir, a.out_dir.resolve(), a.appimagetool.resolve(), a.runtime.resolve())
    elif a.target == "macos":
        macos(a.version, a.bin_dir, a.out_dir.resolve())
    elif a.target == "wizard":
        wizard(a.out)
    else:
        icons(a.out_dir)
    return 0


if __name__ == "__main__":
    sys.exit(main())
