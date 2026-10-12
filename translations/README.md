[简体中文](README.zh.md)

# Interface translations

AxSSH includes English source text and eleven bundled catalogs:

| Locale | Native name |
| --- | --- |
| `en` | English (source) |
| `de` | Deutsch |
| `en-GB` | English (United Kingdom) |
| `es` | Español |
| `fr` | Français |
| `ja` | 日本語 |
| `ko` | 한국어 |
| `pt-BR` | Português (Brasil) |
| `ru` | Русский |
| `uk` | Українська |
| `zh-CN` | 简体中文 |
| `zh-TW` | 繁體中文 |

Translations are compiled by Slint into static tables. The running application
never scans this directory or parses PO files.

Edit `LOCALE/LC_MESSAGES/ax_ssh.po` directly. The context-free entries translate
Slint `@tr` messages. Entries with `msgctxt "settings-search"` translate the
fixed search routes in `settings-search.json`; those entries generate static
Rust arrays without changing their English route IDs.

After editing UI strings, a catalog, or the search routes, run:

```bash
python3 scripts/build_translations.py
python3 scripts/check_translations.py
python3 scripts/build_translations.py --check
python3 -m unittest scripts/test_translations.py -v
```

Run these commands from the repository root. Python 3.10+ and `rustfmt` are
required. `build_translations.py` retains existing translations and translator
comments, removes stale entries, and adds new entries with an empty translation.
Fill these entries and rerun the command; it refuses to generate a search table
from missing or fuzzy search translations. `--check` changes no files. The old
`build_zh_catalog.py` command delegates to this workflow for `zh-CN`.

When adding a language, create `LOCALE/LC_MESSAGES/ax_ssh.po` with matching
`Language` and appropriate `Plural-Forms` headers, plus `LOCALE/language.json`.
For example, the French catalog uses:

```json
{
  "native_name": "Français",
  "aliases": ["fr-FR"],
  "language_default": true
}
```

Complete both Slint and search entries, then run the commands above and Cargo
verification. `build.rs` discovers catalog directories and generates the
registry into Cargo's output directory; Settings uses that registry without
per-language Rust or Slint edits. English source metadata lives in
`en/language.json` and needs no PO file. Choices are System, English, then
catalogs sorted by locale. Selector indices are transient and never persisted.

Metadata uses canonical `language[-Script][-REGION]` identifiers. `native_name`
is required; other fields are optional. `aliases` declares system locale aliases
of the same language. `setting_aliases` retains historical config values.
`script` declares a writing system such as `Hans` or `Hant` (also inferred from
script-qualified locale names/aliases). `language_default` selects at most one
fallback catalog per language; a language-only catalog is the implicit fallback.
`region_scripts` maps regions to scripts even when that script is not bundled.
See `zh-CN/language.json` for the Chinese policy. The build rejects missing
metadata/catalog pairs, invalid names/identifiers, duplicate aliases/defaults,
unknown fields, and conflicting script hints.

Configuration writes `system` or canonical locales (`en`, `zh-CN`, `ja`).
Legacy `english`, `simplified-chinese`, and `japanese` values still load.
System selection tries exact locale/alias, script (explicit or inferred from
region), region, language fallback, then English. Incompatible scripts never
fall back to another script of that language. `zh-TW`, `zh-HK`, `zh-MO`, and
`zh-Hant` select Traditional Chinese; plain `zh` remains Simplified Chinese.
French, Spanish, German, and Korean regions fall back to their bundled language
catalog. Portuguese uses the Brazilian catalog, including a `pt-PT` system locale
until a separate Portugal catalog is added.
Russian (`ru-RU`) and Ukrainian (`uk-UA`) match their own language catalogs.
British English (`en-GB`, with `en-UK` as an alias) uses British display spelling,
including colour, licence, and behaviour. Other English regions such as `en-US`
and `en-AU` continue to use source English. Locale `uk` means Ukrainian; the
United Kingdom is represented by region `GB` in `en-GB`.

The validator checks every catalog for missing/stale entries, empty/fuzzy
translations, duplicate keys, locale headers, plural forms, and format arguments.
Numeric placeholders may be reordered; neither their identity nor number of
occurrences may change. CI and release preflight check catalogs and generated
search arrays.

Runtime technical errors, logs, user values, and remote terminal text are outside
this catalog. Language changes reuse the existing save-before-apply transaction
and update main and detached windows on the UI thread. New translations still
need user review for wording, text length, fonts, and keyboard/focus behavior.
