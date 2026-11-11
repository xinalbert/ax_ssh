"""Check ad-hoc bundle identity without accessing a user's keychain or GUI."""

from __future__ import annotations

import os
import plistlib
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SIGN_APP = ROOT / "packaging/macos/sign-app.sh"
BUILD_APP = ROOT / "packaging/macos/build-app.sh"


@unittest.skipUnless(sys.platform == "darwin", "requires native macOS codesign")
class MacosSigningTests(unittest.TestCase):
    """Exercise actual code requirements, not stored authorization decisions."""

    def setUp(self) -> None:
        self.directory = tempfile.TemporaryDirectory(prefix="axssh-signing-")
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        self.app = self.root / "bundle with spaces" / "AxSSH.app"
        self.binary = self.app / "Contents/MacOS/AxSSH"
        self.binary.parent.mkdir(parents=True)
        self.resources = self.app / "Contents/Resources"
        self.resources.mkdir()
        (self.resources / "fixture.txt").write_text("original\n", encoding="utf-8")
        shutil.copyfile(
            ROOT / "packaging/macos/Info.plist", self.app / "Contents/Info.plist"
        )
        self._compile(0)

    def _compile(self, return_code: int) -> None:
        subprocess.run(
            ["/usr/bin/clang", "-x", "c", "-o", str(self.binary), "-"],
            input=f"int main(void) {{ return {return_code}; }}\n",
            text=True,
            capture_output=True,
            check=True,
            timeout=60,
        )

    def _sign(self) -> None:
        subprocess.run(
            ["sh", str(SIGN_APP), str(self.app)],
            capture_output=True,
            text=True,
            check=True,
            timeout=60,
        )

    def _requirement(self) -> str:
        result = subprocess.run(
            ["/usr/bin/codesign", "--display", "--requirements", "-", str(self.app)],
            capture_output=True,
            text=True,
            check=True,
            timeout=30,
        )
        prefix = "designated => "
        for line in (result.stdout + result.stderr).splitlines():
            line = line.removeprefix("# ")
            if line.startswith(prefix):
                return line.removeprefix(prefix)
        self.fail(f"missing designated requirement: {result.stdout}{result.stderr}")

    def _verify(
        self, requirement: str, app: Path | None = None,
    ) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [
                "/usr/bin/codesign", "--verify", "--deep", "--strict",
                "--test-requirement", "=" + requirement, str(app or self.app),
            ],
            capture_output=True,
            text=True,
            timeout=30,
        )

    def test_repeated_signing_copy_and_relaunch_keep_the_same_requirement(self) -> None:
        self._sign()
        requirement = self._requirement()
        self.assertIn("cdhash", requirement)
        self._sign()
        self.assertEqual(self._requirement(), requirement)
        self.assertEqual(self._verify(requirement).returncode, 0)
        copied = self.root / "copied" / "AxSSH.app"
        shutil.copytree(self.app, copied)
        self.assertEqual(self._verify(requirement, copied).returncode, 0)
        for _ in range(2):
            subprocess.run([str(copied / "Contents/MacOS/AxSSH")], check=True, timeout=30)

    def test_changed_program_does_not_inherit_the_old_requirement(self) -> None:
        self._sign()
        requirement = self._requirement()
        self._compile(1)
        self._sign()
        self.assertNotEqual(self._requirement(), requirement)
        self.assertNotEqual(self._verify(requirement).returncode, 0)

    def test_tampered_resource_fails_signature_verification(self) -> None:
        self._sign()
        requirement = self._requirement()
        (self.resources / "fixture.txt").write_text("changed\n", encoding="utf-8")
        self.assertNotEqual(self._verify(requirement).returncode, 0)

    def test_local_packaging_signs_the_complete_bundle_with_its_identifier(self) -> None:
        environment = os.environ.copy()
        environment["AXSSH_BINARY_PATH"] = str(self.binary)
        environment["AXSSH_BUILD_REVISION"] = "signing-fixture"
        package = self.root / "local package" / "AxSSH.app"
        subprocess.run(
            ["sh", str(BUILD_APP), str(package)],
            env=environment,
            capture_output=True,
            text=True,
            check=True,
            timeout=60,
        )
        with (package / "Contents/Info.plist").open("rb") as stream:
            identifier = plistlib.load(stream)["CFBundleIdentifier"]
        result = self._verify(f'identifier "{identifier}"', package)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue((package / "Contents/_CodeSignature/CodeResources").is_file())


if __name__ == "__main__":
    unittest.main()
