#!/usr/bin/env python3
"""Generate the curated Markdown prefix for an AxSSH GitHub Release."""

from __future__ import annotations

import argparse
import datetime as dt
import re
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Sequence


DATE_TAG_PATTERN = re.compile(r"^\d{4}-\d{2}-\d{2}$")
MAX_COMMITS_PER_CATEGORY = 8
TRACKING_PATTERN = re.compile(
    r"docs\(tracking\)|implementation tracking|tracking records|tracking docs|"
    r"project[- ]implementation|env audit|record .* tracking",
    re.IGNORECASE,
)


@dataclass(frozen=True)
class HighlightCategory:
    """A release-note section and the commit-subject pattern assigned to it."""

    title: str
    pattern: re.Pattern[str]


@dataclass(frozen=True)
class Commit:
    """A commit row used to build a release highlight."""

    short_sha: str
    sha: str
    subject: str


CATEGORIES = (
    HighlightCategory(
        "Workspace and UI",
        re.compile(
            r"(?<![A-Za-z0-9_])(?:workspace|window|titlebar|pane|input|focus|"
            r"menu|icon|asset|tooltip|locali[sz]ation|language|translation)"
            r"(?![A-Za-z0-9_])",
            re.IGNORECASE,
        ),
    ),
    HighlightCategory(
        "Terminal",
        re.compile(
            r"(?<![A-Za-z0-9_])(?:terminal|ime|selection|scrollback|split(?:-screen)?|"
            r"ansi|truecolor|terminal url|url highlighting)(?![A-Za-z0-9_])",
            re.IGNORECASE,
        ),
    ),
    HighlightCategory(
        "SFTP",
        re.compile(
            r"(?<![A-Za-z0-9_])(?:sftp|file transfer|remote file|browser)"
            r"(?![A-Za-z0-9_])",
            re.IGNORECASE,
        ),
    ),
    HighlightCategory(
        "SSH and sessions",
        re.compile(
            r"(?<![A-Za-z0-9_])(?:ssh|telnet|serial|agent|authentication|host key|"
            r"credential|session|x11|xquartz)(?![A-Za-z0-9_])",
            re.IGNORECASE,
        ),
    ),
    HighlightCategory(
        "Settings and themes",
        re.compile(
            r"(?<![A-Za-z0-9_])(?:settings|theme|font|appearance|color|contrast)"
            r"(?![A-Za-z0-9_])",
            re.IGNORECASE,
        ),
    ),
    HighlightCategory(
        "Packaging and release",
        re.compile(
            r"(?<![A-Za-z0-9_])(?:release|workflow|artifact|package|packaging|"
            r"version|bundle|license)(?![A-Za-z0-9_])",
            re.IGNORECASE,
        ),
    ),
)


class ReleaseHighlightsError(RuntimeError):
    """Raised when the checked-out release history cannot be summarized."""


class GitCommandError(ReleaseHighlightsError):
    """Record a failed Git command so expected no-tag cases stay narrow."""

    def __init__(self, detail: str, returncode: int) -> None:
        super().__init__(detail)
        self.returncode = returncode


def validate_date_tag(tag: str) -> None:
    """Require the public release tag format used by AxSSH."""

    if not DATE_TAG_PATTERN.fullmatch(tag):
        raise ReleaseHighlightsError(f"release tag must use YYYY-MM-DD: {tag!r}")
    try:
        dt.date.fromisoformat(tag)
    except ValueError as error:
        raise ReleaseHighlightsError(f"release tag has an invalid date: {tag!r}") from error


def git_output(repository: Path, *args: str) -> str:
    """Run a Git command in *repository* and return its trimmed stdout."""

    try:
        result = subprocess.run(
            ["git", *args],
            cwd=repository,
            check=True,
            capture_output=True,
            text=True,
        )
    except FileNotFoundError as error:
        raise ReleaseHighlightsError("git is required to generate release highlights") from error
    except subprocess.CalledProcessError as error:
        detail = error.stderr.strip() or error.stdout.strip() or "git command failed"
        raise GitCommandError(detail, error.returncode) from error
    return result.stdout.rstrip("\n")


def previous_release_tag(repository: Path, current_commit: str) -> str | None:
    """Find the nearest earlier reachable AxSSH date tag, if one exists."""

    commit_and_parents = git_output(
        repository,
        "rev-list",
        "--parents",
        "-n",
        "1",
        current_commit,
    ).split()
    if len(commit_and_parents) < 2:
        return None

    try:
        candidate = git_output(
            repository,
            "describe",
            "--tags",
            "--abbrev=0",
            "--match",
            "????-??-??",
            f"{current_commit}^",
        )
    except GitCommandError as error:
        if error.returncode == 128 and "No names found" in str(error):
            return None
        raise

    try:
        validate_date_tag(candidate)
    except ReleaseHighlightsError:
        return None
    return candidate


def commits_for_range(repository: Path, revision_range: str) -> list[Commit]:
    """Read release commits without relying on platform-specific shell parsing."""

    output = git_output(
        repository,
        "log",
        "--format=%h%x09%H%x09%s",
        revision_range,
    )
    if not output:
        return []

    commits: list[Commit] = []
    for row in output.splitlines():
        short_sha, separator, remainder = row.partition("\t")
        sha, separator, subject = remainder.partition("\t")
        if not separator or not short_sha or not sha or not subject:
            raise ReleaseHighlightsError("could not parse a commit subject for release highlights")
        commits.append(Commit(short_sha=short_sha, sha=sha, subject=subject))
    return commits


def categorized_commits(commits: Sequence[Commit]) -> list[tuple[HighlightCategory, list[Commit]]]:
    """Assign matching commits to the first suitable category without duplicates."""

    assigned: set[str] = set()
    sections: list[tuple[HighlightCategory, list[Commit]]] = []
    for category in CATEGORIES:
        matches: list[Commit] = []
        for commit in commits:
            if commit.sha in assigned or TRACKING_PATTERN.search(commit.subject):
                continue
            if category.pattern.search(commit.subject):
                matches.append(commit)
                assigned.add(commit.sha)
                if len(matches) == MAX_COMMITS_PER_CATEGORY:
                    break
        if matches:
            sections.append((category, matches))
    return sections


def markdown_commit(commit: Commit, repository_url: str) -> str:
    """Format one release entry with an explicit immutable commit link."""

    return f"- {commit.subject} ([#{commit.short_sha}]({repository_url}/commit/{commit.sha}))"


def render_release_body(
    commits: Sequence[Commit],
    *,
    comparison_url: str,
    repository_url: str,
) -> str:
    """Render curated Highlights Markdown for a GitHub Release body prefix."""

    lines = ["## Highlights", "", f"[Full changelog]({comparison_url})", ""]
    sections = categorized_commits(commits)
    if not sections:
        lines.extend(
            [
                "No categorized feature commits were found. See the generated release notes below for the full changelog.",
                "",
            ]
        )
        return "\n".join(lines)

    for category, category_commits in sections:
        lines.extend([f"### {category.title}", ""])
        lines.extend(markdown_commit(commit, repository_url) for commit in category_commits)
        lines.append("")
    return "\n".join(lines)


def generate_release_body(tag: str, repository_url: str, repository: Path) -> str:
    """Build Highlights for *tag* from the checked-out repository history."""

    validate_date_tag(tag)
    normalized_url = repository_url.rstrip("/")
    if not normalized_url:
        raise ReleaseHighlightsError("repository URL must not be empty")

    current_commit = git_output(repository, "rev-list", "-n", "1", tag)
    previous_tag = previous_release_tag(repository, current_commit)
    if previous_tag:
        revision_range = f"{previous_tag}..{tag}"
        comparison_url = f"{normalized_url}/compare/{previous_tag}...{tag}"
    else:
        revision_range = tag
        comparison_url = f"{normalized_url}/commits/{tag}"

    return render_release_body(
        commits_for_range(repository, revision_range),
        comparison_url=comparison_url,
        repository_url=normalized_url,
    )


def parse_args(argv: Sequence[str] | None = None) -> argparse.Namespace:
    """Parse the release workflow's explicit inputs."""

    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tag", required=True, help="Date tag to summarize (YYYY-MM-DD)")
    parser.add_argument(
        "--repository-url",
        required=True,
        help="Canonical repository URL used in changelog and commit links",
    )
    parser.add_argument(
        "--output",
        type=Path,
        required=True,
        help="Markdown output path",
    )
    parser.add_argument(
        "--repository",
        type=Path,
        default=Path.cwd(),
        help="Git repository to inspect (defaults to the current directory)",
    )
    return parser.parse_args(argv)


def main(argv: Sequence[str] | None = None) -> int:
    """Write the release body and return a shell-friendly exit status."""

    args = parse_args(argv)
    try:
        body = generate_release_body(args.tag, args.repository_url, args.repository)
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(body, encoding="utf-8")
    except ReleaseHighlightsError as error:
        print(f"error: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
