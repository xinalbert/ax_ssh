//! Font-independent terminal geometry in a 128-unit cell coordinate system.

use std::fmt::Write as _;

#[derive(Clone, Debug, Default, PartialEq)]
pub(in crate::app) struct GlyphPaths {
    pub(in crate::app) fill: String,
    pub(in crate::app) light: String,
    pub(in crate::app) heavy: String,
    pub(in crate::app) opacity: f32,
}

impl GlyphPaths {
    pub(super) fn is_empty(&self) -> bool {
        self.fill.is_empty() && self.light.is_empty() && self.heavy.is_empty()
    }

    pub(super) fn append(&mut self, character: char, column: usize) -> bool {
        let x = column * 128;
        if let Some(edges) = box_edges(character) {
            for (index, (end_x, end_y)) in [(0, 64), (128, 64), (64, 0), (64, 128)]
                .into_iter()
                .enumerate()
            {
                let weight = (edges >> (index * 2)) & 3;
                if weight == 0 {
                    continue;
                }
                let target = if weight == 2 {
                    &mut self.heavy
                } else {
                    &mut self.light
                };
                line(target, x as i32 + 64, 64, x as i32 + end_x, end_y);
            }
            self.opacity = 1.0;
            return true;
        }
        let mut rect = |left: usize, top: usize, width: usize, height: usize| {
            // Formatting into an owned String cannot fail.
            self.fill.push_str(&format!(
                "M {} {} h {} v {} h -{} Z ",
                x + left,
                top,
                width,
                height,
                width
            ));
        };
        match character as u32 {
            0x2580 => rect(0, 0, 128, 64),
            0x2581..=0x2588 => {
                let height = (character as usize - 0x2580) * 128 / 8;
                rect(0, 128 - height, 128, height);
            }
            0x2589..=0x258f => rect(0, 0, (0x2590 - character as usize) * 128 / 8, 128),
            0x2590 => rect(64, 0, 64, 128),
            0x2591..=0x2593 => rect(0, 0, 128, 128),
            0x2594 => rect(0, 0, 128, 128 / 8),
            0x2595 => rect(128 - 128 / 8, 0, 128 / 8, 128),
            0x2596..=0x259f => {
                let masks = [4_u8, 8, 1, 13, 9, 7, 11, 2, 6, 14];
                let mask = masks[character as usize - 0x2596];
                for bit in 0..4 {
                    if mask & (1 << bit) != 0 {
                        rect((bit % 2) * 64, (bit / 2) * 64, 64, 64);
                    }
                }
            }
            0xe0b0 => self
                .fill
                .push_str(&format!("M {x} 0 l 128 64 l -128 64 Z ")),
            0xe0b1 => self.light.push_str(&format!("M {x} 0 l 128 64 l -128 64 ")),
            0xe0b2 => self
                .fill
                .push_str(&format!("M {} 0 l -128 64 l 128 64 Z ", x + 128)),
            0xe0b3 => self
                .light
                .push_str(&format!("M {} 0 l -128 64 l 128 64 ", x + 128)),
            0xe0b4 => self.fill.push_str(&format!(
                "M {x} 0 Q {} 0 {} 64 Q {} 128 {x} 128 Z ",
                x + 128,
                x + 128,
                x + 128
            )),
            0xe0b5 => self.light.push_str(&format!(
                "M {x} 0 Q {} 0 {} 64 Q {} 128 {x} 128 ",
                x + 128,
                x + 128,
                x + 128
            )),
            0xe0b6 => self.fill.push_str(&format!(
                "M {} 0 Q {x} 0 {x} 64 Q {x} 128 {} 128 Z ",
                x + 128,
                x + 128
            )),
            0xe0b7 => self.light.push_str(&format!(
                "M {} 0 Q {x} 0 {x} 64 Q {x} 128 {} 128 ",
                x + 128,
                x + 128
            )),
            _ => return false,
        }
        self.opacity = match character {
            '░' => 0.25,
            '▒' => 0.5,
            '▓' => 0.75,
            _ => 1.0,
        };
        true
    }
}

fn line(target: &mut String, x: i32, y: i32, end_x: i32, end_y: i32) {
    // String's fmt::Write implementation cannot fail.
    let result = write!(target, "M {x} {y} L {end_x} {end_y} ");
    debug_assert!(result.is_ok());
}

fn box_edges(character: char) -> Option<u8> {
    // Two bits per edge: left, right, up, down. 1=light, 2=heavy.
    let edges = match character {
        '─' => [1, 1, 0, 0],
        '━' => [2, 2, 0, 0],
        '│' => [0, 0, 1, 1],
        '┃' => [0, 0, 2, 2],
        '┌' => [0, 1, 0, 1],
        '┍' => [0, 2, 0, 1],
        '┎' => [0, 1, 0, 2],
        '┏' => [0, 2, 0, 2],
        '┐' => [1, 0, 0, 1],
        '┑' => [2, 0, 0, 1],
        '┒' => [1, 0, 0, 2],
        '┓' => [2, 0, 0, 2],
        '└' => [0, 1, 1, 0],
        '┕' => [0, 2, 1, 0],
        '┖' => [0, 1, 2, 0],
        '┗' => [0, 2, 2, 0],
        '┘' => [1, 0, 1, 0],
        '┙' => [2, 0, 1, 0],
        '┚' => [1, 0, 2, 0],
        '┛' => [2, 0, 2, 0],
        '├' => [0, 1, 1, 1],
        '┤' => [1, 0, 1, 1],
        '┬' => [1, 1, 0, 1],
        '┴' => [1, 1, 1, 0],
        '┼' => [1, 1, 1, 1],
        '┣' => [0, 2, 2, 2],
        '┫' => [2, 0, 2, 2],
        '┳' => [2, 2, 0, 2],
        '┻' => [2, 2, 2, 0],
        '╋' => [2, 2, 2, 2],
        '╴' => [1, 0, 0, 0],
        '╵' => [0, 0, 1, 0],
        '╶' => [0, 1, 0, 0],
        '╷' => [0, 0, 0, 1],
        '╸' => [2, 0, 0, 0],
        '╹' => [0, 0, 2, 0],
        '╺' => [0, 2, 0, 0],
        '╻' => [0, 0, 0, 2],
        '╼' => [1, 2, 0, 0],
        '╽' => [0, 0, 1, 2],
        '╾' => [2, 1, 0, 0],
        '╿' => [0, 0, 2, 1],
        _ => return None,
    };
    Some(
        edges
            .into_iter()
            .enumerate()
            .fold(0, |bits, (index, edge)| bits | (edge << (index * 2))),
    )
}
