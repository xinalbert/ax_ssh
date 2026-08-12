"""Regression tests for the GitHub Release Highlights generator."""

from __future__ import annotations

import importlib.util
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


SCRIPT_PATH = Path(__file__).with_name("generate_release_highlights.py")
SPEC = importlib.util.spec_from_file_location("generate_release_highlights", SCRIPT_PATH)
assert SPEC and SPEC.loader
release_highlights = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = release_highlights
SPEC.loader.exec_module(release_highlights)


class ReleaseHighlightsTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temp_dir = tempfile.TemporaryDirectory()
        self.repository = Path(self.temp_dir.name)
        self.git("init")
        self.git("config", "user.name", "Release Test")
        self.git("config", "user.email", "release-test@example.invalid")

    def tearDown(self) -> None:
        self.temp_dir.cleanup()

    def git(self, *args: str) -> str:
        result = subprocess.run(
            ["git", *args],
            cwd=self.repository,
            check=True,
            capture_output=True,
            text=True,
        )
        return result.stdout.strip()

    def commit(self, subject: str) -> str:
        marker = self.repository / "history.txt"
        with marker.open("a", encoding="utf-8") as output:
            output.write(f"{subject}\n")
        self.git("add", "history.txt")
        self.git("commit", "-m", subject)
        return self.git("rev-parse", "HEAD")

    def tag(self, name: str) -> None:
        self.git("tag", "-a", name, "-m", f"AxSSH {name}")

    def test_release_body_groups_commits_and_excludes_tracking_records(self) -> None:
        self.commit("Initial workspace")
        self.tag("2026-08-10")
        terminal_sha = self.commit("Improve terminal selection")
        sftp_sha = self.commit("Add SFTP transfer progress")
        self.commit("docs(tracking): record terminal progress")
        self.commit("Package release assets")
        self.tag("2026-08-12")

        body = release_highlights.generate_release_body(
            "2026-08-12",
            "https://github.example/AxSSH/ax_ssh",
            self.repository,
        )

        self.assertIn(
            "[Full changelog](https://github.example/AxSSH/ax_ssh/compare/2026-08-10...2026-08-12)",
            body,
        )
        self.assertIn("### Terminal", body)
        self.assertIn("### SFTP", body)
        self.assertIn("### Packaging and release", body)
        self.assertIn(f"/commit/{terminal_sha}", body)
        self.assertIn(f"/commit/{sftp_sha}", body)
        self.assertNotIn("docs(tracking)", body)

    def test_commit_is_listed_only_in_its_first_matching_category(self) -> None:
        self.commit("Initial workspace")
        self.tag("2026-08-10")
        self.commit("Improve terminal SSH authentication")
        self.tag("2026-08-12")

        body = release_highlights.generate_release_body(
            "2026-08-12",
            "https://github.example/AxSSH/ax_ssh/",
            self.repository,
        )

        self.assertEqual(body.count("Improve terminal SSH authentication"), 1)
        self.assertIn("### Terminal", body)
        self.assertNotIn("### SSH and sessions", body)

    def test_category_keeps_only_the_eight_most_recent_commits(self) -> None:
        self.commit("Initial workspace")
        self.tag("2026-08-10")
        for index in range(9):
            self.commit(f"Improve terminal output {index}")
        self.tag("2026-08-12")

        body = release_highlights.generate_release_body(
            "2026-08-12",
            "https://github.example/AxSSH/ax_ssh",
            self.repository,
        )

        self.assertEqual(body.count("Improve terminal output"), 8)
        self.assertNotIn("Improve terminal output 0", body)

    def test_first_release_uses_a_commit_history_link_and_fallback(self) -> None:
        self.commit("Initialize repository")
        self.tag("2026-08-12")

        body = release_highlights.generate_release_body(
            "2026-08-12",
            "https://github.example/AxSSH/ax_ssh",
            self.repository,
        )

        self.assertIn(
            "[Full changelog](https://github.example/AxSSH/ax_ssh/commits/2026-08-12)",
            body,
        )
        self.assertIn("No categorized feature commits were found.", body)

    def test_invalid_date_tag_is_rejected(self) -> None:
        with self.assertRaises(release_highlights.ReleaseHighlightsError):
            release_highlights.validate_date_tag("v2026-08-12")
        with self.assertRaises(release_highlights.ReleaseHighlightsError):
            release_highlights.validate_date_tag("2026-02-30")

    def test_unexpected_git_failure_is_not_treated_as_a_first_release(self) -> None:
        with self.assertRaises(release_highlights.GitCommandError):
            release_highlights.previous_release_tag(self.repository, "not-a-commit")


if __name__ == "__main__":
    unittest.main()
