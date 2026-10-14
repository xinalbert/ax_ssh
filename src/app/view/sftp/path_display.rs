//! Display-only middle elision. Navigation always uses the original path.

const MAX_DISPLAY_CHARS: usize = 4096;

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
