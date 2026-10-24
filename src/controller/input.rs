//! Frontend-independent terminal pointer transaction encoding.

use crate::terminal::{
    TerminalKey, TerminalModel, TerminalModifiers, TerminalMouseButton, TerminalMouseEvent,
    TerminalMouseEventKind, encode_key,
};

/// Maximum wheel reports encoded in one atomic terminal input transaction.
pub const MAX_MOUSE_WHEEL_REPORTS: i32 = 256;

/// Encodes bounded pointer repeats using the model's current protocol modes.
///
/// Only wheel presses may repeat. The optional alternate-scroll policy appends
/// Home after each cursor step; ordinary mouse reporting is unchanged.
pub fn encode_terminal_mouse_input(
    model: &TerminalModel,
    event: TerminalMouseEvent,
    pixel_x: usize,
    pixel_y: usize,
    repeat_count: i32,
    alternate_scroll_to_home: bool,
) -> Option<Vec<u8>> {
    let repeat_count = repeat_count.clamp(1, MAX_MOUSE_WHEEL_REPORTS) as usize;
    let wheel = matches!(
        event.button,
        TerminalMouseButton::WheelUp
            | TerminalMouseButton::WheelDown
            | TerminalMouseButton::WheelLeft
            | TerminalMouseButton::WheelRight
    );
    if repeat_count > 1 && (event.kind != TerminalMouseEventKind::Press || !wheel) {
        return None;
    }
    let data = model.encode_mouse_event_with_pixels(event, pixel_x, pixel_y)?;
    if alternate_scroll_to_home
        && !model.mouse_reporting().enabled()
        && event.kind == TerminalMouseEventKind::Press
        && matches!(
            event.button,
            TerminalMouseButton::WheelUp | TerminalMouseButton::WheelDown
        )
    {
        let home = encode_key(
            &TerminalKey::Home,
            TerminalModifiers::default(),
            model.application_cursor(),
        )?;
        let mut repeated = Vec::with_capacity((home.len() + data.len()) * repeat_count);
        for _ in 0..repeat_count {
            repeated.extend_from_slice(&data);
            repeated.extend_from_slice(&home);
        }
        return Some(repeated);
    }
    Some(data.repeat(repeat_count))
}

#[cfg(test)]
mod tests;
