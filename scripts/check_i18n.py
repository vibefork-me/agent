#!/usr/bin/env python3
"""Check the agent's languages and texts against the shared language list and glossary.

- crates/vf-core/locales/shared is an unchanged copy of the list and glossary shared by every
  Vibefork product: the sha256 of each file is the one in SOURCE.json (`cargo xtask sync-i18n`);
- every `ready` language of the list has its `<code>.ftl`; a `planned` one may have none yet; no
  `.ftl` belongs to an unlisted language (that `Language::ALL` matches the list is a test of vf-core);
- the glossary: where an English message (or attribute) matches a term, the message in each language
  of the term's `expect` contains one of its variants, except the messages in
  locales/glossary.except.json (which name glossary terms, languages and English messages); no
  message uses a forbidden spelling.
"""
import hashlib
import json
import re
import sys
from pathlib import Path

root = Path(__file__).resolve().parent.parent
locales_dir = root / "crates" / "vf-core" / "locales"
shared_dir = locales_dir / "shared"
errors: list[str] = []


def read_json(path: Path):
    return json.loads(path.read_text(encoding="utf-8"))


# Fluent: `id = value`, indented continuation lines, `.attribute = value`; `#` comments.
ENTRY = re.compile(r"^([A-Za-z][\w-]*)\s*=\s?(.*)$")
ATTRIBUTE = re.compile(r"^\s+\.([\w-]+)\s*=\s?(.*)$")


def messages(text: str) -> dict[str, str]:
    """Message and attribute (`id.attr`) values of a Fluent file, as written."""
    out: dict[str, list[str]] = {}
    current = None
    message = None
    for line in text.splitlines():
        if line.startswith("#"):
            current = None
            continue
        if m := ENTRY.match(line):
            message = current = m.group(1)
            out[current] = [m.group(2)]
        elif message and (m := ATTRIBUTE.match(line)):
            current = f"{message}.{m.group(1)}"
            out[current] = [m.group(2)]
        elif current and (line.startswith((" ", "\t")) or not line.strip()):
            out[current].append(line.strip())
        elif line.strip():
            current = message = None
    return {k: "\n".join(v).strip() for k, v in out.items()}


# `{ $name }` and other placeables without text are names, not words; select expressions keep the
# text of their variants.
PLACEABLE = re.compile(r"\{\s*[$-]?[\w-]+(\([^)]*\))?\s*\}")
SELECT = re.compile(r"\{\s*\$[\w-]+\s*->|\*?\[[\w-]+\]|\}")


def words(value: str) -> str:
    return " ".join(SELECT.sub(" ", PLACEABLE.sub(" ", value)).split())


# The shared copy.
record = read_json(shared_dir / "SOURCE.json")
for name, sha256 in record.get("files", {}).items():
    if hashlib.sha256((shared_dir / name).read_bytes()).hexdigest() != sha256:
        errors.append(f"locales/shared/{name}: changed by hand (sha256 differs from SOURCE.json); run cargo xtask sync-i18n")
shared = read_json(shared_dir / "locales.json")
glossary = read_json(shared_dir / "glossary.json")
codes = [l["code"] for l in shared["locales"]]
ready = {l["code"] for l in shared["locales"] if l["status"] == "ready"}
source_code = shared["source"]

# Language files.
catalogs: dict[str, dict[str, str]] = {}
for code in codes:
    path = locales_dir / f"{code}.ftl"
    if path.exists():
        catalogs[code] = messages(path.read_text(encoding="utf-8"))
    elif code in ready:
        errors.append(f"locales/{code}.ftl: missing, {code} is ready in the shared list")
for path in locales_dir.glob("*.ftl"):
    if path.stem not in codes:
        errors.append(f"locales/{path.name}: {path.stem} is not in the shared list")
english = catalogs.get(source_code, {})

# Exceptions.
exempt = read_json(locales_dir / "glossary.except.json").get("terms", {})
term_ids = {t["id"] for t in glossary["terms"]}
for term_id, entry in exempt.items():
    if term_id not in term_ids:
        errors.append(f"glossary.except.json: {term_id!r} is not a glossary term")
    for code, keys in entry.get("keys", {}).items():
        if code not in codes:
            errors.append(f"glossary.except.json: {term_id}: {code} is not a language")
        for key in keys:
            if key not in english:
                errors.append(f"glossary.except.json: {term_id}: {key} is not an English message")

# Forbidden spellings and terms.
for code, catalog in catalogs.items():
    for key, value in catalog.items():
        for spelling in glossary["forbidden"]:
            if spelling in value:
                errors.append(f"{code}.ftl: {key}: {spelling!r} (glossary forbids it)")
checked = 0
for term in glossary["terms"]:
    if "match" not in term:
        continue
    case = term.get("caseSensitive", False)
    pattern = re.compile(term["match"], 0 if case else re.IGNORECASE)
    for key, value in english.items():
        if not pattern.search(words(value)):
            continue
        checked += 1
        for code, variants in term["expect"].items():
            text = catalogs.get(code, {}).get(key)
            if text is None or key in exempt.get(term["id"], {}).get("keys", {}).get(code, []):
                continue
            haystack = text if case else text.lower()
            if not any((v if case else v.lower()) in haystack for v in variants):
                say = term["say"][code]
                errors.append(f"{code}.ftl: {key}: glossary term {term['id']!r} should be {say!r}: {words(text)}")

if errors:
    print("i18n check failed:\n  " + "\n  ".join(errors))
    sys.exit(1)
print(f"i18n ok: {len(codes)} languages ({len(ready)} ready), {len(english)} English messages, "
      f"{len(glossary['terms'])} glossary terms, {checked} term uses checked")
