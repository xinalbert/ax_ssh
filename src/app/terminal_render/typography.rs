//! Terminal presentation decisions; source cells and their Unicode stay intact.

use super::{RenderedTerminalRun, geometry::GlyphPaths};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::app) enum FontRole {
    #[default]
    Primary,
    Symbol,
    Emoji,
}

pub(super) fn font_role(text: &str) -> FontRole {
    if text.contains('\u{fe0f}') || text.contains('\u{20e3}') {
        FontRole::Emoji
    } else if text.contains('\u{fe0e}') {
        FontRole::Symbol
    } else if text.chars().any(default_emoji)
        || (text.contains('\u{200d}') && text.chars().any(is_symbol))
    {
        FontRole::Emoji
    } else if text.chars().next().is_some_and(is_symbol) {
        FontRole::Symbol
    } else {
        FontRole::Primary
    }
}

// BMP Emoji_Presentation characters must keep their default color presentation;
// text-default symbols (including U+23FA) use the explicit symbol family.
fn default_emoji(character: char) -> bool {
    matches!(character as u32,
        0x231a..=0x231b | 0x23e9..=0x23ec | 0x23f0 | 0x23f3
        | 0x25fd..=0x25fe | 0x2614..=0x2615 | 0x2648..=0x2653
        | 0x267f | 0x2693 | 0x26a1 | 0x26aa..=0x26ab
        | 0x26bd..=0x26be | 0x26c4..=0x26c5 | 0x26ce | 0x26d4
        | 0x26ea | 0x26f2..=0x26f3 | 0x26f5 | 0x26fa | 0x26fd
        | 0x2705 | 0x270a..=0x270b | 0x2728 | 0x274c | 0x274e
        | 0x2753..=0x2755 | 0x2757 | 0x2795..=0x2797 | 0x27b0
        | 0x27bf | 0x2b1b..=0x2b1c | 0x2b50 | 0x2b55
        | 0x1f000..=0x1faff)
}

fn is_symbol(character: char) -> bool {
    matches!(character as u32, 0x2190..=0x23ff | 0x2500..=0x27bf | 0x2b00..=0x2bff)
}

pub(super) fn prepare(runs: Vec<RenderedTerminalRun>) -> Vec<RenderedTerminalRun> {
    let mut result: Vec<RenderedTerminalRun> = Vec::with_capacity(runs.len());
    for mut run in runs {
        run.font_role = font_role(&run.text);
        let characters = run.text.chars().count();
        // A wide base plus a zero-width mark can also have cells == scalars.
        // Only the domain's known box-line batches may be split into cells;
        // every other multi-scalar composition must remain intact.
        let simple = run.cells == characters
            && (characters == 1
                || run
                    .text
                    .chars()
                    .all(|ch| matches!(ch as u32, 0x2500..=0x257f)));
        if !simple || run.font_role == FontRole::Emoji || run.text.is_ascii() {
            if let Some(previous) = result.last_mut()
                && previous.font_role == FontRole::Emoji
                && run.font_role == FontRole::Emoji
                && same_style(previous, &run)
                && (previous.text.ends_with('\u{200d}')
                    || run
                        .text
                        .chars()
                        .next()
                        .is_some_and(|ch| matches!(ch as u32, 0x1f3fb..=0x1f3ff))
                    || (single_regional_indicator(&previous.text)
                        && single_regional_indicator(&run.text)))
            {
                previous.text.push_str(&run.text);
                previous.cells += run.cells;
                continue;
            }
            result.push(run);
            continue;
        }
        // The style template must not clone a whole box-line string per cell.
        let text = std::mem::take(&mut run.text);
        for (index, character) in text.chars().enumerate() {
            let mut cell = run.clone();
            cell.text = character.to_string();
            cell.column += index;
            cell.cells = 1;
            cell.font_role = font_role(&cell.text);
            cell.fixed_spacing = fixed_symbol(character);
            cell.geometry = GlyphPaths::default();
            cell.geometry.append(character, 0);
            if let Some(previous) = result.last_mut()
                && can_append(previous, &cell)
            {
                if !cell.geometry.is_empty() {
                    previous.geometry.append(character, previous.cells);
                }
                previous.text.push(character);
                previous.cells += 1;
            } else {
                result.push(cell);
            }
        }
    }
    result
}

fn can_append(left: &RenderedTerminalRun, right: &RenderedTerminalRun) -> bool {
    let geometry = !left.geometry.is_empty()
        && !right.geometry.is_empty()
        && left.geometry.opacity == right.geometry.opacity;
    let symbols = left.geometry.is_empty()
        && right.geometry.is_empty()
        && left.fixed_spacing
        && right.fixed_spacing;
    (geometry || symbols) && same_style(left, right)
}

fn single_regional_indicator(text: &str) -> bool {
    let mut characters = text.chars();
    characters
        .next()
        .is_some_and(|ch| matches!(ch as u32, 0x1f1e6..=0x1f1ff))
        && characters.next().is_none()
}

fn same_style(left: &RenderedTerminalRun, right: &RenderedTerminalRun) -> bool {
    left.column + left.cells == right.column
        && left.foreground == right.foreground
        && left.background == right.background
        && left.decoration_foreground == right.decoration_foreground
        && left.bold == right.bold
        && left.italic == right.italic
        && left.underline == right.underline
        && left.underline_style == right.underline_style
        && left.strikethrough == right.strikethrough
        && left.hyperlink == right.hyperlink
        && left.font_role == right.font_role
}

// Only symbols with the same advance as M in all four bundled Iosevka faces
// can share a fixed-spacing shaping run. Coverage/advances are checked against
// the real fonts, not inferred from a Unicode block or a monospace flag.
pub(super) fn fixed_symbol(character: char) -> bool {
    let point = character as u32;
    FIXED_SYMBOL_RANGES
        .iter()
        .any(|&(start, end)| (start..=end).contains(&point))
}

const FIXED_SYMBOL_RANGES: &[(u32, u32)] = &[
    (0x2190, 0x21aa),
    (0x21ad, 0x21ae),
    (0x21b0, 0x21d9),
    (0x21de, 0x21e9),
    (0x21f3, 0x21f3),
    (0x21f5, 0x21f5),
    (0x21f7, 0x21fc),
    (0x2200, 0x22d3),
    (0x22d5, 0x22d7),
    (0x22da, 0x22f1),
    (0x22f6, 0x22f8),
    (0x22fd, 0x22ff),
    (0x2302, 0x2302),
    (0x2305, 0x2306),
    (0x2308, 0x230b),
    (0x2310, 0x2310),
    (0x231c, 0x2321),
    (0x2329, 0x232a),
    (0x2336, 0x237a),
    (0x2388, 0x238a),
    (0x2395, 0x2395),
    (0x239b, 0x23ae),
    (0x23b0, 0x23b1),
    (0x23cf, 0x23cf),
    (0x23f4, 0x23fe),
    (0x2500, 0x2600),
    (0x2605, 0x2606),
    (0x2610, 0x2612),
    (0x2630, 0x2637),
    (0x2639, 0x263c),
    (0x2640, 0x2642),
    (0x2660, 0x2667),
    (0x2669, 0x266f),
    (0x2680, 0x2687),
    (0x268a, 0x2691),
    (0x2699, 0x2699),
    (0x269b, 0x269b),
    (0x26a1, 0x26a1),
    (0x26aa, 0x26ab),
    (0x2713, 0x2718),
    (0x271a, 0x271a),
    (0x2731, 0x2731),
    (0x2733, 0x2734),
    (0x2736, 0x273a),
    (0x274c, 0x274c),
    (0x2764, 0x2764),
    (0x276c, 0x2771),
    (0x2776, 0x277f),
    (0x2794, 0x2794),
    (0x27a1, 0x27a1),
    (0x27a4, 0x27a6),
    (0x2b00, 0x2b0d),
    (0x2b12, 0x2b2f),
    (0x2b50, 0x2b55),
    (0x2b58, 0x2b58),
    (0x2b95, 0x2b95),
    (0x2bc0, 0x2bc8),
    (0x2bca, 0x2bcb),
    (0x2be8, 0x2beb),
];
