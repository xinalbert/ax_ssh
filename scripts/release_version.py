#!/usr/bin/env python3
"""Resolve AxSSH's date-based release version across package formats."""

from __future__ import annotations

import argparse
import datetime as dt
import plistlib
import re
from dataclasses import dataclass
from pathlib import Path
from zoneinfo import ZoneInfo


DATE_PATTERN = re.compile(r"^(?P<year>\d{4})-(?P<month>\d{2})-(?P<day>\d{2})$")
TAG_PATTERN = re.compile(r"^(?P<date>\d{4}-\d{2}-\d{2})$")


@dataclass(frozen=True)
class ReleaseVersion:
    """One date represented in the formats accepted by each distribution tool."""

    public_version: str
    cargo_version: str
    macos_short_version: str
    macos_bundle_version: str
    tag: str


def release_version_from_date(raw_date: str) -> ReleaseVersion:
    """Validate an ISO date and derive package-manager-specific version strings."""
    match = DATE_PATTERN.fullmatch(raw_date)
    if not match:
        raise ValueError("expected release date in YYYY-MM-DD format")

    try:
        date = dt.date(
            int(match.group("year")),
            int(match.group("month")),
            int(match.group("day")),
        )
    except ValueError as error:
        raise ValueError(f"invalid release date {raw_date!r}: {error}") from error

    cargo_version = f"{date.year}.{date.month}.{date.day}"
    return ReleaseVersion(
        public_version=date.isoformat(),
        cargo_version=cargo_version,
        macos_short_version=cargo_version,
        macos_bundle_version=date.strftime("%Y%m%d"),
        tag=date.isoformat(),
    )


def release_version_from_tag(tag: str) -> ReleaseVersion:
    """Validate a Git release tag and map it to its date version."""
    match = TAG_PATTERN.fullmatch(tag)
    if not match:
        raise ValueError("expected release tag in YYYY-MM-DD format")
    return release_version_from_date(match.group("date"))


def today_in_timezone(timezone: str) -> ReleaseVersion:
    """Resolve today's date from an IANA timezone, failing clearly if unknown."""
    try:
        zone = ZoneInfo(timezone)
    except Exception as error:
        raise ValueError(f"unknown IANA timezone {timezone!r}") from error
    return release_version_from_date(dt.datetime.now(zone).date().isoformat())


def read_package_version(cargo_toml: Path) -> str:
    """Read the root package version without treating dependency versions as package data."""
    in_package = False
    for line in cargo_toml.read_text(encoding="utf-8").splitlines():
        stripped = line.strip()
        if stripped.startswith("["):
            in_package = stripped == "[package]"
            continue
        if in_package and stripped.startswith("version = "):
            match = re.fullmatch(r'version\s*=\s*"([^"]+)"', stripped)
            if match:
                return match.group(1)
    raise ValueError(f"cannot read [package] version from {cargo_toml}")


def replace_root_package_version(path: Path, package_name: str, version: str) -> bool:
    """Replace a root package version in Cargo.toml or Cargo.lock exactly once."""
    lines = path.read_text(encoding="utf-8").splitlines(keepends=True)
    in_package = path.name == "Cargo.toml"
    matching_lock_package = False

    for index, line in enumerate(lines):
        stripped = line.strip()
        if path.name == "Cargo.toml":
            if stripped.startswith("["):
                in_package = stripped == "[package]"
                continue
            if in_package and stripped.startswith("version = "):
                return replace_line(path, lines, index, version)
            continue

        if stripped == "[[package]]":
            matching_lock_package = False
            continue
        if stripped.startswith("name = "):
            matching_lock_package = stripped == f'name = "{package_name}"'
            continue
        if matching_lock_package and stripped.startswith("version = "):
            return replace_line(path, lines, index, version)

    raise ValueError(f"cannot find root package {package_name!r} version in {path}")


def replace_line(path: Path, lines: list[str], index: int, version: str) -> bool:
    """Write an already located Cargo version line only when it actually changes."""
    ending = "\n" if lines[index].endswith("\n") else ""
    replacement = f'version = "{version}"{ending}'
    if lines[index] == replacement:
        return False
    lines[index] = replacement
    path.write_text("".join(lines), encoding="utf-8")
    return True


def update_macos_plist(path: Path, version: ReleaseVersion) -> bool:
    """Synchronize macOS bundle version keys through plistlib rather than text matching."""
    with path.open("rb") as file:
        contents = plistlib.load(file)
    if not isinstance(contents, dict):
        raise ValueError(f"macOS plist {path} is not a dictionary")

    updated = dict(contents)
    updated["CFBundleShortVersionString"] = version.macos_short_version
    updated["CFBundleVersion"] = version.macos_bundle_version
    if updated == contents:
        return False
    with path.open("wb") as file:
        plistlib.dump(updated, file, sort_keys=False)
    return True


def verify_release_files(args: argparse.Namespace, version: ReleaseVersion) -> None:
    """Reject a tag whose checked-out release metadata does not agree with it."""
    cargo_toml = Path(args.cargo_toml)
    cargo_lock = Path(args.cargo_lock)
    plist = Path(args.macos_plist)
    if read_package_version(cargo_toml) != version.cargo_version:
        raise ValueError(f"{cargo_toml} does not contain version {version.cargo_version}")

    lock_text = cargo_lock.read_text(encoding="utf-8")
    package_block = re.compile(
        rf'\[\[package\]\]\nname = "{re.escape(args.package_name)}"\nversion = "{re.escape(version.cargo_version)}"',
    )
    if not package_block.search(lock_text):
        raise ValueError(f"{cargo_lock} does not contain {args.package_name} {version.cargo_version}")

    with plist.open("rb") as file:
        plist_contents = plistlib.load(file)
    if (
        plist_contents.get("CFBundleShortVersionString") != version.macos_short_version
        or plist_contents.get("CFBundleVersion") != version.macos_bundle_version
    ):
        raise ValueError(f"{plist} does not match release tag {version.tag}")


def print_environment(version: ReleaseVersion) -> None:
    """Emit shell-safe GitHub Actions environment assignments."""
    print(f"RELEASE_PUBLIC_VERSION={version.public_version}")
    print(f"RELEASE_CARGO_VERSION={version.cargo_version}")
    print(f"RELEASE_DEBIAN_VERSION={version.cargo_version}")
    print(f"RELEASE_MACOS_SHORT_VERSION={version.macos_short_version}")
    print(f"RELEASE_MACOS_BUNDLE_VERSION={version.macos_bundle_version}")
    print(f"RELEASE_TAG={version.tag}")


def version_from_args(args: argparse.Namespace) -> ReleaseVersion:
    """Resolve exactly one explicit date, tag, or timezone-derived date."""
    if args.date:
        return release_version_from_date(args.date)
    if args.tag:
        return release_version_from_tag(args.tag)
    return today_in_timezone(args.timezone)


def add_version_source_arguments(parser: argparse.ArgumentParser) -> None:
    source = parser.add_mutually_exclusive_group(required=True)
    source.add_argument("--date", help="release date in YYYY-MM-DD format")
    source.add_argument("--tag", help="release tag in YYYY-MM-DD format")
    source.add_argument("--timezone", help="derive the date from this IANA timezone")


def add_release_file_arguments(parser: argparse.ArgumentParser) -> None:
    parser.add_argument("--cargo-toml", default="Cargo.toml")
    parser.add_argument("--cargo-lock", default="Cargo.lock")
    parser.add_argument("--macos-plist", default="packaging/macos/Info.plist")
    parser.add_argument("--package-name", default="ax_ssh")


def build_parser() -> argparse.ArgumentParser:
    """Build the small command-line interface used by local packaging and Actions."""
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)

    for command_name, help_text in [
        ("env", "print resolved release fields as KEY=VALUE lines"),
        ("sync", "synchronize Cargo and macOS package metadata"),
        ("verify", "verify Cargo and macOS metadata for a release tag"),
    ]:
        command = commands.add_parser(command_name, help=help_text)
        add_version_source_arguments(command)
        if command_name in {"sync", "verify"}:
            add_release_file_arguments(command)
    return parser


def main() -> int:
    """Run one version-resolution command."""
    parser = build_parser()
    args = parser.parse_args()
    try:
        version = version_from_args(args)
        if args.command == "env":
            print_environment(version)
        elif args.command == "sync":
            replace_root_package_version(Path(args.cargo_toml), args.package_name, version.cargo_version)
            replace_root_package_version(Path(args.cargo_lock), args.package_name, version.cargo_version)
            update_macos_plist(Path(args.macos_plist), version)
            print_environment(version)
        else:
            verify_release_files(args, version)
            print_environment(version)
    except ValueError as error:
        parser.error(str(error))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
