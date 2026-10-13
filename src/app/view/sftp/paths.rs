//! Bounded path presentation, independent of the host filesystem.

const MAX_PATH_BYTES: usize = 4096;
const MAX_SEGMENTS: usize = 6;

#[derive(Clone, Copy)]
pub(super) enum PathStyle {
    Posix,
    Windows,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct Breadcrumb {
    pub(super) label: String,
    pub(super) path: String,
    pub(super) current: bool,
}

pub(super) fn breadcrumbs(path: &str, style: PathStyle) -> Vec<Breadcrumb> {
    if path.is_empty() || path.len() > MAX_PATH_BYTES || path.chars().any(char::is_control) {
        return Vec::new();
    }
    let separator =
        |character| character == '/' || matches!(style, PathStyle::Windows) && character == '\\';
    let mut spans = Vec::new();
    let mut start = None;
    for (index, character) in path.char_indices() {
        if separator(character) {
            if let Some(start) = start.take() {
                spans.push((start, index));
            }
        } else {
            start.get_or_insert(index);
        }
    }
    if let Some(start) = start {
        spans.push((start, path.len()));
    }
    let Some((root_end, first_component, label)) = root(path, style, &spans) else {
        // Relative or unsupported device paths stay editable; their prefixes
        // are not absolute navigation targets and must not be guessed.
        return Vec::new();
    };
    let mut rows = vec![Breadcrumb {
        label,
        path: path[..root_end].to_owned(),
        current: spans.len() == first_component,
    }];
    let mut components = &spans[first_component..];
    if components.len() + 1 > MAX_SEGMENTS {
        let tail_start = components.len() - (MAX_SEGMENTS - 2);
        rows.push(Breadcrumb {
            label: "…".into(),
            path: path[..components[tail_start - 1].1].to_owned(),
            current: false,
        });
        components = &components[tail_start..];
    }
    for &(start, end) in components {
        rows.push(Breadcrumb {
            label: path[start..end].to_owned(),
            path: path[..end].to_owned(),
            current: end == spans.last().map_or(0, |span| span.1),
        });
    }
    rows
}

fn root(path: &str, style: PathStyle, spans: &[(usize, usize)]) -> Option<(usize, usize, String)> {
    let separator = |byte| byte == b'/' || byte == b'\\';
    if matches!(style, PathStyle::Posix) {
        return path.starts_with('/').then(|| {
            let end = path.bytes().take_while(|byte| *byte == b'/').count();
            (end, 0, path[..end].to_owned())
        });
    }
    let bytes = path.as_bytes();
    let drive = |name: &str| {
        name.len() == 2 && name.as_bytes()[0].is_ascii_alphabetic() && name.ends_with(':')
    };
    if bytes.first().is_some_and(|byte| separator(*byte))
        && bytes.get(1).is_some_and(|byte| separator(*byte))
    {
        let name = |index: usize| spans.get(index).map(|&(start, end)| &path[start..end]);
        let (count, label) = match (name(0), name(1)) {
            (Some("?"), Some(prefix)) if prefix.eq_ignore_ascii_case("UNC") => {
                (4, format!("\\\\{}\\{}", name(2)?, name(3)?))
            }
            (Some("?"), Some(volume)) if drive(volume) => (2, format!("{volume}\\")),
            (Some("?" | "."), _) => return None,
            (Some(server), Some(share)) => (2, format!("\\\\{server}\\{share}")),
            _ => return None,
        };
        let end = spans.get(count - 1)?.1;
        let end = end + usize::from(bytes.get(end).is_some_and(|byte| separator(*byte)));
        return Some((end, count, label));
    }
    if let Some(&(start, end)) = spans.first()
        && start == 0
        && drive(&path[start..end])
        && bytes.get(end).is_some_and(|byte| separator(*byte))
    {
        return Some((end + 1, 1, path[..end + 1].to_owned()));
    }
    bytes
        .first()
        .filter(|byte| separator(**byte))
        .map(|_| (1, 0, path[..1].to_owned()))
}

#[cfg(test)]
mod tests;
