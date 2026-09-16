//! Bounded recognition of actionable terminal text.
//!
//! This module deliberately receives only a bounded visible logical-line
//! context. It must not retain terminal content, make network requests, or
//! inspect a worker-owned terminal buffer.

use ax_ssh::terminal::TerminalTargetContext;

const MAX_TARGET_LINE_CHARS: usize = 2_048;
const MAX_TARGET_CHARS: usize = 1_024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum TerminalTarget {
    Url(String),
    RemotePath(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct TerminalTargetSpan {
    pub(super) target: TerminalTarget,
    pub(super) start: usize,
    pub(super) end: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct TerminalTargetSegment {
    pub(super) row: usize,
    pub(super) start: usize,
    pub(super) end: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct TerminalTargetMatch {
    pub(super) target: TerminalTarget,
    pub(super) segments: Vec<TerminalTargetSegment>,
}

pub(super) fn terminal_target_span_at_cell(
    text: &str,
    column: usize,
) -> Option<TerminalTargetSpan> {
    if text.chars().any(char::is_control) {
        return None;
    }
    let characters = text.chars().collect::<Vec<_>>();
    if characters.len() > MAX_TARGET_LINE_CHARS || column >= characters.len() {
        return None;
    }

    let (start, end) = token_bounds(&characters, column)?;
    let token = characters[start..end].iter().collect::<String>();

    find_url_target(&token, start, column)
        .or_else(|| find_remote_path_target(&token, start, column))
}

pub(super) fn terminal_target_match_at_context(
    context: &TerminalTargetContext,
) -> Option<TerminalTargetMatch> {
    if context.rows.is_empty() {
        return None;
    }

    let mut text = String::new();
    let mut clicked_row_present = false;
    let mut row_offsets = Vec::with_capacity(context.rows.len());
    for row in &context.rows {
        let start = text.chars().count();
        text.push_str(&row.text);
        let end = text.chars().count();
        row_offsets.push((row.row, start, end));
        clicked_row_present |= row.row == context.clicked_row;
    }

    if !clicked_row_present || context.clicked_character >= text.chars().count() {
        return None;
    }
    let span = terminal_target_span_at_cell(&text, context.clicked_character)?;
    let total_characters = text.chars().count();
    if (context.starts_mid_logical_line && span.start == 0)
        || (context.ends_mid_logical_line && span.end == total_characters)
    {
        return None;
    }

    let mut segments = Vec::new();
    for (row, row_start, row_end) in row_offsets {
        let segment_start = span.start.max(row_start).saturating_sub(row_start);
        let segment_end = span.end.min(row_end).saturating_sub(row_start);
        if segment_start < segment_end {
            segments.push(TerminalTargetSegment {
                row,
                start: segment_start,
                end: segment_end,
            });
        }
    }
    (!segments.is_empty()).then_some(TerminalTargetMatch {
        target: span.target,
        segments,
    })
}

fn token_bounds(characters: &[char], column: usize) -> Option<(usize, usize)> {
    if is_token_boundary(characters[column]) {
        return None;
    }
    let mut start = column;
    while start > 0 && !is_token_boundary(characters[start - 1]) {
        start -= 1;
    }
    let mut end = column + 1;
    while end < characters.len() && !is_token_boundary(characters[end]) {
        end += 1;
    }
    Some((start, end))
}

fn is_token_boundary(character: char) -> bool {
    character.is_whitespace()
        || matches!(
            character,
            '\'' | '"' | '<' | '>' | '`' | '|' | '(' | '[' | '{'
        )
}

fn find_url_target(token: &str, token_start: usize, column: usize) -> Option<TerminalTargetSpan> {
    for prefix in ["https://", "http://"] {
        let mut offset = 0;
        while let Some(found) = token[offset..].find(prefix) {
            let start_byte = offset + found;
            let candidate = trim_terminal_punctuation(&token[start_byte..]);
            let candidate_chars = candidate.chars().count();
            let start_column = token_start + token[..start_byte].chars().count();
            let end_column = start_column + candidate_chars;
            if column >= start_column
                && column < end_column
                && valid_web_url(candidate)
                && candidate_chars <= MAX_TARGET_CHARS
            {
                return Some(TerminalTargetSpan {
                    target: TerminalTarget::Url(candidate.to_owned()),
                    start: start_column,
                    end: end_column,
                });
            }
            offset = start_byte + prefix.len();
        }
    }
    None
}

fn valid_web_url(candidate: &str) -> bool {
    let Some((scheme, authority_and_path)) = candidate.split_once("://") else {
        return false;
    };
    if !matches!(scheme, "http" | "https") {
        return false;
    }
    let authority = authority_and_path
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default();
    !authority.is_empty()
        && !authority.starts_with('.')
        && authority
            .chars()
            .all(|character| !character.is_control() && !character.is_whitespace())
}

fn find_remote_path_target(
    token: &str,
    token_start: usize,
    column: usize,
) -> Option<TerminalTargetSpan> {
    let candidate = trim_location_suffix(trim_terminal_punctuation(token));
    let candidate_chars = candidate.chars().count();
    if candidate_chars == 0
        || candidate_chars > MAX_TARGET_CHARS
        || column < token_start
        || column >= token_start + candidate_chars
        || !matches_remote_path(candidate)
    {
        return None;
    }
    Some(TerminalTargetSpan {
        target: TerminalTarget::RemotePath(candidate.to_owned()),
        start: token_start,
        end: token_start + candidate_chars,
    })
}

fn matches_remote_path(candidate: &str) -> bool {
    (candidate.starts_with('/') || candidate.starts_with("./") || candidate.starts_with("../"))
        && !candidate.chars().any(char::is_control)
}

fn trim_location_suffix(candidate: &str) -> &str {
    let Some((before_column, column)) = candidate.rsplit_once(':') else {
        return candidate;
    };
    if !is_decimal(column) {
        return candidate;
    }
    match before_column.rsplit_once(':') {
        Some((path, line)) if is_decimal(line) => path,
        _ => before_column,
    }
}

fn is_decimal(value: &str) -> bool {
    !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit())
}

fn trim_terminal_punctuation(mut candidate: &str) -> &str {
    while let Some(last) = candidate.chars().last() {
        let remove = matches!(last, '.' | ',' | ';' | ':' | '!')
            || (last == ')' && unmatched_closing(candidate, '(', ')'))
            || (last == ']' && unmatched_closing(candidate, '[', ']'))
            || (last == '}' && unmatched_closing(candidate, '{', '}'));
        if !remove {
            break;
        }
        candidate = &candidate[..candidate.len() - last.len_utf8()];
    }
    candidate
}

fn unmatched_closing(candidate: &str, opening: char, closing: char) -> bool {
    candidate
        .chars()
        .filter(|character| *character == closing)
        .count()
        > candidate
            .chars()
            .filter(|character| *character == opening)
            .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifies_web_urls_and_omits_terminal_punctuation() {
        assert_eq!(
            terminal_target_span_at_cell("See https://example.test/releases/v1.2).", 16)
                .map(|span| span.target),
            Some(TerminalTarget::Url(
                "https://example.test/releases/v1.2".to_owned()
            ))
        );
        assert_eq!(
            terminal_target_span_at_cell("See https://example.test/releases/v1.2).", 16),
            Some(TerminalTargetSpan {
                target: TerminalTarget::Url("https://example.test/releases/v1.2".to_owned()),
                start: 4,
                end: 38,
            })
        );
    }

    #[test]
    fn identifies_absolute_and_relative_remote_paths() {
        assert_eq!(
            terminal_target_span_at_cell("open /srv/app/src/main.rs:42:7", 16)
                .map(|span| span.target),
            Some(TerminalTarget::RemotePath(
                "/srv/app/src/main.rs".to_owned()
            ))
        );
        assert_eq!(
            terminal_target_span_at_cell("open ../logs/service.log", 10).map(|span| span.target),
            Some(TerminalTarget::RemotePath("../logs/service.log".to_owned()))
        );
        assert_eq!(
            terminal_target_span_at_cell("open ./build/output", 8).map(|span| span.target),
            Some(TerminalTarget::RemotePath("./build/output".to_owned()))
        );
    }

    #[test]
    fn rejects_non_target_text_and_clicks_on_trimmed_suffixes() {
        assert_eq!(terminal_target_span_at_cell("https://", 4), None);
        assert_eq!(terminal_target_span_at_cell("relative/file.txt", 4), None);
        assert_eq!(terminal_target_span_at_cell("/srv/app.rs:42", 12), None);
    }

    #[test]
    fn bounds_and_controls_prevent_untrusted_terminal_text_from_being_parsed() {
        let long = format!("/{}", "a".repeat(MAX_TARGET_CHARS + 1));
        assert_eq!(terminal_target_span_at_cell(&long, 1), None);
        assert_eq!(terminal_target_span_at_cell("/srv/\u{1b}[31mfile", 2), None);
    }

    #[test]
    fn target_span_excludes_location_suffix_and_trailing_punctuation() {
        assert_eq!(
            terminal_target_span_at_cell("open /srv/app/src/main.rs:42:7", 16),
            Some(TerminalTargetSpan {
                target: TerminalTarget::RemotePath("/srv/app/src/main.rs".to_owned()),
                start: 5,
                end: 25,
            })
        );
    }

    #[test]
    fn joins_soft_wrapped_url_and_maps_each_physical_row() {
        let context = TerminalTargetContext {
            rows: vec![
                ax_ssh::terminal::TerminalTargetRow {
                    row: 0,
                    text: "https://example.test/very-long/".to_owned(),
                },
                ax_ssh::terminal::TerminalTargetRow {
                    row: 1,
                    text: "path?q=1".to_owned(),
                },
            ],
            clicked_row: 1,
            clicked_character: 33,
            starts_mid_logical_line: false,
            ends_mid_logical_line: false,
        };

        let target = terminal_target_match_at_context(&context).expect("wrapped URL");
        assert_eq!(
            target.target,
            TerminalTarget::Url("https://example.test/very-long/path?q=1".to_owned())
        );
        assert_eq!(
            target.segments,
            vec![
                TerminalTargetSegment {
                    row: 0,
                    start: 0,
                    end: 31,
                },
                TerminalTargetSegment {
                    row: 1,
                    start: 0,
                    end: 8,
                },
            ]
        );
    }

    #[test]
    fn refuses_target_truncated_by_viewport_edge() {
        let context = TerminalTargetContext {
            rows: vec![ax_ssh::terminal::TerminalTargetRow {
                row: 0,
                text: "https://example.test/partial".to_owned(),
            }],
            clicked_row: 0,
            clicked_character: 10,
            starts_mid_logical_line: false,
            ends_mid_logical_line: true,
        };
        assert_eq!(terminal_target_match_at_context(&context), None);
    }
}
