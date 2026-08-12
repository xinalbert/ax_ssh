#!/usr/bin/env python3
"""Validate that every Slint translation message has a non-empty catalog entry."""

from __future__ import annotations

import ast
import re
import sys
from pathlib import Path


TRANSLATION_CALL = re.compile(r'@tr\(\s*("(?:\\.|[^"\\])*")')
PO_FIELD = re.compile(r'^(msgid|msgstr)\s+("(?:\\.|[^"\\])*")\s*$')
PLACEHOLDER = re.compile(r"\{\d+\}")


def decode_quoted(value: str) -> str:
    decoded = ast.literal_eval(value)
    if not isinstance(decoded, str):
        raise ValueError(f"expected quoted string, got {value!r}")
    return decoded


def slint_messages(ui_root: Path) -> set[str]:
    messages: set[str] = set()
    for path in sorted(ui_root.rglob("*.slint")):
        source = path.read_text(encoding="utf-8")
        messages.update(decode_quoted(match.group(1)) for match in TRANSLATION_CALL.finditer(source))
    return messages


def po_messages(path: Path) -> dict[str, str]:
    entries: dict[str, str] = {}
    current_field: str | None = None
    current_id = ""
    current_value = ""

    def finish() -> None:
        nonlocal current_id, current_value
        if current_id:
            entries[current_id] = current_value
        current_id = ""
        current_value = ""

    for raw_line in path.read_text(encoding="utf-8").splitlines():
        match = PO_FIELD.match(raw_line)
        if match:
            field, quoted = match.groups()
            if field == "msgid":
                finish()
                current_id = decode_quoted(quoted)
                current_field = "msgid"
            else:
                current_value = decode_quoted(quoted)
                current_field = "msgstr"
            continue
        if raw_line.startswith('"') and current_field:
            value = decode_quoted(raw_line)
            if current_field == "msgid":
                current_id += value
            else:
                current_value += value
        elif not raw_line.strip():
            finish()
            current_field = None
    finish()
    return entries


def main() -> int:
    repo_root = Path(__file__).resolve().parents[1]
    source_messages = slint_messages(repo_root / "ui")
    if "" in source_messages:
        print("Empty @tr() message is not allowed", file=sys.stderr)
        return 1
    catalog = po_messages(repo_root / "translations/zh-CN/LC_MESSAGES/ax_ssh.po")
    missing = sorted(message for message in source_messages if not catalog.get(message, "").strip())
    stale = sorted(message for message in catalog if message not in source_messages)
    mismatched_placeholders = sorted(
        message
        for message in source_messages & catalog.keys()
        if set(PLACEHOLDER.findall(message))
        != set(PLACEHOLDER.findall(catalog[message]))
    )
    if missing:
        print("Missing Simplified Chinese translations:", file=sys.stderr)
        for message in missing:
            print(f"  {message}", file=sys.stderr)
    if stale:
        print("Stale Simplified Chinese translations:", file=sys.stderr)
        for message in stale:
            print(f"  {message}", file=sys.stderr)
    if mismatched_placeholders:
        print("Mismatched translation placeholders:", file=sys.stderr)
        for message in mismatched_placeholders:
            print(f"  {message}", file=sys.stderr)
    if missing or stale or mismatched_placeholders:
        return 1
    print(f"Validated {len(source_messages)} Simplified Chinese Slint translations")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
