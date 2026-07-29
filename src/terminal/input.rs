//! Terminal key encoding independent from the Slint input event types.

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TerminalKey {
    Text(String),
    Return,
    Backspace,
    Tab,
    Escape,
    Up,
    Down,
    Right,
    Left,
    Insert,
    Delete,
    Home,
    End,
    PageUp,
    PageDown,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TerminalModifiers {
    pub alt: bool,
    pub control: bool,
    pub meta: bool,
    pub shift: bool,
}

impl TerminalModifiers {
    fn is_empty(self) -> bool {
        !self.alt && !self.control && !self.meta && !self.shift
    }

    fn xterm_code(self) -> u8 {
        1 + u8::from(self.shift) + 2 * u8::from(self.alt) + 4 * u8::from(self.control)
    }
}

pub fn encode_key(key: &TerminalKey, modifiers: TerminalModifiers) -> Option<Vec<u8>> {
    encode_key_for_platform(key, modifiers, TerminalPlatform::current())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TerminalPlatform {
    MacOs,
    Other,
}

impl TerminalPlatform {
    fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::MacOs
        } else {
            Self::Other
        }
    }
}

fn encode_key_for_platform(
    key: &TerminalKey,
    modifiers: TerminalModifiers,
    platform: TerminalPlatform,
) -> Option<Vec<u8>> {
    if let Some(sequence) = platform_navigation_sequence(key, modifiers, platform) {
        return Some(sequence.to_vec());
    }

    if modifiers.meta {
        return None;
    }

    let sequence = match key {
        TerminalKey::Return if modifiers.is_empty() => Some(b"\r".as_slice()),
        TerminalKey::Return if modifiers.shift && !modifiers.alt && !modifiers.control => {
            Some(b"\n".as_slice())
        }
        TerminalKey::Return if modifiers.alt && !modifiers.control && !modifiers.shift => {
            Some(b"\x1b\r".as_slice())
        }
        TerminalKey::Backspace
            if modifiers.is_empty()
                || (modifiers.shift && !modifiers.alt && !modifiers.control) =>
        {
            Some(b"\x7f".as_slice())
        }
        TerminalKey::Backspace if modifiers.control && !modifiers.alt => Some(b"\x08".as_slice()),
        TerminalKey::Backspace if modifiers.alt && !modifiers.control => {
            Some(b"\x1b\x7f".as_slice())
        }
        TerminalKey::Tab if modifiers.is_empty() => Some(b"\t".as_slice()),
        TerminalKey::Tab if modifiers.shift && !modifiers.alt && !modifiers.control => {
            Some(b"\x1b[Z".as_slice())
        }
        TerminalKey::Escape if modifiers.is_empty() => Some(b"\x1b".as_slice()),
        TerminalKey::Up if modifiers.is_empty() => Some(b"\x1b[A".as_slice()),
        TerminalKey::Down if modifiers.is_empty() => Some(b"\x1b[B".as_slice()),
        TerminalKey::Right if modifiers.is_empty() => Some(b"\x1b[C".as_slice()),
        TerminalKey::Left if modifiers.is_empty() => Some(b"\x1b[D".as_slice()),
        TerminalKey::Home if modifiers.is_empty() => Some(b"\x1b[H".as_slice()),
        TerminalKey::End if modifiers.is_empty() => Some(b"\x1b[F".as_slice()),
        TerminalKey::Insert if modifiers.is_empty() => Some(b"\x1b[2~".as_slice()),
        TerminalKey::Delete if modifiers.is_empty() => Some(b"\x1b[3~".as_slice()),
        TerminalKey::PageUp if modifiers.is_empty() => Some(b"\x1b[5~".as_slice()),
        TerminalKey::PageDown if modifiers.is_empty() => Some(b"\x1b[6~".as_slice()),
        _ => None,
    };
    if let Some(sequence) = sequence {
        return Some(sequence.to_vec());
    }

    if let Some(sequence) = modified_navigation_sequence(key, modifiers) {
        return Some(sequence.into_bytes());
    }

    let TerminalKey::Text(text) = key else {
        return None;
    };
    if modifiers.control {
        let mut encoded = encode_control_text(text)?;
        if modifiers.alt {
            encoded.insert(0, 0x1b);
        }
        return Some(encoded);
    }
    if modifiers.alt {
        let mut encoded = Vec::with_capacity(text.len() + 1);
        encoded.push(0x1b);
        encoded.extend_from_slice(text.as_bytes());
        return Some(encoded);
    }
    if text.is_empty() {
        None
    } else {
        Some(text.as_bytes().to_vec())
    }
}

fn platform_navigation_sequence(
    key: &TerminalKey,
    modifiers: TerminalModifiers,
    platform: TerminalPlatform,
) -> Option<&'static [u8]> {
    if platform != TerminalPlatform::MacOs || modifiers.control || modifiers.shift {
        return None;
    }
    match (key, modifiers.alt, modifiers.meta) {
        (TerminalKey::Left, false, true) => Some(b"\x01"),
        (TerminalKey::Right, false, true) => Some(b"\x05"),
        (TerminalKey::Left, true, false) => Some(b"\x1bb"),
        (TerminalKey::Right, true, false) => Some(b"\x1bf"),
        _ => None,
    }
}

fn modified_navigation_sequence(key: &TerminalKey, modifiers: TerminalModifiers) -> Option<String> {
    if modifiers.is_empty() || modifiers.meta {
        return None;
    }
    let code = modifiers.xterm_code();
    match key {
        TerminalKey::Up => Some(format!("\x1b[1;{code}A")),
        TerminalKey::Down => Some(format!("\x1b[1;{code}B")),
        TerminalKey::Right => Some(format!("\x1b[1;{code}C")),
        TerminalKey::Left => Some(format!("\x1b[1;{code}D")),
        TerminalKey::Home => Some(format!("\x1b[1;{code}H")),
        TerminalKey::End => Some(format!("\x1b[1;{code}F")),
        TerminalKey::Insert => Some(format!("\x1b[2;{code}~")),
        TerminalKey::Delete => Some(format!("\x1b[3;{code}~")),
        TerminalKey::PageUp => Some(format!("\x1b[5;{code}~")),
        TerminalKey::PageDown => Some(format!("\x1b[6;{code}~")),
        _ => None,
    }
}

fn encode_control_text(text: &str) -> Option<Vec<u8>> {
    let mut characters = text.chars();
    let character = characters.next()?;
    if characters.next().is_some() {
        return None;
    }
    let upper = character.to_ascii_uppercase();
    let byte = match upper {
        ' ' | '@' => 0x00,
        'A'..='Z' => upper as u8 - b'A' + 1,
        '[' => 0x1b,
        '\\' => 0x1c,
        ']' => 0x1d,
        '^' => 0x1e,
        '_' => 0x1f,
        '?' => 0x7f,
        _ => return None,
    };
    Some(vec![byte])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_text_return_backspace_and_navigation() {
        assert_eq!(
            encode_key(
                &TerminalKey::Text("ls".into()),
                TerminalModifiers::default()
            ),
            Some(b"ls".to_vec())
        );
        assert_eq!(
            encode_key(&TerminalKey::Return, TerminalModifiers::default()),
            Some(b"\r".to_vec())
        );
        assert_eq!(
            encode_key(&TerminalKey::Backspace, TerminalModifiers::default()),
            Some(b"\x7f".to_vec())
        );
        assert_eq!(
            encode_key(&TerminalKey::Up, TerminalModifiers::default()),
            Some(b"\x1b[A".to_vec())
        );
    }

    #[test]
    fn encodes_control_and_alt_text() {
        let control = TerminalModifiers {
            control: true,
            ..TerminalModifiers::default()
        };
        assert_eq!(
            encode_key(&TerminalKey::Text("c".into()), control),
            Some(vec![0x03])
        );
        assert_eq!(
            encode_key(&TerminalKey::Text("[".into()), control),
            Some(vec![0x1b])
        );
        let control_shift = TerminalModifiers {
            control: true,
            shift: true,
            ..TerminalModifiers::default()
        };
        assert_eq!(
            encode_key(&TerminalKey::Backspace, control_shift),
            Some(vec![0x08])
        );

        let alt = TerminalModifiers {
            alt: true,
            ..TerminalModifiers::default()
        };
        assert_eq!(
            encode_key(&TerminalKey::Text("b".into()), alt),
            Some(b"\x1bb".to_vec())
        );
    }

    #[test]
    fn encodes_xterm_modified_navigation() {
        let modifiers = TerminalModifiers {
            control: true,
            shift: true,
            ..TerminalModifiers::default()
        };
        assert_eq!(
            encode_key(&TerminalKey::Left, modifiers),
            Some(b"\x1b[1;6D".to_vec())
        );
        assert_eq!(
            encode_key(&TerminalKey::Delete, modifiers),
            Some(b"\x1b[3;6~".to_vec())
        );
    }

    #[test]
    fn keeps_macos_line_and_word_navigation_conventional() {
        let meta = TerminalModifiers {
            meta: true,
            ..TerminalModifiers::default()
        };
        assert_eq!(
            encode_key_for_platform(&TerminalKey::Left, meta, TerminalPlatform::MacOs),
            Some(vec![0x01])
        );
        let alt = TerminalModifiers {
            alt: true,
            ..TerminalModifiers::default()
        };
        assert_eq!(
            encode_key_for_platform(&TerminalKey::Right, alt, TerminalPlatform::MacOs),
            Some(b"\x1bf".to_vec())
        );
        assert_eq!(
            encode_key_for_platform(&TerminalKey::Right, alt, TerminalPlatform::Other),
            Some(b"\x1b[1;3C".to_vec())
        );
    }

    #[test]
    fn leaves_unhandled_meta_shortcuts_to_the_ui() {
        let meta = TerminalModifiers {
            meta: true,
            ..TerminalModifiers::default()
        };
        assert_eq!(encode_key(&TerminalKey::Text("c".into()), meta), None);
    }
}
