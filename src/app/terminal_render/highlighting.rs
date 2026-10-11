//! Regex recognition of log severity names and contextual HTTP status codes.
//!
//! Severity names follow OpenTelemetry and RFC 5424's syslog severity table.
//! HTTP classes follow RFC 9110; color choices belong to AxSSH. HTTP/2 and
//! HTTP/3 prefixes are textual diagnostic conventions, not wire status lines.
//! No terminal content or transport state is retained by this module.

use std::ops::Range;
use std::sync::LazyLock;

use regex::Regex;

use super::SemanticHighlight;

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

pub(super) struct HighlightSpan {
    pub(super) range: Range<usize>,
    pub(super) highlight: SemanticHighlight,
}

/// Compile once during startup, before a window or event loop is created.
pub(super) fn prepare() {
    LazyLock::force(&RULES);
}

/// Return ordered, non-overlapping captures within an eligible ASCII run.
pub(super) fn spans(text: &str) -> Vec<HighlightSpan> {
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
