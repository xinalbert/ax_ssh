"""Read gettext catalogs and static Slint/egui translation markers."""

from __future__ import annotations

import ast
import json
import re
from collections import Counter
from dataclasses import dataclass, field
from pathlib import Path
from string import Formatter

QUOTED = r'"(?:\\.|[^"\\])*"'
TRANSLATION_CALL = re.compile(
    rf"@tr\(\s*(?:({QUOTED})\s*=>\s*)?({QUOTED})(?:\s*\|\s*({QUOTED}))?"
)
PO_FIELD = re.compile(
    rf"^(msgctxt|msgid_plural|msgid|msgstr(?:\[\d+\])?)\s+({QUOTED})\s*$"
)
SOURCE_COMMENTS = re.compile(rf"({QUOTED})|//[^\n]*|/\*[\s\S]*?\*/")
RUST_TOKENS = re.compile(
    rf'r(?P<hashes>\#{{0,255}})"[\s\S]*?"(?P=hashes)'
    r"|'(?:\\(?:u\{[0-9a-fA-F_]+\}|x[0-9a-fA-F]{2}|.)|[^'\\\n])'"
    rf'|{QUOTED}|//[^\n]*|(?P<block>/\*)'
    r'|(?P<marker>\b(?:tr|mark)\s*!\s*\()'
)
RUST_MESSAGE = re.compile(
    rf"\s*(?:({QUOTED})\s*=>\s*)?({QUOTED})\s*(?=[,)])"
)
BLOCK_COMMENT = re.compile(r"/\*|\*/")


@dataclass(frozen=True, order=True)
class MessageKey:
    """Identify one source message, optionally disambiguated by a context."""

    context: str
    message: str
    plural: str = ""


@dataclass
class CatalogEntry:
    """Represent one gettext entry without losing translator annotations."""

    key: MessageKey
    translations: dict[int, str] = field(default_factory=dict)
    comments: list[str] = field(default_factory=list)

    @property
    def fuzzy(self) -> bool:
        """Whether gettext considers the translation unreviewed."""
        return any(
            "fuzzy" in {flag.strip() for flag in comment[2:].split(",")}
            for comment in self.comments
            if comment.startswith("#,")
        )


def decode_quoted(value: str) -> str:
    """Decode a quoted PO or Slint string literal."""
    decoded = ast.literal_eval(value)
    if not isinstance(decoded, str):
        raise ValueError(f"expected quoted string, got {value!r}")
    return decoded


def slint_keys(ui_root: Path) -> set[MessageKey]:
    """Extract static keys with the repository's disabled default contexts."""
    keys: set[MessageKey] = set()
    for path in sorted(ui_root.rglob("*.slint")):
        source = path.read_text(encoding="utf-8")
        # Keep strings intact while removing comments that may mention @tr.
        source = SOURCE_COMMENTS.sub(lambda match: match[1] or " ", source)
        for match in TRANSLATION_CALL.finditer(source):
            context, message, plural = match.groups()
            keys.add(
                MessageKey(
                    decode_quoted(context) if context else "",
                    decode_quoted(message),
                    decode_quoted(plural) if plural else "",
                )
            )
    return keys


def search_rows(repo_root: Path) -> list[tuple[str, str, str]]:
    """Read the fixed settings routes and their translatable search labels."""
    raw = json.loads(
        (repo_root / "translations/settings-search.json").read_text(encoding="utf-8")
    )
    if not isinstance(raw, list) or not raw:
        raise ValueError("settings search catalog must be a non-empty list")
    rows: list[tuple[str, str, str]] = []
    for row in raw:
        if (
            not isinstance(row, list)
            or len(row) != 3
            or any(not isinstance(value, str) or not value.strip() for value in row)
        ):
            raise ValueError(f"invalid settings search row: {row!r}")
        rows.append(tuple(row))
    if len(set(rows)) != len(rows):
        raise ValueError("duplicate settings search row")
    return rows


def egui_keys(source_root: Path) -> set[MessageKey]:
    """Extract literal tr!/mark! keys outside Rust strings and nested comments.

    Markers accept a source literal, optionally preceded by a context literal
    and ``=>``. Dynamic keys fail validation rather than silently losing text.
    Generated tables are excluded; they never define their own source keys.
    """
    keys: set[MessageKey] = set()
    for path in sorted(source_root.rglob("*.rs")):
        if path.relative_to(source_root).parts[:1] == ("i18n",):
            continue
        source = path.read_text(encoding="utf-8")
        position = 0
        while token := RUST_TOKENS.search(source, position):
            position = token.end()
            if token["block"]:
                depth = 1
                for delimiter in BLOCK_COMMENT.finditer(source, position):
                    depth += 1 if delimiter[0] == "/*" else -1
                    position = delimiter.end()
                    if depth == 0:
                        break
                if depth:
                    raise ValueError(f"{path}: unterminated Rust block comment")
            elif token["marker"]:
                message = RUST_MESSAGE.match(source, position)
                if message is None:
                    raise ValueError(f"{path}: translation marker needs string literals")
                context, text = message.groups()
                source_text = decode_quoted(text)
                if not source_text:
                    raise ValueError(f"{path}: empty translation source")
                keys.add(
                    MessageKey(
                        decode_quoted(context) if context else "", source_text
                    )
                )
    return keys


def source_keys(repo_root: Path) -> set[MessageKey]:
    """Keep both frontends and settings-search keys in the shared PO catalogs."""
    keys = slint_keys(repo_root / "ui")
    keys.update(egui_keys(repo_root / "src/egui_app"))
    keys.update(
        MessageKey("settings-search", value)
        for row in search_rows(repo_root)
        for value in row
    )
    return keys


def read_catalog(path: Path) -> dict[MessageKey, CatalogEntry]:
    """Parse singular/plural PO entries, contexts, comments, and multiline fields."""
    entries: dict[MessageKey, CatalogEntry] = {}
    values: dict[str, str] = {}
    comments: list[str] = []
    current_field: str | None = None

    def finish() -> None:
        nonlocal values, comments, current_field
        if "msgid" in values:
            key = MessageKey(
                values.get("msgctxt", ""),
                values["msgid"],
                values.get("msgid_plural", ""),
            )
            if key in entries:
                raise ValueError(f"{path}: duplicate PO key {key!r}")
            translations = {
                int(name[7:-1]) if name.startswith("msgstr[") else 0: value
                for name, value in values.items()
                if name.startswith("msgstr")
            }
            entries[key] = CatalogEntry(key, translations, comments)
        elif values:
            raise ValueError(f"{path}: PO entry has no msgid")
        values, comments, current_field = {}, [], None

    for number, raw_line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        line = raw_line.strip()
        if not line:
            finish()
        elif line.startswith("#~"):
            continue
        elif line.startswith("#"):
            if "msgid" in values:
                finish()
            comments.append(line)
        elif match := PO_FIELD.fullmatch(line):
            name, quoted = match.groups()
            if name in {"msgctxt", "msgid"} and "msgid" in values:
                finish()
            if name in values:
                raise ValueError(f"{path}:{number}: repeated PO field {name}")
            current_field = name
            values[name] = decode_quoted(quoted)
        elif line.startswith('"') and current_field:
            values[current_field] += decode_quoted(line)
        else:
            raise ValueError(f"{path}:{number}: unsupported PO line {line!r}")
    finish()
    return entries


def catalog_metadata(entries: dict[MessageKey, CatalogEntry]) -> dict[str, str]:
    """Return gettext header fields."""
    header = entries.get(MessageKey("", ""))
    if header is None:
        raise ValueError("PO catalog has no metadata header")
    return {
        name.strip(): value.strip()
        for line in header.translations.get(0, "").splitlines()
        if ":" in line
        for name, value in [line.split(":", 1)]
    }


def placeholder_signature(value: str) -> Counter[str]:
    """Compare format arguments while permitting positional reordering."""
    signature: Counter[str] = Counter()
    implicit_index = 0
    saw_implicit = False
    saw_explicit = False
    for _, name, format_spec, conversion in Formatter().parse(value):
        if name is None:
            continue
        if format_spec or conversion:
            raise ValueError(f"unsupported Slint placeholder in {value!r}")
        if not name:
            saw_implicit = True
            name = str(implicit_index)
            implicit_index += 1
        elif name.isdigit():
            saw_explicit = True
        if not name.isdigit() and name != "n":
            raise ValueError(f"invalid Slint argument {name!r}")
        signature[name] += 1
    if saw_implicit and saw_explicit:
        raise ValueError(f"mixed positional and non-positional arguments in {value!r}")
    return signature


def catalog_paths(repo_root: Path) -> list[Path]:
    """Find build-time application catalogs in deterministic locale order."""
    return sorted((repo_root / "translations").glob("*/LC_MESSAGES/ax_ssh.po"))


def render_catalog(entries: list[CatalogEntry]) -> str:
    """Write canonical UTF-8 PO content, retaining comments and metadata."""
    lines: list[str] = []
    for entry in entries:
        lines.extend(entry.comments)
        if entry.key.context:
            lines.append(f"msgctxt {json.dumps(entry.key.context, ensure_ascii=False)}")
        lines.append(f"msgid {json.dumps(entry.key.message, ensure_ascii=False)}")
        if entry.key.plural:
            lines.append(
                f"msgid_plural {json.dumps(entry.key.plural, ensure_ascii=False)}"
            )
        for index, value in sorted(entry.translations.items()):
            field_name = f"msgstr[{index}]" if entry.key.plural else "msgstr"
            if not entry.key.message:
                lines.append(f'{field_name} ""')
                lines.extend(
                    json.dumps(line, ensure_ascii=False)
                    for line in value.splitlines(keepends=True)
                )
            else:
                lines.append(f"{field_name} {json.dumps(value, ensure_ascii=False)}")
        lines.append("")
    return "\n".join(lines)
