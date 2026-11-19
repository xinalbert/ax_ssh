// Copyright © SixtyFPS GmbH <info@slint.dev>
// SPDX-License-Identifier: GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0

// cSpell: ignore xdir ydir
use winit::window::{CursorIcon, ResizeDirection};

pub fn handle_cursor_move_for_resize(
    window: &winit::window::Window,
    position: winit::dpi::PhysicalPosition<f64>,
    current_direction: Option<ResizeDirection>,
    border_size: f64,
) -> Option<ResizeDirection> {
    if !window.is_decorated() && window.is_resizable() {
        // A maximized or fullscreen window must not advertise an edge resize
        // on Windows: starting one there can leave winit stuck in a dragging
        // state (winit#4821). Report no direction, which clears a stale
        // direction and restores the default cursor below.
        let location = get_resize_direction(
            window.inner_size(),
            position,
            border_size,
            window_edge_resize_allowed(window),
        );

        if current_direction != location {
            window.set_cursor(resize_direction_cursor_icon(location));
        }

        return location;
    }

    None
}

pub fn handle_resize(
    window: &winit::window::Window,
    direction: &std::cell::Cell<Option<ResizeDirection>>,
) -> bool {
    // Re-read window state at press time: a cached hover direction may predate
    // a maximize/fullscreen transition, even without another CursorMoved.
    start_resize_on_press(
        direction,
        window_edge_resize_allowed(window),
        |cursor| window.set_cursor(cursor),
        |dir| {
            let _ = window.drag_resize_window(dir);
        },
    )
}

fn window_edge_resize_allowed(window: &winit::window::Window) -> bool {
    // Do not query or change non-Windows native state on this workaround path.
    !cfg!(windows)
        || (!window.is_decorated()
            && window.is_resizable()
            && edge_resize_allowed(true, window.is_maximized(), window.fullscreen().is_some()))
}

fn start_resize_on_press(
    direction: &std::cell::Cell<Option<ResizeDirection>>,
    allowed: bool,
    set_cursor: impl FnOnce(CursorIcon),
    start_resize: impl FnOnce(ResizeDirection),
) -> bool {
    if !allowed {
        if direction.replace(None).is_some() {
            set_cursor(CursorIcon::Default);
        }
        return false;
    }
    if let Some(dir) = direction.get() {
        start_resize(dir);
        true
    } else {
        false
    }
}

/// Get the cursor icon that corresponds to the resize direction.
fn resize_direction_cursor_icon(resize_direction: Option<ResizeDirection>) -> CursorIcon {
    match resize_direction {
        Some(resize_direction) => match resize_direction {
            ResizeDirection::East => CursorIcon::EResize,
            ResizeDirection::North => CursorIcon::NResize,
            ResizeDirection::NorthEast => CursorIcon::NeResize,
            ResizeDirection::NorthWest => CursorIcon::NwResize,
            ResizeDirection::South => CursorIcon::SResize,
            ResizeDirection::SouthEast => CursorIcon::SeResize,
            ResizeDirection::SouthWest => CursorIcon::SwResize,
            ResizeDirection::West => CursorIcon::WResize,
        },
        None => CursorIcon::Default,
    }
}

/// Decide whether an edge resize may be advertised (hover cursor) or started
/// (press). Windows can leave `drag_resize_window` stuck in a dragging state
/// when a resize is started on a maximized or fullscreen window
/// (winit#4821), so there the edge resize is blocked in that state. The
/// Windows native state is checked at both hover and press time; other
/// platforms keep their previous behavior.
fn edge_resize_allowed(
    blocks_edge_resize_when_full: bool,
    maximized: bool,
    fullscreen: bool,
) -> bool {
    !(blocks_edge_resize_when_full && (maximized || fullscreen))
}

fn get_resize_direction(
    win_size: winit::dpi::PhysicalSize<u32>,
    position: winit::dpi::PhysicalPosition<f64>,
    border_size: f64,
    allowed: bool,
) -> Option<ResizeDirection> {
    if !allowed {
        return None;
    }
    enum X {
        West,
        East,
        Default,
    }

    enum Y {
        North,
        South,
        Default,
    }

    let xdir = if position.x < border_size {
        X::West
    } else if position.x > (win_size.width as f64 - border_size) {
        X::East
    } else {
        X::Default
    };

    let ydir = if position.y < border_size {
        Y::North
    } else if position.y > (win_size.height as f64 - border_size) {
        Y::South
    } else {
        Y::Default
    };

    Some(match (xdir, ydir) {
        (X::West, Y::North) => ResizeDirection::NorthWest,
        (X::West, Y::South) => ResizeDirection::SouthWest,
        (X::West, Y::Default) => ResizeDirection::West,

        (X::East, Y::North) => ResizeDirection::NorthEast,
        (X::East, Y::South) => ResizeDirection::SouthEast,
        (X::East, Y::Default) => ResizeDirection::East,

        (X::Default, Y::North) => ResizeDirection::North,
        (X::Default, Y::South) => ResizeDirection::South,
        (X::Default, Y::Default) => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_blocks_edge_resize_when_maximized_or_fullscreen() {
        assert!(!edge_resize_allowed(true, true, false));
        assert!(!edge_resize_allowed(true, false, true));
        assert!(!edge_resize_allowed(true, true, true));
        // A restored window can still be edge-resized on Windows.
        assert!(edge_resize_allowed(true, false, false));
    }

    #[test]
    fn non_windows_keeps_edge_resize_when_maximized_or_fullscreen() {
        assert!(edge_resize_allowed(false, true, false));
        assert!(edge_resize_allowed(false, false, true));
        assert!(edge_resize_allowed(false, true, true));
        assert!(edge_resize_allowed(false, false, false));
    }

    #[test]
    fn blocked_hover_direction_restores_default_cursor() {
        // When the hover policy blocks, the direction becomes None and the
        // None mapping restores the default cursor (clearing a stale
        // resize cursor).
        let location = get_resize_direction(
            winit::dpi::PhysicalSize::new(800, 600),
            winit::dpi::PhysicalPosition::new(799.0, 1.0),
            6.0,
            edge_resize_allowed(true, true, false),
        );
        assert_eq!(location, None);
        assert_eq!(resize_direction_cursor_icon(None), CursorIcon::Default);
        // Regression: an active direction still maps to a resize cursor.
        assert_eq!(
            resize_direction_cursor_icon(Some(ResizeDirection::NorthEast)),
            CursorIcon::NeResize
        );
    }

    #[test]
    fn stale_resize_press_does_not_start_or_consume_after_maximize() {
        use std::cell::Cell;

        let direction = Cell::new(Some(ResizeDirection::NorthEast));
        let cursor = Cell::new(CursorIcon::NeResize);
        let starts = Cell::new(0);
        let consumed = start_resize_on_press(
            &direction,
            edge_resize_allowed(true, true, false),
            |icon| cursor.set(icon),
            |_| starts.set(starts.get() + 1),
        );
        assert!(!consumed, "a blocked resize must leave the press for Slint");
        assert_eq!(starts.get(), 0, "must not enter winit's dragging state");
        assert_eq!(direction.get(), None, "clear the pre-maximize direction");
        assert_eq!(cursor.get(), CursorIcon::Default);
    }

    #[test]
    fn restored_window_can_resize_again_after_a_blocked_press() {
        use std::cell::Cell;

        let direction = Cell::new(Some(ResizeDirection::East));
        for fullscreen in [false, true] {
            direction.set(Some(ResizeDirection::East));
            assert!(!start_resize_on_press(
                &direction,
                edge_resize_allowed(true, !fullscreen, fullscreen),
                |_| {},
                |_| panic!("blocked window must not initiate native resize"),
            ));
            assert_eq!(direction.get(), None);
        }
        // Restore and hover an edge again; no process-local latch is retained.
        direction.set(get_resize_direction(
            winit::dpi::PhysicalSize::new(800, 600),
            winit::dpi::PhysicalPosition::new(799.0, 300.0),
            6.0,
            edge_resize_allowed(true, false, false),
        ));
        let started = Cell::new(None);
        assert!(start_resize_on_press(
            &direction,
            true,
            |_| panic!("normal resize must retain its cursor"),
            |dir| started.set(Some(dir)),
        ));
        assert_eq!(started.get(), Some(ResizeDirection::East));
    }

    #[test]
    fn press_without_resize_direction_is_not_consumed() {
        assert!(!start_resize_on_press(
            &std::cell::Cell::new(None),
            true,
            |_| panic!("ordinary press must not change the cursor"),
            |_| panic!("ordinary press must not initiate native resize"),
        ));
    }

    #[test]
    fn restored_edges_and_corners_keep_their_resize_directions() {
        for (x, y, expected) in [
            (1.0, 1.0, ResizeDirection::NorthWest),
            (400.0, 1.0, ResizeDirection::North),
            (799.0, 1.0, ResizeDirection::NorthEast),
            (1.0, 300.0, ResizeDirection::West),
            (799.0, 300.0, ResizeDirection::East),
            (1.0, 599.0, ResizeDirection::SouthWest),
            (400.0, 599.0, ResizeDirection::South),
            (799.0, 599.0, ResizeDirection::SouthEast),
        ] {
            assert_eq!(
                get_resize_direction(
                    winit::dpi::PhysicalSize::new(800, 600),
                    winit::dpi::PhysicalPosition::new(x, y),
                    6.0,
                    true,
                ),
                Some(expected),
            );
        }
    }
}
