//! Static language policies and locale resolution, independent of Slint.

use serde::{Deserialize, Serialize};

#[derive(Debug)]
struct LanguageDefinition {
    locale: &'static str,
    native_name: &'static str,
    aliases: &'static [&'static str],
    setting_aliases: &'static [&'static str],
    script: Option<&'static str>,
    language_default: bool,
    region_scripts: &'static [(&'static str, &'static str)],
}

include!(concat!(env!("OUT_DIR"), "/language_registry.rs"));

/// Select the system language or one locale from the compiled registry.
///
/// Locale identifiers, rather than selector indices or translated labels, are
/// persisted. Construction accepts only bundled locales and declared aliases.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UiLanguage(Option<&'static str>);

impl UiLanguage {
    pub const SYSTEM: Self = Self(None);

    pub const fn as_setting(self) -> &'static str {
        match self.0 {
            Some(locale) => locale,
            None => "system",
        }
    }

    pub fn from_setting(value: &str) -> Self {
        let value = normalize_locale(value);
        LANGUAGES
            .iter()
            .find(|language| {
                std::iter::once(language.locale)
                    .chain(language.aliases.iter().copied())
                    .chain(language.setting_aliases.iter().copied())
                    .any(|alias| alias.eq_ignore_ascii_case(&value))
            })
            .map_or(Self::SYSTEM, |language| Self(Some(language.locale)))
    }

    /// Enumerate explicit choices in the same order as the Settings model.
    pub fn bundled() -> impl ExactSizeIterator<Item = Self> + DoubleEndedIterator {
        LANGUAGES.iter().map(|language| Self(Some(language.locale)))
    }

    pub fn native_name(self) -> &'static str {
        LANGUAGES
            .iter()
            .find(|language| Some(language.locale) == self.0)
            .map_or("Follow system", |language| language.native_name)
    }

    /// Index zero is System; the remaining indices belong to this build only.
    pub fn from_selector_index(index: i32) -> Self {
        usize::try_from(index)
            .ok()
            .and_then(|index| index.checked_sub(1))
            .and_then(|index| LANGUAGES.get(index))
            .map_or(Self::SYSTEM, |language| Self(Some(language.locale)))
    }

    pub fn selector_index(self) -> i32 {
        LANGUAGES
            .iter()
            .position(|language| Some(language.locale) == self.0)
            .and_then(|index| i32::try_from(index + 1).ok())
            .unwrap_or(0)
    }

    pub fn resolved_locale(self, system_locale: Option<&str>) -> &'static str {
        self.0
            .unwrap_or_else(|| resolve_locale_in(system_locale, LANGUAGES))
    }
}

impl Serialize for UiLanguage {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_setting())
    }
}

impl<'de> Deserialize<'de> for UiLanguage {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Ok(Self::from_setting(&String::deserialize(deserializer)?))
    }
}

fn normalize_locale(locale: &str) -> String {
    locale
        .trim()
        .split(['.', '@'])
        .next()
        .unwrap_or_default()
        .replace('_', "-")
        .to_ascii_lowercase()
}

struct LocaleParts<'a> {
    language: &'a str,
    script: Option<&'a str>,
    region: Option<&'a str>,
}

impl<'a> LocaleParts<'a> {
    fn new(locale: &'a str) -> Self {
        let mut parts = locale.split('-');
        let language = parts.next().unwrap_or_default();
        let mut script = None;
        let mut region = None;
        for part in parts {
            // Stop at BCP 47 extensions/private use; their data is not a
            // language script or region (for example en-u-rg-cnzzzz).
            if part.len() == 1 {
                break;
            }
            if script.is_none() && part.len() == 4 && part.bytes().all(|c| c.is_ascii_alphabetic())
            {
                script = Some(part);
            } else if region.is_none()
                && (part.len() == 2 && part.bytes().all(|c| c.is_ascii_alphabetic())
                    || part.len() == 3 && part.bytes().all(|c| c.is_ascii_digit()))
            {
                region = Some(part);
            }
        }
        Self {
            language,
            script,
            region,
        }
    }
}

fn locale_names(language: &LanguageDefinition) -> impl Iterator<Item = &'static str> + '_ {
    std::iter::once(language.locale).chain(language.aliases.iter().copied())
}

// Called at startup, explicit selection, and settings search. It neither
// changes terminal worker locales nor reads translation files at runtime.
fn resolve_locale_in(locale: Option<&str>, languages: &[LanguageDefinition]) -> &'static str {
    let locale = normalize_locale(locale.unwrap_or_default());
    if let Some(language) = languages
        .iter()
        .find(|language| locale_names(language).any(|alias| alias.eq_ignore_ascii_case(&locale)))
    {
        return language.locale;
    }
    let parts = LocaleParts::new(&locale);
    let same_language = |language: &&LanguageDefinition| {
        LocaleParts::new(language.locale)
            .language
            .eq_ignore_ascii_case(parts.language)
    };
    let script = parts.script.or_else(|| {
        let region = parts.region?;
        languages.iter().filter(same_language).find_map(|language| {
            language
                .region_scripts
                .iter()
                .find_map(|(candidate, script)| {
                    candidate.eq_ignore_ascii_case(region).then_some(*script)
                })
        })
    });
    let supports_script = |language: &&LanguageDefinition| {
        script.is_none_or(|script| {
            language
                .script
                .is_none_or(|supported| supported.eq_ignore_ascii_case(script))
        })
    };
    if let Some(script) = script
        && let Some(language) = languages.iter().filter(same_language).find(|language| {
            language
                .script
                .is_some_and(|supported| supported.eq_ignore_ascii_case(script))
                || locale_names(language).any(|alias| {
                    LocaleParts::new(alias)
                        .script
                        .is_some_and(|candidate| candidate.eq_ignore_ascii_case(script))
                })
        })
    {
        return language.locale;
    }
    if let Some(region) = parts.region
        && let Some(language) = languages
            .iter()
            .filter(same_language)
            .filter(supports_script)
            .find(|language| {
                locale_names(language).any(|alias| {
                    LocaleParts::new(alias)
                        .region
                        .is_some_and(|candidate| candidate.eq_ignore_ascii_case(region))
                })
            })
    {
        return language.locale;
    }
    let compatible = || {
        languages
            .iter()
            .filter(same_language)
            .filter(supports_script)
    };
    compatible()
        .find(|language| language.language_default)
        .or_else(|| {
            compatible().find(|language| {
                let parts = LocaleParts::new(language.locale);
                parts.script.is_none() && parts.region.is_none()
            })
        })
        .map_or("en", |language| language.locale)
}

#[cfg(test)]
mod tests;

#[cfg(test)]
#[path = "../../build/languages.rs"]
mod registry_build;
