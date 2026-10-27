//! Regex recognition of log severity names and contextual HTTP status codes.
//!
//! Severity names follow OpenTelemetry and RFC 5424's syslog severity table.
//! HTTP classes follow RFC 9110; color choices belong to AxSSH. HTTP/2 and
//! HTTP/3 prefixes are textual diagnostic conventions, not wire status lines.
//! Recognition emits only ranges/classes; the visible-row cache shares source Arcs.

use std::ops::Range;
use std::sync::LazyLock;

use regex::Regex;

#[cfg(any(feature = "egui-ui", test))]
use crate::terminal::TerminalSnapshot;
use crate::terminal::{TerminalColor, TerminalStyle, TerminalStyledLine};
use std::sync::Arc;

/// Presentation class; colors remain the responsibility of the frontend.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SemanticHighlight {
    Success,
    Info,
    Warning,
    Error,
}

const MAX_SEMANTIC_HIGHLIGHT_BYTES: usize = 512;

// A single matcher returns the capture to color rather than the surrounding
// context. ASCII word boundaries preserve terminal cell geometry. The access
// log rule recognizes the quoted request field used by Common/Combined logs;
// explicit HTTP fields include current and legacy OpenTelemetry spellings.
const RULE_PATTERN: &str = r#"(?ix-u)
    "[A-Z]+[\x20\t]+[\x20-\x21\x23-\x7e]+[\x20\t]+HTTP/(?:1\.[01]|[23](?:\.0)?)"
        [\x20\t]+(?<access_status>[1-5][0-9]{2})(?:[\x20\t]|$)
    | \bHTTP/(?:1\.[01]|[23](?:\.0)?)[\x20\t]+
        (?<response_status>[1-5][0-9]{2})(?:[\x20\t]|$)
    | \b(?:http_status(?:_code)?|http\.status_code|http\.response\.status_code)
        ["']?[\x20\t]*[=:][\x20\t]*["']?
        (?<field_status>[1-5][0-9]{2})(?:$|[\x20\t,;}\]"'])
    | \b(?<error>ERROR|FATAL|CRITICAL|ALERT|EMERGENCY)\b
    | \b(?<warning>WARN|WARNING)\b
    | \b(?<info>TRACE|DEBUG|INFO|NOTICE|INFORMATIONAL)\b
"#;

static RULES: LazyLock<Option<Regex>> = LazyLock::new(|| match Regex::new(RULE_PATTERN) {
    Ok(rules) => Some(rules),
    Err(error) => {
        tracing::error!(%error, "Terminal highlighting rules could not compile; semantic highlighting disabled");
        None
    }
});

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HighlightSpan {
    /// Byte range within one eligible run; one ASCII byte is one terminal cell.
    pub range: Range<usize>,
    /// Color class without any terminal contents.
    pub highlight: SemanticHighlight,
}

/// Compile once during startup, before a window or event loop is created.
pub fn prepare() {
    LazyLock::force(&RULES);
}

/// Return ordered, non-overlapping captures within an eligible ASCII run.
fn spans(text: &str) -> Vec<HighlightSpan> {
    let Some(rules) = RULES.as_ref() else {
        return Vec::new();
    };
    rules
        .captures_iter(text)
        .filter_map(|captures| {
            for (name, highlight) in [
                ("error", SemanticHighlight::Error),
                ("warning", SemanticHighlight::Warning),
                ("info", SemanticHighlight::Info),
            ] {
                if let Some(matched) = captures.name(name) {
                    return Some(HighlightSpan {
                        range: matched.range(),
                        highlight,
                    });
                }
            }
            let status = captures
                .name("access_status")
                .or_else(|| captures.name("response_status"))
                .or_else(|| captures.name("field_status"))?;
            let highlight = match status.as_str().as_bytes().first() {
                Some(b'1' | b'3') => SemanticHighlight::Info,
                Some(b'2') => SemanticHighlight::Success,
                Some(b'4') => SemanticHighlight::Warning,
                Some(b'5') => SemanticHighlight::Error,
                _ => return None,
            };
            Some(HighlightSpan {
                range: status.range(),
                highlight,
            })
        })
        .collect()
}

/// Recognize only bounded ASCII runs whose program styling can be preserved.
pub fn highlight_run(text: &str, cells: usize, style: TerminalStyle) -> Vec<HighlightSpan> {
    if !text.is_ascii()
        || text.is_empty()
        || text.len() > MAX_SEMANTIC_HIGHLIGHT_BYTES
        || cells != text.len()
        || style.foreground != TerminalColor::Default
        || style.background != TerminalColor::Default
        || style.inverse
        || style.dim
        || style.hidden
    {
        return Vec::new();
    }
    spans(text)
}

/// Recognition for one immutable visible source line, without copying its text.
pub struct SemanticLine {
    source: Arc<TerminalStyledLine>,
    runs: Vec<Vec<HighlightSpan>>,
}

impl SemanticLine {
    /// Reject annotations from a different source line, even with a reused revision.
    pub fn spans(&self, source: &Arc<TerminalStyledLine>, run: usize) -> &[HighlightSpan] {
        if !Arc::ptr_eq(&self.source, source) {
            return &[];
        }
        self.runs.get(run).map_or(&[], Vec::as_slice)
    }
}

#[cfg(any(feature = "egui-ui", test))]
#[derive(Default)]
pub(crate) struct Cache {
    lines: Vec<Arc<SemanticLine>>,
}

#[cfg(any(feature = "egui-ui", test))]
impl Cache {
    pub(crate) fn update(&mut self, snapshot: &TerminalSnapshot) -> Vec<Arc<SemanticLine>> {
        let previous = std::mem::take(&mut self.lines);
        self.lines = snapshot
            .lines
            .iter()
            .enumerate()
            .map(|(row, source)| {
                if let Some(cached) = previous
                    .get(row)
                    .filter(|cached| Arc::ptr_eq(&cached.source, source))
                {
                    return cached.clone();
                }
                Arc::new(SemanticLine {
                    source: source.clone(),
                    runs: source
                        .runs
                        .iter()
                        .map(|run| highlight_run(&run.text, run.cells, run.style))
                        .collect(),
                })
            })
            .collect();
        self.lines.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn matches(text: &str) -> Vec<(&str, SemanticHighlight)> {
        prepare();
        assert!(RULES.is_some(), "built-in regex rules must compile");
        spans(text)
            .into_iter()
            .map(|span| (&text[span.range], span.highlight))
            .collect()
    }

    #[test]
    fn recognizes_standard_log_severity_names_case_insensitively() {
        let text = "[trace] DEBUG Info NOTICE informational WARN warning ERROR fatal Critical ALERT emergency";
        assert_eq!(
            matches(text),
            vec![
                ("trace", SemanticHighlight::Info),
                ("DEBUG", SemanticHighlight::Info),
                ("Info", SemanticHighlight::Info),
                ("NOTICE", SemanticHighlight::Info),
                ("informational", SemanticHighlight::Info),
                ("WARN", SemanticHighlight::Warning),
                ("warning", SemanticHighlight::Warning),
                ("ERROR", SemanticHighlight::Error),
                ("fatal", SemanticHighlight::Error),
                ("Critical", SemanticHighlight::Error),
                ("ALERT", SemanticHighlight::Error),
                ("emergency", SemanticHighlight::Error),
            ]
        );
    }

    #[test]
    fn does_not_guess_severity_from_old_status_words_or_identifiers() {
        assert!(
            matches("terror error_code debug123 WARNINGS _INFO success OK connected pending ssh tls timed out timeout TODO FIXME failure traceback")
                .is_empty()
        );
    }

    #[test]
    fn recognizes_all_http_classes_in_response_summaries() {
        assert_eq!(
            matches("HTTP/1.1 100 Continue; HTTP/2 204 ; HTTP/3 302 ; HTTP/1.0 404 ; HTTP/2.0 503"),
            vec![
                ("100", SemanticHighlight::Info),
                ("204", SemanticHighlight::Success),
                ("302", SemanticHighlight::Info),
                ("404", SemanticHighlight::Warning),
                ("503", SemanticHighlight::Error),
            ]
        );
    }

    #[test]
    fn does_not_color_bare_numbers_ips_or_invalid_http_codes() {
        assert!(
            matches("200 302 404 503 count=200 pid=503 192.168.200.1 id200 HTTP/1.1 2000 HTTP/2 600 HTTP/3 099 HTTP/1.1 200.5 HTTP/2 404x status=200 http_status=2000 http_status=200.5")
                .is_empty()
        );
    }

    #[test]
    fn colors_only_the_status_field_in_common_and_combined_access_logs() {
        assert_eq!(
            matches(
                r#"192.168.200.1 - - [10/Oct/2026:23:00:00 +0800] "GET /200 HTTP/1.1" 404 503 "https://example.test/200" "agent""#
            ),
            vec![("404", SemanticHighlight::Warning)]
        );
        assert_eq!(
            matches(r#""PROPFIND /files HTTP/1.1" 207 200"#),
            vec![("207", SemanticHighlight::Success)]
        );
    }

    #[test]
    fn recognizes_explicit_http_attributes_and_preserves_their_context() {
        assert_eq!(
            matches(
                r#"{"http.response.status_code":200, "http.status_code": "404"} http_status=503 http_status_code: 302"#
            ),
            vec![
                ("200", SemanticHighlight::Success),
                ("404", SemanticHighlight::Warning),
                ("503", SemanticHighlight::Error),
                ("302", SemanticHighlight::Info),
            ]
        );
    }
}

#[cfg(test)]
mod cache_tests {
    use super::*;
    use crate::terminal::TerminalModel;

    #[test]
    fn eligibility_keeps_explicit_styles_unicode_and_oversize_runs_untouched() {
        for style in [
            TerminalStyle {
                foreground: TerminalColor::Indexed(1),
                ..Default::default()
            },
            TerminalStyle {
                background: TerminalColor::Indexed(1),
                ..Default::default()
            },
            TerminalStyle {
                inverse: true,
                ..Default::default()
            },
            TerminalStyle {
                dim: true,
                ..Default::default()
            },
            TerminalStyle {
                hidden: true,
                ..Default::default()
            },
        ] {
            assert!(highlight_run("ERROR HTTP/1.1 200", 18, style).is_empty());
        }
        let text = format!("{} ERROR", "x".repeat(506));
        assert_eq!(text.len(), 512);
        assert_eq!(highlight_run(&text, 512, TerminalStyle::default()).len(), 1);
        assert!(highlight_run(&(text + " "), 513, TerminalStyle::default()).is_empty());
        for (text, cells) in [("ERROR", 4), ("中文 ERROR", 9), ("é ERROR", 7)] {
            assert!(highlight_run(text, cells, TerminalStyle::default()).is_empty());
        }
    }

    #[test]
    fn cache_reuses_only_source_identity_and_releases_removed_visible_rows() {
        let mut model = TerminalModel::new(80, 4, 100);
        model.process(b"ERROR HTTP/1.1 200");
        let mut snapshot = model.snapshot();
        let mut cache = Cache::default();
        let first = cache.update(&snapshot);
        let reused = cache.update(&snapshot);
        assert!(Arc::ptr_eq(&first[0], &reused[0]));
        assert!(!first[0].spans(&snapshot.lines[0], 0).is_empty());
        let old = snapshot.lines[0].clone();
        snapshot.lines[0] = Arc::new((*old).clone());
        assert!(first[0].spans(&snapshot.lines[0], 0).is_empty());
        let replaced = cache.update(&snapshot);
        assert!(!Arc::ptr_eq(&first[0], &replaced[0]));
        let removed = Arc::downgrade(replaced.last().unwrap());
        snapshot.lines.truncate(1);
        cache.update(&snapshot);
        assert_eq!(cache.lines.len(), 1);
        drop(first);
        drop(reused);
        drop(replaced);
        assert!(removed.upgrade().is_none());
    }
}
