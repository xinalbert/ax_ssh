"""Regression tests for AxSSH's date-based release metadata helper."""

from __future__ import annotations

import importlib.util
import plistlib
import sys
import tempfile
import unittest
from pathlib import Path


SCRIPT_PATH = Path(__file__).with_name("release_version.py")
SPEC = importlib.util.spec_from_file_location("release_version", SCRIPT_PATH)
assert SPEC and SPEC.loader
release_version = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = release_version
SPEC.loader.exec_module(release_version)


class ReleaseVersionTests(unittest.TestCase):
    def test_iso_date_maps_to_supported_package_versions(self) -> None:
        version = release_version.release_version_from_date("2026-08-12")

        self.assertEqual(version.public_version, "2026-08-12")
        self.assertEqual(version.cargo_version, "2026.8.12")
        self.assertEqual(version.macos_short_version, "2026.8.12")
        self.assertEqual(version.macos_bundle_version, "20260812")
        self.assertEqual(version.tag, "2026-08-12")

    def test_invalid_dates_and_tags_are_rejected(self) -> None:
        for raw_date in ["2026-2-12", "2026-02-30", "v2026-08-12"]:
            with self.assertRaises(ValueError):
                release_version.release_version_from_date(raw_date)
        with self.assertRaises(ValueError):
            release_version.release_version_from_tag("v2026-08-12")

    def test_sync_updates_only_root_package_and_macos_version_keys(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            cargo_toml = root / "Cargo.toml"
            cargo_lock = root / "Cargo.lock"
            plist = root / "Info.plist"
            cargo_toml.write_text(
                '[package]\nname = "ax_ssh"\nversion = "0.1.0"\n\n[dependencies]\nfoo = "1"\n',
                encoding="utf-8",
            )
            cargo_lock.write_text(
                'version = 4\n\n[[package]]\nname = "ax_ssh"\nversion = "0.1.0"\n\n[[package]]\nname = "other"\nversion = "0.1.0"\n',
                encoding="utf-8",
            )
            with plist.open("wb") as file:
                plistlib.dump(
                    {
                        "CFBundleShortVersionString": "0.1.0",
                        "CFBundleVersion": "1",
                        "CFBundleName": "AxSSH",
                    },
                    file,
                    sort_keys=False,
                )

            version = release_version.release_version_from_date("2026-08-12")
            self.assertTrue(release_version.replace_root_package_version(cargo_toml, "ax_ssh", version.cargo_version))
            self.assertTrue(release_version.replace_root_package_version(cargo_lock, "ax_ssh", version.cargo_version))
            self.assertTrue(release_version.update_macos_plist(plist, version))
            release_version.verify_release_files(
                type(
                    "Arguments",
                    (),
                    {
                        "cargo_toml": str(cargo_toml),
                        "cargo_lock": str(cargo_lock),
                        "macos_plist": str(plist),
                        "package_name": "ax_ssh",
                    },
                )(),
                version,
            )

            self.assertIn('name = "other"\nversion = "0.1.0"', cargo_lock.read_text(encoding="utf-8"))
            with plist.open("rb") as file:
                contents = plistlib.load(file)
            self.assertEqual(contents["CFBundleName"], "AxSSH")
            self.assertEqual(contents["CFBundleVersion"], "20260812")


if __name__ == "__main__":
    unittest.main()
