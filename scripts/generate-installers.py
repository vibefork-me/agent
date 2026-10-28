#!/usr/bin/env python3
"""Render platform bootstrap installers with hashes of the supplied release binaries.

The installers' messages come from the agent's string catalog (`crates/vf-core/locales/*.ftl`,
messages `install-*`). `--check` renders both installers with placeholder hashes and checks them:
every language has every installer message with the same variables, every message the templates use
exists and every installer message is used, the Windows script is ASCII.
"""

from __future__ import annotations

import argparse
import hashlib
import re
import sys
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
# Interface languages, English (the reference) first; the same as `vf_core::catalog::Language::ALL`.
LANGUAGES = ("en", "ru", "pt-BR", "es-ES", "es-419", "de")
SCRIPTS_DIR = Path(__file__).resolve().parent
LOCALES_DIR = SCRIPTS_DIR.parent / "crates" / "vf-core" / "locales"

MESSAGE_LINE = re.compile(r"^(install-[a-z][a-z-]*) = (.+)$")
PLACEABLE = re.compile(r"\{ \$([a-z]+) \}")
# How the templates use a message: `Msg 'install-…'` (PowerShell), `say install-…` / `fail install-…` (sh).
USES = {
    "install.ps1.in": re.compile(r"\bMsg '(install-[a-z-]+)'"),
    "install.sh.in": re.compile(r"\b(?:say|fail) (install-[a-z-]+)"),
}


class CatalogError(ValueError):
    """The installer messages of the catalog cannot be rendered into the scripts."""


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


def installer_messages(lang: str, source: str) -> dict[str, str]:
    """The `install-*` messages of a Fluent source as plain text with `{name}` placeholders."""
    messages: dict[str, str] = {}
    lines = source.split("\n")
    for number, line in enumerate(lines):
        if not line.startswith("install-"):
            continue
        match = MESSAGE_LINE.match(line)
        following = lines[number + 1] if number + 1 < len(lines) else ""
        if not match or following.startswith((" ", "\t")):
            raise CatalogError(f"{lang}.ftl:{number + 1}: an installer message must be one line")
        message_id, value = match.groups()
        text = PLACEABLE.sub(r"{\1}", value)
        if re.search(r"[{}]", re.sub(r"\{[a-z]+\}", "", text)):
            raise CatalogError(f"{lang}.ftl:{number + 1}: only `{{ $name }}` variables are allowed")
        if message_id in messages:
            raise CatalogError(f"{lang}.ftl:{number + 1}: {message_id} is defined twice")
        messages[message_id] = text
    return messages


def variables(text: str) -> list[str]:
    """The `{name}` placeholders of a message, sorted."""
    return sorted(re.findall(r"\{([a-z]+)\}", text))


def load_messages() -> dict[str, dict[str, str]]:
    """Installer messages of every language, each with exactly the English ids and variables."""
    catalog = {
        lang: installer_messages(lang, (LOCALES_DIR / f"{lang}.ftl").read_text(encoding="utf-8"))
        for lang in LANGUAGES
    }
    english = catalog["en"]
    if not english:
        raise CatalogError("en.ftl has no installer messages")
    for lang, messages in catalog.items():
        if sorted(messages) != sorted(english):
            missing = sorted(set(english) - set(messages))
            extra = sorted(set(messages) - set(english))
            raise CatalogError(f"{lang}.ftl: installer messages differ from en: missing {missing}, extra {extra}")
        for message_id, text in messages.items():
            if variables(text) != variables(english[message_id]):
                raise CatalogError(f"{lang}.ftl: {message_id} has other variables than in en")
    return catalog


def powershell_literal(text: str) -> str:
    """A single-quoted PowerShell string that `[regex]::Unescape` turns back into `text`; ASCII only."""
    escaped = "".join(
        c if " " <= c <= "~" and c != "\\" else ("\\\\" if c == "\\" else f"\\u{ord(c):04x}")
        for c in text
    )
    return "'" + escaped.replace("'", "''") + "'"


def sh_literal(text: str) -> str:
    """A single-quoted POSIX shell string."""
    return "'" + text.replace("'", "'\\''") + "'"


def windows_messages(catalog: dict[str, dict[str, str]]) -> str:
    blocks = []
    for lang, messages in catalog.items():
        entries = "\n".join(
            f"        '{message_id}' = {powershell_literal(text)}" for message_id, text in messages.items()
        )
        blocks.append(f"    '{lang}' = @{{\n{entries}\n    }}")
    return "\n".join(blocks)


def unix_messages(catalog: dict[str, dict[str, str]]) -> str:
    return "\n".join(
        f"    '{lang}:{message_id}') t={sh_literal(text)} ;;"
        for lang, messages in catalog.items()
        for message_id, text in messages.items()
    )


def check_uses(catalog: dict[str, dict[str, str]], templates: dict[str, str]) -> None:
    """Every message a template uses exists, and every installer message is used by some template."""
    used: set[str] = set()
    for name, template in templates.items():
        ids = set(USES[name].findall(template))
        unknown = sorted(ids - set(catalog["en"]))
        if unknown:
            raise CatalogError(f"{name} uses messages missing in en.ftl: {unknown}")
        used |= ids
    unused = sorted(set(catalog["en"]) - used)
    if unused:
        raise CatalogError(f"installer messages no template uses: {unused}")


def render_all(version: str, hashes: dict[tuple[str, str, str], str]) -> dict[str, str]:
    """Both installers, rendered: output file name → text."""
    templates = {
        name: (SCRIPTS_DIR / "templates" / name).read_text(encoding="utf-8")
        for name in ("install.ps1.in", "install.sh.in")
    }
    catalog = load_messages()
    check_uses(catalog, templates)

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
        "@@VERSION@@": version,
        "@@WINDOWS_HASH_ENTRIES@@": "\n".join(windows_entries),
        "@@UNIX_HASH_CASES@@": "\n".join(unix_cases),
        "@@WINDOWS_MESSAGES@@": windows_messages(catalog),
        "@@UNIX_MESSAGES@@": unix_messages(catalog),
    }
    rendered = {
        "install.ps1": render(templates["install.ps1.in"], replacements),
        "install.sh": render(templates["install.sh.in"], replacements),
    }
    if not rendered["install.ps1"].isascii():
        raise CatalogError("install.ps1 must be ASCII")
    return rendered


def check() -> int:
    hashes = {(binary, os_name, arch): "0" * 64 for binary in ("vibefork", "vibefork-tray") for os_name, arch, _ in TARGETS}
    try:
        rendered = render_all("0.0.0-check", hashes)
    except (CatalogError, ValueError) as e:
        print(f"generate-installers: {e}", file=sys.stderr)
        return 1
    count = len(load_messages()["en"])
    print(f"generate-installers: {len(rendered)} installers, {count} messages in {len(LANGUAGES)} languages")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--check", action="store_true", help="check the templates and messages, write nothing")
    parser.add_argument("version", nargs="?", help="release version embedded in the installer")
    parser.add_argument("release_dir", nargs="?", type=Path, help="directory containing release binaries")
    parser.add_argument("output_dir", nargs="?", type=Path, help="directory for generated install scripts")
    args = parser.parse_args()
    if args.check:
        return check()
    if args.version is None or args.release_dir is None or args.output_dir is None:
        parser.error("version, release_dir and output_dir are required")
    if not re.fullmatch(r"[0-9A-Za-z][0-9A-Za-z.+-]*", args.version):
        parser.error("version contains unsupported characters")

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

    try:
        rendered = render_all(args.version, hashes)
    except CatalogError as e:
        parser.error(str(e))
    args.output_dir.mkdir(parents=True, exist_ok=True)
    for output_name, output in rendered.items():
        (args.output_dir / output_name).write_text(output, encoding="utf-8", newline="\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
