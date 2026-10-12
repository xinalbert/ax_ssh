"""Exercise catalog validation, source extraction, and static search generation."""

from __future__ import annotations

import importlib.util
import json
import sys
import tempfile
import unittest
from pathlib import Path
from types import ModuleType


def load_helper(name: str) -> ModuleType:
    """Load a sibling CLI helper without changing Python's import path."""
    spec = importlib.util.spec_from_file_location(
        name, Path(__file__).with_name(f"{name}.py")
    )
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


catalog = load_helper("translation_catalog")
validator = load_helper("check_translations")
generator = load_helper("build_translations")


class TranslationTests(unittest.TestCase):
    """Keep malformed or incomplete translations out of compiled artifacts."""

    def write_catalog(self, root: Path, entries: list, locale: str = "ja") -> Path:
        path = root / "translations" / locale / "LC_MESSAGES/ax_ssh.po"
        path.parent.mkdir(parents=True, exist_ok=True)
        metadata = f"Language: {locale}\nPlural-Forms: nplurals=1; plural=0;\n"
        header = catalog.CatalogEntry(catalog.MessageKey("", ""), {0: metadata})
        path.write_text(catalog.render_catalog([header, *entries]), encoding="utf-8")
        return path

    def test_source_extraction_preserves_context_plural_and_ignores_comments(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "sample.slint").write_text(
                '// @tr("Comment")\nText { text: @tr("Hello {}", name); }\n'
                '/* @tr("Another comment") */\n'
                'Text { text: @tr("menu" => "Open"); }\n'
                'Text { text: @tr("one {n}" | "many {n}" % count); }\n',
                encoding="utf-8",
            )
            self.assertEqual(
                catalog.slint_keys(root),
                {
                    catalog.MessageKey("", "Hello {}"),
                    catalog.MessageKey("menu", "Open"),
                    catalog.MessageKey("", "one {n}", "many {n}"),
                },
            )

    def test_po_contexts_multiline_and_annotations_round_trip(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "sample.po"
            path.write_text(
                "# translator note\n#, fuzzy, rust-format\n"
                'msgctxt "menu"\nmsgid "Open"\nmsgstr "開く"\n\n'
                'msgid "Hello "\n"world"\nmsgstr "こんにちは"\n',
                encoding="utf-8",
            )
            entries = catalog.read_catalog(path)
            key = catalog.MessageKey("menu", "Open")
            self.assertTrue(entries[key].fuzzy)
            self.assertEqual(
                entries[catalog.MessageKey("", "Hello world")].translations[0],
                "こんにちは",
            )
            path.write_text(
                catalog.render_catalog(list(entries.values())), encoding="utf-8"
            )
            self.assertEqual(catalog.read_catalog(path), entries)

    def test_duplicate_po_key_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "duplicate.po"
            path.write_text(
                'msgid "Save"\nmsgstr "保存"\n\nmsgid "Save"\nmsgstr "保存"\n',
                encoding="utf-8",
            )
            with self.assertRaisesRegex(ValueError, "duplicate PO key"):
                catalog.read_catalog(path)

    def test_validator_detects_fuzzy_missing_empty_stale_and_wrong_arguments(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            entry = catalog.CatalogEntry(
                catalog.MessageKey("", "Hello {0}"), {0: "こんにちは{1}"}, ["#, fuzzy"]
            )
            empty = catalog.CatalogEntry(catalog.MessageKey("", "Empty"), {0: ""})
            stale = catalog.CatalogEntry(
                catalog.MessageKey("", "Obsolete"), {0: "古い"}
            )
            path = self.write_catalog(root, [entry, empty, stale])
            keys = {entry.key, empty.key, catalog.MessageKey("", "Missing")}
            errors = "\n".join(validator.validate_catalog(path, keys))
            for expected in [
                "fuzzy entry",
                "missing entry",
                "empty translation",
                "stale entry",
                "mismatched placeholders",
            ]:
                self.assertIn(expected, errors)

    def test_placeholder_validation_accepts_reordering_but_rejects_dropped_duplicates(
        self,
    ) -> None:
        self.assertEqual(
            catalog.placeholder_signature("{} {}"),
            catalog.placeholder_signature("{1} {0}"),
        )
        self.assertEqual(
            catalog.placeholder_signature("{{name}} {0}"),
            catalog.placeholder_signature("{0} {{名前}}"),
        )
        self.assertNotEqual(
            catalog.placeholder_signature("{0} {0}"),
            catalog.placeholder_signature("{0}"),
        )
        with self.assertRaises(ValueError):
            catalog.placeholder_signature("unclosed {")
        with self.assertRaises(ValueError):
            catalog.placeholder_signature("mixed {} {0}")

    def test_locale_metadata_and_plural_form_counts_are_validated(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            key = catalog.MessageKey("", "one {n}", "many {n}")
            path = self.write_catalog(
                Path(directory), [catalog.CatalogEntry(key, {0: "{n}個"})]
            )
            self.assertEqual(validator.validate_catalog(path, {key}), [])
            text = (
                path.read_text(encoding="utf-8")
                .replace("Language: ja", "Language: fr")
                .replace("nplurals=1", "nplurals=2")
            )
            path.write_text(text, encoding="utf-8")
            errors = "\n".join(validator.validate_catalog(path, {key}))
            self.assertIn("Language header", errors)
            self.assertIn("invalid translation forms", errors)

    def test_sync_preserves_reviewed_translations_and_exposes_new_empty_entries(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as directory:
            existing = catalog.CatalogEntry(
                catalog.MessageKey("", "Save"), {0: "保存"}, ["# keep this note"]
            )
            path = self.write_catalog(Path(directory), [existing])
            new_key = catalog.MessageKey("", "Cancel")
            content = generator.synchronize_catalog(path, {existing.key, new_key})
            path.write_text(content, encoding="utf-8")
            entries = catalog.read_catalog(path)
            self.assertEqual(entries[existing.key], existing)
            self.assertEqual(entries[new_key].translations, {0: ""})

    def test_search_generation_embeds_only_static_translated_rows(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            rows = [["General", "Language", "Interface language"]]
            entries = [
                catalog.CatalogEntry(
                    catalog.MessageKey("settings-search", source), {0: translated}
                )
                for source, translated in zip(
                    rows[0], ["一般", "言語", "表示言語"], strict=True
                )
            ]
            self.write_catalog(root, entries)
            (root / "translations/settings-search.json").write_text(
                json.dumps(rows), encoding="utf-8"
            )
            output = generator.render_search_catalog(root)
            self.assertIn('"ja" => Some(SETTINGS_SEARCH_JA)', output)
            self.assertIn('("一般", "言語", "表示言語")', output)
            self.assertNotIn("HashMap", output)
            self.assertNotIn("read_to_string", output)


if __name__ == "__main__":
    unittest.main()
