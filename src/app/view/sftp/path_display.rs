//! Stateless path presentation. Filesystem state retains the original path.

const MAX_DISPLAY_CHARS: usize = 4096;

pub(in crate::app) fn local_path(path: &str) -> String {
    let Some(verbatim) = path.strip_prefix(r"\\?\") else {
        return path.to_owned();
    };
    let bytes = verbatim.as_bytes();
    if bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1..3] == *b":\\"
        && ordinary_windows_components(&verbatim[3..])
    {
        return verbatim.to_owned();
    }
    if verbatim
        .get(..4)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("UNC\\"))
    {
        let unc = &verbatim[4..];
        let mut components = unc.split('\\');
        if components.next().is_some_and(|server| !server.is_empty())
            && components.next().is_some_and(|share| !share.is_empty())
            && ordinary_windows_components(unc)
        {
            return format!(r"\\{unc}");
        }
    }
    // Removing the prefix can change how Win32 interprets special names,
    // trailing dots/spaces, or device/volume namespaces. Keep those verbatim.
    path.to_owned()
}

fn ordinary_windows_components(path: &str) -> bool {
    if path.is_empty() {
        return true;
    }
    path.strip_suffix('\\')
        .unwrap_or(path)
        .split('\\')
        .all(|part| {
            if part.is_empty()
                || part.ends_with(['.', ' '])
                || part
                    .chars()
                    .any(|ch| ch.is_control() || "/:*?\"<>|".contains(ch))
            {
                return false;
            }
            let stem = part
                .split('.')
                .next()
                .unwrap_or_default()
                .trim_end_matches(' ');
            !matches!(
                stem.to_ascii_uppercase().as_str(),
                "CON"
                    | "PRN"
                    | "AUX"
                    | "NUL"
                    | "CONIN$"
                    | "CONOUT$"
                    | "COM1"
                    | "COM2"
                    | "COM3"
                    | "COM4"
                    | "COM5"
                    | "COM6"
                    | "COM7"
                    | "COM8"
                    | "COM9"
                    | "LPT1"
                    | "LPT2"
                    | "LPT3"
                    | "LPT4"
                    | "LPT5"
                    | "LPT6"
                    | "LPT7"
                    | "LPT8"
                    | "LPT9"
                    | "COM¹"
                    | "COM²"
                    | "COM³"
                    | "LPT¹"
                    | "LPT²"
                    | "LPT³"
            )
        })
}

pub(in crate::app) fn elide(path: &str, budget: i32) -> String {
    let budget = usize::try_from(budget)
        .unwrap_or_default()
        .min(MAX_DISPLAY_CHARS);
    if budget == 0 {
        return String::new();
    }
    if path.chars().take(budget + 1).count() <= budget {
        return path.to_owned();
    }
    let retained = budget - 1;
    // Reserve roughly three quarters for the tail, while retaining the start
    // whenever the available budget can show both ends and the ellipsis.
    let head = if retained >= 2 {
        (retained / 4).max(1)
    } else {
        0
    };
    let tail = retained - head;
    let start: String = path.chars().take(head).collect();
    let end: String = path.chars().rev().take(tail).collect();
    format!("{start}…{}", end.chars().rev().collect::<String>())
}

#[cfg(test)]
mod tests;
