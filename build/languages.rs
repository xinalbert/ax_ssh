//! Discover and validate Slint catalog metadata before generating static data.

use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::Path;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Metadata {
    native_name: String,
    #[serde(default)]
    aliases: Vec<String>,
    #[serde(default)]
    setting_aliases: Vec<String>,
    #[serde(default)]
    script: Option<String>,
    #[serde(default)]
    language_default: bool,
    #[serde(default)]
    region_scripts: BTreeMap<String, String>,
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

fn valid_locale(locale: &str) -> bool {
    let mut parts = locale.split('-');
    let base = parts.next().unwrap_or_default();
    if !(2..=3).contains(&base.len()) || !base.bytes().all(|byte| byte.is_ascii_lowercase()) {
        return false;
    }
    let mut next = parts.next();
    if next.is_some_and(valid_script) {
        next = parts.next();
    }
    if next.is_some_and(|region| {
        !(region.len() == 2 && region.bytes().all(|byte| byte.is_ascii_uppercase())
            || region.len() == 3 && region.bytes().all(|byte| byte.is_ascii_digit()))
    }) {
        return false;
    }
    parts.next().is_none()
}

fn valid_script(script: &str) -> bool {
    script.len() == 4
        && script.as_bytes()[0].is_ascii_uppercase()
        && script.as_bytes()[1..].iter().all(u8::is_ascii_lowercase)
}

fn discover(root: &Path) -> io::Result<Vec<(String, Metadata)>> {
    let mut paths = fs::read_dir(root)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<io::Result<Vec<_>>>()?;
    paths.sort();
    let mut result = Vec::new();
    let mut settings = BTreeSet::from(["system".to_owned()]);
    let mut aliases = BTreeSet::new();
    let mut defaults = BTreeSet::new();
    let mut hints = BTreeMap::new();
    for path in paths {
        let catalog = path.join("LC_MESSAGES/ax_ssh.po");
        let metadata_path = path.join("language.json");
        if !catalog.is_file() && !metadata_path.is_file() {
            continue;
        }
        let locale = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| invalid(format!("invalid locale directory {}", path.display())))?
            .to_owned();
        if !valid_locale(&locale) {
            return Err(invalid(format!("noncanonical locale directory {locale}")));
        }
        if locale != "en" && !catalog.is_file() {
            return Err(invalid(format!("{locale}: missing LC_MESSAGES/ax_ssh.po")));
        }
        if locale == "en" && catalog.is_file() {
            return Err(invalid("en uses English source text, not a PO catalog"));
        }
        let bytes = fs::read(&metadata_path).map_err(|error| {
            invalid(format!(
                "failed to read {}: {error}",
                metadata_path.display()
            ))
        })?;
        let mut metadata: Metadata = serde_json::from_slice(&bytes)
            .map_err(|error| invalid(format!("invalid {}: {error}", metadata_path.display())))?;
        if metadata.native_name.trim().is_empty()
            || metadata.native_name != metadata.native_name.trim()
            || metadata.native_name.chars().any(char::is_control)
        {
            return Err(invalid(format!("{locale}: invalid native_name")));
        }
        if metadata
            .script
            .as_deref()
            .is_some_and(|script| !valid_script(script))
        {
            return Err(invalid(format!("{locale}: invalid script")));
        }
        let base = locale.split('-').next().unwrap_or_default();
        if metadata.language_default && !defaults.insert(base.to_owned()) {
            return Err(invalid(format!("{base}: duplicate language_default")));
        }
        for alias in std::iter::once(&locale).chain(metadata.aliases.iter()) {
            if !valid_locale(alias)
                || alias.split('-').next() != Some(base)
                || !aliases.insert(alias.to_ascii_lowercase())
            {
                return Err(invalid(format!(
                    "{locale}: invalid or duplicate locale alias {alias}"
                )));
            }
            if let Some(script) = alias.split('-').find(|part| valid_script(part)) {
                if metadata
                    .script
                    .as_deref()
                    .is_some_and(|supported| supported != script)
                {
                    return Err(invalid(format!(
                        "{locale}: conflicting alias script {script}"
                    )));
                }
                metadata.script = Some(script.to_owned());
            }
        }
        for setting in std::iter::once(&locale)
            .chain(metadata.aliases.iter())
            .chain(metadata.setting_aliases.iter())
        {
            if setting.is_empty()
                || !setting
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
                || !settings.insert(setting.to_ascii_lowercase())
            {
                return Err(invalid(format!(
                    "{locale}: invalid or duplicate setting alias {setting}"
                )));
            }
        }
        for (region, script) in &metadata.region_scripts {
            if !(region.len() == 2 && region.bytes().all(|byte| byte.is_ascii_uppercase())
                || region.len() == 3 && region.bytes().all(|byte| byte.is_ascii_digit()))
                || !valid_script(script)
            {
                return Err(invalid(format!("{locale}: invalid region_scripts")));
            }
            let key = (base.to_owned(), region.clone());
            if let Some(previous) = hints.insert(key, script.clone())
                && previous != *script
            {
                return Err(invalid(format!(
                    "{locale}: conflicting region script for {region}"
                )));
            }
        }
        result.push((locale, metadata));
    }
    if !result.iter().any(|(locale, _)| locale == "en") {
        return Err(invalid("missing translations/en/language.json"));
    }
    // English source is the first explicit choice; all catalog choices are
    // deterministic. Indices are transient, never saved to configuration.
    result.sort_by(|left, right| (left.0 != "en", &left.0).cmp(&(right.0 != "en", &right.0)));
    Ok(result)
}

pub(super) fn generate(root: &Path, out_dir: &Path) -> io::Result<()> {
    let languages = discover(root)?;
    let mut output = String::from(
        "// Generated from translations/*/language.json by build.rs.\n\
         const LANGUAGES: &[LanguageDefinition] = &[\n",
    );
    for (locale, metadata) in languages {
        output.push_str(&format!(
            "LanguageDefinition {{ locale: {locale:?}, native_name: {:?}, aliases: &{:?}, \
             setting_aliases: &{:?}, script: {:?}, language_default: {}, region_scripts: &[",
            metadata.native_name,
            metadata.aliases,
            metadata.setting_aliases,
            metadata.script,
            metadata.language_default,
        ));
        for (region, script) in metadata.region_scripts {
            output.push_str(&format!("({region:?}, {script:?}),"));
        }
        output.push_str("] },\n");
    }
    output.push_str("];\n");
    let path = out_dir.join("language_registry.rs");
    fs::write(&path, output)
        .map_err(|error| invalid(format!("failed to write {}: {error}", path.display())))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("axssh-languages-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(path.join("en")).expect("fixture directory");
        fs::write(
            path.join("en/language.json"),
            r#"{"native_name":"English","setting_aliases":["english"],"language_default":true}"#,
        )
        .expect("English metadata");
        path
    }

    #[test]
    fn new_catalog_is_discovered_without_source_changes() {
        let root = fixture();
        fs::create_dir_all(root.join("fr/LC_MESSAGES")).expect("French directory");
        fs::write(root.join("fr/LC_MESSAGES/ax_ssh.po"), "").expect("French catalog");
        fs::write(
            root.join("fr/language.json"),
            r#"{"native_name":"Français"}"#,
        )
        .expect("French metadata");
        generate(&root, &root).expect("generated registry");
        let output = fs::read_to_string(root.join("language_registry.rs")).expect("registry");
        assert!(output.contains("locale: \"fr\", native_name: \"Français\""));
        fs::remove_dir_all(root).expect("remove fixture");
    }

    #[test]
    fn missing_catalog_metadata_and_colliding_aliases_are_rejected() {
        let root = fixture();
        fs::create_dir_all(root.join("fr/LC_MESSAGES")).expect("French directory");
        fs::write(root.join("fr/LC_MESSAGES/ax_ssh.po"), "").expect("French catalog");
        assert!(
            discover(&root)
                .unwrap_err()
                .to_string()
                .contains("language.json")
        );
        fs::write(
            root.join("fr/language.json"),
            r#"{"native_name":"Français","aliases":["en"]}"#,
        )
        .expect("colliding metadata");
        assert!(
            discover(&root)
                .unwrap_err()
                .to_string()
                .contains("duplicate locale alias")
        );
        fs::remove_file(root.join("fr/LC_MESSAGES/ax_ssh.po")).expect("remove catalog");
        assert!(
            discover(&root)
                .unwrap_err()
                .to_string()
                .contains("missing LC_MESSAGES")
        );
        fs::remove_dir_all(root).expect("remove fixture");
    }

    #[test]
    fn malformed_metadata_cannot_enter_the_static_registry() {
        for (metadata, expected) in [
            (r#"{"native_name":""}"#, "invalid native_name"),
            (r#"{"native_name":" Français "}"#, "invalid native_name"),
            (
                r#"{"native_name":"Français","unknown":true}"#,
                "unknown field",
            ),
            (
                r#"{"native_name":"Français","script":"latin"}"#,
                "invalid script",
            ),
            (
                r#"{"native_name":"Français","aliases":["fr-FR-FR"]}"#,
                "locale alias",
            ),
            (
                r#"{"native_name":"Français","setting_aliases":["english"]}"#,
                "setting alias",
            ),
            (
                r#"{"native_name":"Français","region_scripts":{"FR":"latin"}}"#,
                "region_scripts",
            ),
            (
                r#"{"native_name":"Français","script":"Latn","aliases":["fr-Cyrl"]}"#,
                "alias script",
            ),
        ] {
            let root = fixture();
            fs::create_dir_all(root.join("fr/LC_MESSAGES")).expect("French directory");
            fs::write(root.join("fr/LC_MESSAGES/ax_ssh.po"), "").expect("French catalog");
            fs::write(root.join("fr/language.json"), metadata).expect("invalid metadata");
            let error = discover(&root).unwrap_err().to_string();
            assert!(error.contains(expected), "{metadata}: {error}");
            fs::remove_dir_all(root).expect("remove fixture");
        }
    }

    #[test]
    fn regional_script_is_inferred_and_conflicting_defaults_are_rejected() {
        let root = fixture();
        for locale in ["fr", "fr-CA"] {
            fs::create_dir_all(root.join(locale).join("LC_MESSAGES")).expect("catalog directory");
            fs::write(root.join(locale).join("LC_MESSAGES/ax_ssh.po"), "").expect("catalog");
            fs::write(
                root.join(locale).join("language.json"),
                r#"{"native_name":"Français","language_default":true}"#,
            )
            .expect("metadata");
        }
        assert!(
            discover(&root)
                .unwrap_err()
                .to_string()
                .contains("duplicate language_default")
        );
        fs::write(
            root.join("fr-CA/language.json"),
            r#"{"native_name":"Français (Canada)","aliases":["fr-Latn-CA"]}"#,
        )
        .expect("script metadata");
        let languages = discover(&root).expect("registry");
        let regional = languages
            .iter()
            .find(|(locale, _)| locale == "fr-CA")
            .expect("regional locale");
        assert_eq!(regional.1.script.as_deref(), Some("Latn"));
        fs::remove_dir_all(root).expect("remove fixture");
    }
}
