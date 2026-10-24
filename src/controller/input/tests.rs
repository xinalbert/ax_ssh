use super::*;
use crate::terminal::TerminalMouseModifiers;

fn wheel_event(button: TerminalMouseButton) -> TerminalMouseEvent {
    TerminalMouseEvent {
        kind: TerminalMouseEventKind::Press,
        button,
        column: 0,
        row: 0,
        modifiers: TerminalMouseModifiers::default(),
    }
}

#[test]
fn alternate_scroll_to_home_appends_home_after_each_bounded_wheel_step() {
    let mut model = TerminalModel::new(80, 24, 10);
    model.process(b"\x1b[?1049h");
    for (mode, up, down, home) in [
        (b"\x1b[?1l", b"\x1b[A", b"\x1b[B", b"\x1b[H"),
        (b"\x1b[?1h", b"\x1bOA", b"\x1bOB", b"\x1bOH"),
    ] {
        model.process(mode);
        for (button, direction) in [
            (TerminalMouseButton::WheelUp, up),
            (TerminalMouseButton::WheelDown, down),
        ] {
            let event = wheel_event(button);
            for (count, expected_count) in [(0, 1), (1, 1), (3, 3), (i32::MAX, 256)] {
                let arrows = direction.repeat(expected_count);
                assert_eq!(
                    encode_terminal_mouse_input(&model, event, 1, 1, count, false),
                    Some(arrows.clone())
                );
                let mut expected = Vec::new();
                for _ in 0..expected_count {
                    expected.extend(direction);
                    expected.extend(home);
                }
                assert_eq!(
                    encode_terminal_mouse_input(&model, event, 1, 1, count, true),
                    Some(expected)
                );
            }
        }
    }
}

#[test]
fn alternate_scroll_to_home_preserves_mouse_reporting_and_other_events() {
    let mut model = TerminalModel::new(80, 24, 10);
    model.process(b"\x1b[?1049h\x1b[?1h\x1b[?1000h\x1b[?1006h");
    let event = wheel_event(TerminalMouseButton::WheelUp);
    assert_eq!(
        encode_terminal_mouse_input(&model, event, 1, 1, 3, true),
        Some(b"\x1b[<64;1;1M".repeat(3))
    );
    for button in [
        TerminalMouseButton::WheelDown,
        TerminalMouseButton::WheelLeft,
        TerminalMouseButton::WheelRight,
        TerminalMouseButton::Left,
    ] {
        let event = wheel_event(button);
        assert_eq!(
            encode_terminal_mouse_input(&model, event, 1, 1, 1, true),
            model.encode_mouse_event(event)
        );
    }
    assert_eq!(
        encode_terminal_mouse_input(
            &model,
            wheel_event(TerminalMouseButton::Left),
            1,
            1,
            2,
            true
        ),
        None
    );
    model.process(b"\x1b[?1000l");
    for button in [TerminalMouseButton::WheelLeft, TerminalMouseButton::Left] {
        assert_eq!(
            encode_terminal_mouse_input(&model, wheel_event(button), 1, 1, 1, true),
            None
        );
    }
    model.process(b"\x1b[?1007l");
    assert_eq!(
        encode_terminal_mouse_input(&model, event, 1, 1, 1, true),
        None
    );
    model.process(b"\x1b[?1007h\x1b[?1049l");
    assert_eq!(
        encode_terminal_mouse_input(&model, event, 1, 1, 1, true),
        None
    );
}
