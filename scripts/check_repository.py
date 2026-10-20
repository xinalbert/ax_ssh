#!/usr/bin/env python3
"""Check owned Markdown links and the repository's fixed source boundaries.

This read-only check uses Python 3.10+ and the standard library. It excludes
upstream vendor/reference trees and does not contact web links or verify
Markdown heading fragments. It deliberately does not parse Rust semantics;
Cargo and Clippy remain authoritative for compilation and platform cfgs.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path
from urllib.parse import unquote, urlsplit


INLINE_LINK = re.compile(
    r"!?\[[^\n]*?\]\(\s*(<[^>\n]+>|(?:\\.|[^\s()\\]|\([^()\n]*\))+)"
    r"(?:\s+[\"'][^\n]*?[\"'])?\s*\)"
)
REFERENCE_LINK = re.compile(r"^ {0,3}\[[^\]\n]+\]:\s*(<[^>\n]+>|\S+)", re.M)
FENCE = re.compile(r"^ {0,3}(`{3,}|~{3,})")


def markdown_targets(source: str) -> list[str]:
    """Extract inline/image and reference targets outside code examples."""
    prose: list[str] = []
    fence = ""
    for line in source.splitlines():
        marker = FENCE.match(line)
        if marker:
            run = marker[1]
            if not fence:
                fence = run
            elif run[0] == fence[0] and len(run) >= len(fence):
                fence = ""
            continue
        if not fence:
            prose.append(re.sub(r"(`+).*?\1", "", line))
    text = "\n".join(prose)
    return [
        match[1].strip("<>")
        for pattern in (INLINE_LINK, REFERENCE_LINK)
        for match in pattern.finditer(text)
    ]


def owned_markdown(root: Path) -> list[Path]:
    """List documentation owned by AxSSH, excluding upstream checkouts."""
    files = set(root.glob("*.md"))
    for directory in ("docs", ".agents", "translations", "assets"):
        files.update((root / directory).rglob("*.md"))
    return sorted(files)


def validate_markdown(root: Path, paths: list[Path]) -> list[str]:
    """Report missing/escaping local targets and unpaired Chinese pages."""
    root = root.resolve()
    errors: list[str] = []
    for path in paths:
        path = path.resolve()
        label = path.relative_to(root).as_posix()
        if path.name.endswith(".zh.md"):
            english = path.with_name(path.name.removesuffix(".zh.md") + ".md")
            if not english.is_file():
                errors.append(f"{label}: missing English counterpart")
        for target in markdown_targets(path.read_text(encoding="utf-8")):
            url = urlsplit(target)
            if url.scheme or url.netloc or not url.path:
                continue
            local = unquote(url.path)
            resolved = (
                root / local.lstrip("/")
                if local.startswith("/")
                else path.parent / local
            ).resolve()
            if not resolved.is_relative_to(root):
                errors.append(f"{label}: local link escapes repository: {target}")
            elif resolved.is_relative_to(root / "third_package" / "axshell"):
                errors.append(f"{label}: reference checkout cannot be a link target: {target}")
            elif not resolved.exists():
                errors.append(f"{label}: missing local target: {target}")
    return errors


def validate_source_layout(root: Path) -> list[str]:
    """Check modern module filenames and the unique Slint build/type entry."""
    errors: list[str] = []
    include_sites: list[str] = []
    for path in sorted((root / "src").rglob("*.rs")):
        label = path.relative_to(root).as_posix()
        if path.name == "mod.rs":
            errors.append(f"{label}: use foo.rs and foo/ child modules")
        source = path.read_text(encoding="utf-8")
        include_sites.extend(
            label
            for _ in re.finditer(r"(?m)^\s*slint::include_modules!\s*\(", source)
        )
    if include_sites != ["src/app.rs"]:
        errors.append("generated Slint types must be included once in src/app.rs")
    build = (root / "build.rs").read_text(encoding="utf-8")
    entries = re.findall(
        r'slint_build::compile(?:_with_config)?\(\s*"([^"\n]+)"', build
    )
    if entries != ["ui/app.slint"]:
        errors.append("build.rs must compile only ui/app.slint")
    return errors


def main() -> int:
    """Validate this checkout without depending on the current directory."""
    root = Path(__file__).resolve().parents[1]
    try:
        paths = owned_markdown(root)
        errors = validate_markdown(root, paths) + validate_source_layout(root)
    except (OSError, ValueError) as error:
        print(f"Repository validation failed: {error}", file=sys.stderr)
        return 1
    for error in errors:
        print(error, file=sys.stderr)
    if errors:
        return 1
    print(f"Checked {len(paths)} Markdown files and Rust/Slint entry boundaries")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
