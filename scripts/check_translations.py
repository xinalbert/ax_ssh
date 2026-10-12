#!/usr/bin/env python3
"""Validate every bundled locale and the generated settings-search translations."""

from __future__ import annotations

import re
import sys
from pathlib import Path

from translation_catalog import (
    MessageKey,
    catalog_metadata,
    catalog_paths,
    placeholder_signature,
    read_catalog,
    slint_keys,
    source_keys,
)


def slint_messages(ui_root: Path) -> set[str]:
    """Return context-free Slint strings for compatibility with older tooling."""
    return {key.message for key in slint_keys(ui_root)}


def po_messages(path: Path) -> dict[str, str]:
    """Return context-free singular translations for existing inspection tools."""
    return {
        key.message: entry.translations.get(0, "")
        for key, entry in read_catalog(path).items()
        if key.message and not key.context and not key.plural
    }


def validate_catalog(path: Path, required: set[MessageKey]) -> list[str]:
    """Return missing, obsolete, fuzzy, metadata, and format-argument errors."""
    entries = read_catalog(path)
    metadata = catalog_metadata(entries)
    errors: list[str] = []
    locale = path.parent.parent.name
    language = metadata.get("Language", "").strip().replace("_", "-")
    if language.casefold() != locale.casefold():
        errors.append(
            f"Language header {language!r} does not match directory {locale!r}"
        )
    plural_header = metadata.get("Plural-Forms", "")
    match = re.search(r"nplurals\s*=\s*(\d+)\s*;", plural_header)
    plural_count = int(match[1]) if match else 0
    if not 1 <= plural_count <= 6 or "plural=" not in plural_header.replace(" ", ""):
        errors.append("missing or invalid Plural-Forms header")
    actual = {key for key in entries if key.message}
    for key in sorted(required - actual):
        errors.append(f"missing entry: {key}")
    for key in sorted(actual - required):
        errors.append(f"stale entry: {key}")
    for key in sorted(actual & required):
        entry = entries[key]
        if entry.fuzzy:
            errors.append(f"fuzzy entry: {key}")
        expected_forms = set(range(plural_count)) if key.plural else {0}
        if set(entry.translations) != expected_forms:
            errors.append(f"invalid translation forms: {key}")
        for index, value in entry.translations.items():
            if not value.strip():
                errors.append(f"empty translation form {index}: {key}")
                continue
            source = key.plural if key.plural and index > 0 else key.message
            try:
                if placeholder_signature(source) != placeholder_signature(value):
                    errors.append(f"mismatched placeholders in form {index}: {key}")
            except ValueError as error:
                errors.append(f"invalid placeholders: {key}: {error}")
    return errors


def main() -> int:
    """Check all locales without changing files."""
    root = Path(__file__).resolve().parents[1]
    try:
        required = source_keys(root)
        paths = catalog_paths(root)
        if not paths:
            raise ValueError("no bundled translation catalogs found")
        if any(not key.message for key in required):
            raise ValueError("empty @tr() message is not allowed")
        failed = False
        for path in paths:
            errors = validate_catalog(path, required)
            if errors:
                failed = True
                print(f"{path.relative_to(root)}:", file=sys.stderr)
                for error in errors:
                    print(f"  {error}", file=sys.stderr)
            else:
                print(
                    f"Validated {len(required)} messages for {path.parent.parent.name}"
                )
        return int(failed)
    except (OSError, ValueError, SyntaxError) as error:
        print(f"Translation validation failed: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
