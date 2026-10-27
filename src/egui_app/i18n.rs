//! Immutable PO translations scoped to one egui context; no runtime catalog I/O.

use ax_ssh::config::UiLanguage;
use eframe::egui;

mod catalog;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Translator {
    locale: &'static str,
}

impl Default for Translator {
    fn default() -> Self {
        Self { locale: "en" }
    }
}

impl Translator {
    pub(super) fn new(language: UiLanguage, system_locale: Option<&str>) -> Self {
        Self {
            locale: language.resolved_locale(system_locale),
        }
    }

    pub(super) fn text(self, context: &str, source: &'static str) -> &'static str {
        let messages = catalog::messages(self.locale);
        messages
            .binary_search_by(|(key_context, key, _)| (*key_context, *key).cmp(&(context, source)))
            .ok()
            .map_or(source, |index| messages[index].2)
    }

    pub(super) fn apply(self, ctx: &egui::Context) {
        if current(ctx) != self {
            ctx.data_mut(|data| data.insert_temp(language_id(), self));
        }
    }
}

fn language_id() -> egui::Id {
    egui::Id::unique("axssh-ui-language")
}

pub(super) fn current(ctx: &egui::Context) -> Translator {
    ctx.data(|data| data.get_temp::<Translator>(language_id()))
        .unwrap_or_default()
}

macro_rules! tr {
    ($source:literal, $ctx:expr) => {
        $crate::egui_app::i18n::current($ctx).text("", $source)
    };
    ($context:literal => $source:literal, $ctx:expr) => {
        $crate::egui_app::i18n::current($ctx).text($context, $source)
    };
}
macro_rules! mark {
    ($source:literal) => {
        $source
    };
}
pub(super) use {mark, tr};

#[cfg(test)]
mod tests;
