"""Exercise repository checks using isolated files, without Git or network."""

from __future__ import annotations

import importlib.util
import tempfile
import unittest
from pathlib import Path


SPEC = importlib.util.spec_from_file_location(
    "check_repository", Path(__file__).with_name("check_repository.py")
)
assert SPEC and SPEC.loader
repository = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(repository)


class RepositoryTests(unittest.TestCase):
    """Keep ownership checks portable and avoid Markdown example false alarms."""

    def test_targets_skip_code_and_keep_images_references_and_parentheses(self) -> None:
        source = """[guide](usage.md#part) ![icon](assets/icon(1).png)
[spaced](<a b.md>) [site](https://example.com/x(y))
[reference]: reference.md
`[inline example](absent.md)`
````md
[fenced example](absent.md)
```
````
~~~
[another example](absent.md)
~~~
"""
        self.assertEqual(
            repository.markdown_targets(source),
            [
                "usage.md#part", "assets/icon(1).png", "a b.md",
                "https://example.com/x(y)", "reference.md",
            ],
        )

    def test_local_links_decode_paths_reject_escapes_and_require_pairs(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            page = root / "guide.zh.md"
            (root / "space name.md").write_text("# Guide\n", encoding="utf-8")
            page.write_text(
                "[valid](space%20name.md#heading) [root](/space%20name.md)\n"
                "[web](https://example.invalid) [anchor](#here)\n"
                "[missing](missing.md) [escape](../outside.md)\n",
                encoding="utf-8",
            )
            errors = repository.validate_markdown(root, [page])
            self.assertEqual(len(errors), 3)
            self.assertTrue(any("English counterpart" in error for error in errors))
            self.assertTrue(any("missing.md" in error for error in errors))
            self.assertTrue(any("escapes repository" in error for error in errors))

    def test_owned_markdown_excludes_vendor_and_reference_trees(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for name in (
                "README.md", "docs/guide.md", "assets/fonts/README.md",
                "vendor/lib/README.md", "third_package/axshell/README.md",
            ):
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text("[irrelevant](missing.md)\n", encoding="utf-8")
            self.assertEqual(len(repository.owned_markdown(root)), 3)
            page = root / "README.md"
            page.write_text(
                "[reference](third_package/axshell/README.md)\n", encoding="utf-8"
            )
            errors = repository.validate_markdown(root, [page])
            self.assertEqual(len(errors), 1)
            self.assertIn("reference checkout cannot be a link target", errors[0])

    def test_layout_rejects_duplicate_entries_old_modules_and_extra_ui_entries(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "src").mkdir()
            app = root / "src/app.rs"
            app.write_text("slint::include_modules!();\n", encoding="utf-8")
            build = root / "build.rs"
            build.write_text(
                'slint_build::compile_with_config("ui/app.slint", config);\n',
                encoding="utf-8",
            )
            self.assertEqual(repository.validate_source_layout(root), [])
            (root / "src/mod.rs").write_text(
                "slint::include_modules!();\n", encoding="utf-8"
            )
            build.write_text('slint_build::compile("ui/other.slint");\n', encoding="utf-8")
            self.assertEqual(len(repository.validate_source_layout(root)), 3)


if __name__ == "__main__":
    unittest.main()
