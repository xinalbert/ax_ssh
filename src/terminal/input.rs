//! Terminal key encoding independent from the Slint input event types.

use crate::terminal_input::TERMINAL_PASTE_MAX_BYTES;

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
    Function(u8),
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

pub fn encode_key(
    key: &TerminalKey,
    modifiers: TerminalModifiers,
    application_cursor: bool,
) -> Option<Vec<u8>> {
    encode_key_for_platform(
        key,
        modifiers,
        application_cursor,
        TerminalPlatform::current(),
    )
}

/// Encode a clipboard paste as one bounded terminal input transaction.
///
/// DEC bracketed paste keeps the wrapper around the complete payload, rather
/// than around individual transport chunks. Newlines are normalized to CR and
/// ESC is removed before the bytes cross the worker boundary.
pub fn encode_paste(text: &str, bracketed: bool) -> Option<Vec<u8>> {
    let mut normalized = String::with_capacity(text.len());
    let mut characters = text.chars().peekable();
    while let Some(character) = characters.next() {
        match character {
            '\x1b' => {}
            '\r' => {
                normalized.push('\r');
                if characters.peek() == Some(&'\n') {
                    characters.next();
                }
            }
            '\n' => normalized.push('\r'),
            character => normalized.push(character),
        }
    }
    if normalized.is_empty() || normalized.len() > TERMINAL_PASTE_MAX_BYTES {
        return None;
    }

    let wrapper_bytes = if bracketed { 12 } else { 0 };
    let total_bytes = normalized.len().checked_add(wrapper_bytes)?;
    if total_bytes > TERMINAL_PASTE_MAX_BYTES {
        return None;
    }

    let mut bytes = Vec::with_capacity(total_bytes);
    if bracketed {
        bytes.extend_from_slice(b"\x1b[200~");
    }
    bytes.extend_from_slice(normalized.as_bytes());
    if bracketed {
        bytes.extend_from_slice(b"\x1b[201~");
    }
    Some(bytes)
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
    application_cursor: bool,
    platform: TerminalPlatform,
) -> Option<Vec<u8>> {
    if let Some(sequence) = platform_navigation_sequence(key, modifiers, platform) {
        return Some(sequence.to_vec());
    }

    if modifiers.meta {
        return None;
    }

    if let Some(sequence) = function_key_sequence(key, modifiers) {
        return Some(sequence);
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
        TerminalKey::Up if modifiers.is_empty() && application_cursor => Some(b"\x1bOA".as_slice()),
        TerminalKey::Down if modifiers.is_empty() && application_cursor => {
            Some(b"\x1bOB".as_slice())
        }
        TerminalKey::Right if modifiers.is_empty() && application_cursor => {
            Some(b"\x1bOC".as_slice())
        }
        TerminalKey::Left if modifiers.is_empty() && application_cursor => {
            Some(b"\x1bOD".as_slice())
        }
        TerminalKey::Up if modifiers.is_empty() => Some(b"\x1b[A".as_slice()),
        TerminalKey::Down if modifiers.is_empty() => Some(b"\x1b[B".as_slice()),
        TerminalKey::Right if modifiers.is_empty() => Some(b"\x1b[C".as_slice()),
        TerminalKey::Left if modifiers.is_empty() => Some(b"\x1b[D".as_slice()),
        TerminalKey::Home if modifiers.is_empty() && application_cursor => {
            Some(b"\x1bOH".as_slice())
        }
        TerminalKey::End if modifiers.is_empty() && application_cursor => {
            Some(b"\x1bOF".as_slice())
        }
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
    if is_bare_modifier_text(text, modifiers) {
        return None;
    }
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

fn is_bare_modifier_text(text: &str, modifiers: TerminalModifiers) -> bool {
    if modifiers.control {
        return false;
    }
    let mut characters = text.chars();
    let Some(character) = characters.next() else {
        return false;
    };
    characters.next().is_none() && ('\u{0010}'..='\u{0018}').contains(&character)
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

fn function_key_sequence(key: &TerminalKey, modifiers: TerminalModifiers) -> Option<Vec<u8>> {
    let TerminalKey::Function(number) = key else {
        return None;
    };
    let unmodified = match number {
        1 => Some(b"\x1bOP".as_slice()),
        2 => Some(b"\x1bOQ".as_slice()),
        3 => Some(b"\x1bOR".as_slice()),
        4 => Some(b"\x1bOS".as_slice()),
        5 => Some(b"\x1b[15~".as_slice()),
        6 => Some(b"\x1b[17~".as_slice()),
        7 => Some(b"\x1b[18~".as_slice()),
        8 => Some(b"\x1b[19~".as_slice()),
        9 => Some(b"\x1b[20~".as_slice()),
        10 => Some(b"\x1b[21~".as_slice()),
        11 => Some(b"\x1b[23~".as_slice()),
        12 => Some(b"\x1b[24~".as_slice()),
        // xterm-256color's kf13-kf24 entries. F13-F16 are the
        // shift-modified F1-F4 forms; F17-F24 use the extended tilde
        // numbers rather than reusing the F1-F4 forms.
        13 => Some(b"\x1b[1;2P".as_slice()),
        14 => Some(b"\x1b[1;2Q".as_slice()),
        15 => Some(b"\x1b[1;2R".as_slice()),
        16 => Some(b"\x1b[1;2S".as_slice()),
        17 => Some(b"\x1b[15;2~".as_slice()),
        18 => Some(b"\x1b[17;2~".as_slice()),
        19 => Some(b"\x1b[18;2~".as_slice()),
        20 => Some(b"\x1b[19;2~".as_slice()),
        21 => Some(b"\x1b[20;2~".as_slice()),
        22 => Some(b"\x1b[21;2~".as_slice()),
        23 => Some(b"\x1b[23;2~".as_slice()),
        24 => Some(b"\x1b[24;2~".as_slice()),
        _ => None,
    };
    if modifiers.is_empty() {
        return unmodified.map(|sequence| sequence.to_vec());
    }

    let code = modifiers.xterm_code();
    let sequence = match number {
        1 => format!("\x1b[1;{code}P"),
        2 => format!("\x1b[1;{code}Q"),
        3 => format!("\x1b[1;{code}R"),
        4 => format!("\x1b[1;{code}S"),
        5 => format!("\x1b[15;{code}~"),
        6 => format!("\x1b[17;{code}~"),
        7 => format!("\x1b[18;{code}~"),
        8 => format!("\x1b[19;{code}~"),
        9 => format!("\x1b[20;{code}~"),
        10 => format!("\x1b[21;{code}~"),
        11 => format!("\x1b[23;{code}~"),
        12 => format!("\x1b[24;{code}~"),
        13 => format!("\x1b[1;{code}P"),
        14 => format!("\x1b[1;{code}Q"),
        15 => format!("\x1b[1;{code}R"),
        16 => format!("\x1b[1;{code}S"),
        17 => format!("\x1b[15;{code}~"),
        18 => format!("\x1b[17;{code}~"),
        19 => format!("\x1b[18;{code}~"),
        20 => format!("\x1b[19;{code}~"),
        21 => format!("\x1b[20;{code}~"),
        22 => format!("\x1b[21;{code}~"),
        23 => format!("\x1b[23;{code}~"),
        24 => format!("\x1b[24;{code}~"),
        _ => return None,
    };
    Some(sequence.into_bytes())
}

fn encode_control_text(text: &str) -> Option<Vec<u8>> {
    let mut characters = text.chars();
    let character = characters.next()?;
    if characters.next().is_some() {
        return None;
    }
    if ('\u{0001}'..='\u{001f}').contains(&character) || character == '\u{007f}' {
        return Some(vec![character as u8]);
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
mod tests;
