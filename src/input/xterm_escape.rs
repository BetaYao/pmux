//! xterm_escape.rs - Convert GPUI keys to xterm escape sequences
//!
//! Returns raw bytes for PTY write. Used by Runtime.send_input.

/// Modifiers for key events
#[derive(Clone, Copy, Debug, Default)]
pub struct KeyModifiers {
    pub platform: bool,  // Cmd on Mac, Win on Windows
    pub shift: bool,
    pub alt: bool,
    pub ctrl: bool,
}

/// Convert a GPUI key name + modifiers to xterm escape sequence bytes.
/// Returns None if the key should be handled by pmux (app shortcut, e.g. Cmd+key).
/// Returns Some(bytes) for keys to forward to the terminal.
pub fn key_to_xterm_escape(key: &str, modifiers: KeyModifiers) -> Option<Vec<u8>> {
    // pmux shortcuts: Cmd+key — intercept, don't forward
    if modifiers.platform {
        return None;
    }

    let bytes = match key {
        "enter" | "return" => vec![b'\r'],
        "backspace" => vec![0x7f],
        "escape" => vec![0x1b],
        "space" | "Space" => vec![b' '],
        "tab" => {
            if modifiers.shift {
                vec![0x1b, b'[', b'Z']
            } else {
                vec![b'\t']
            }
        }
        "up" => escape_csi(modifiers, b'A'),
        "down" => escape_csi(modifiers, b'B'),
        "right" => escape_csi(modifiers, b'C'),
        "left" => escape_csi(modifiers, b'D'),
        "home" => escape_csi_special(modifiers, b'H'),
        "end" => escape_csi_special(modifiers, b'F'),
        "pageup" | "page_up" => vec![0x1b, b'[', b'5', b'~'],
        "pagedown" | "page_down" => vec![0x1b, b'[', b'6', b'~'],
        "delete" => vec![0x1b, b'[', b'3', b'~'],
        "insert" => vec![0x1b, b'[', b'2', b'~'],
        "f1" => vec![0x1b, b'O', b'P'],
        "f2" => vec![0x1b, b'O', b'Q'],
        "f3" => vec![0x1b, b'O', b'R'],
        "f4" => vec![0x1b, b'O', b'S'],
        "f5" => vec![0x1b, b'[', b'1', b'5', b'~'],
        "f6" => vec![0x1b, b'[', b'1', b'7', b'~'],
        "f7" => vec![0x1b, b'[', b'1', b'8', b'~'],
        "f8" => vec![0x1b, b'[', b'1', b'9', b'~'],
        "f9" => vec![0x1b, b'[', b'2', b'0', b'~'],
        "f10" => vec![0x1b, b'[', b'2', b'1', b'~'],
        "f11" => vec![0x1b, b'[', b'2', b'3', b'~'],
        "f12" => vec![0x1b, b'[', b'2', b'4', b'~'],
        other => {
            if modifiers.ctrl && other.len() == 1 {
                if let Some(c) = other.chars().next() {
                    let code = c as u32;
                    if code >= 0x40 && code <= 0x5f {
                        return Some(vec![(code as u8) - 0x40]);
                    }
                    if code >= 0x61 && code <= 0x7a {
                        return Some(vec![(code as u8) - 0x60]);
                    }
                    if c == '@' {
                        return Some(vec![0]);
                    }
                    if c == '[' {
                        return Some(vec![0x1b]);
                    }
                    if c == '\\' {
                        return Some(vec![0x1c]);
                    }
                    if c == ']' {
                        return Some(vec![0x1d]);
                    }
                    if c == '^' {
                        return Some(vec![0x1e]);
                    }
                    if c == '_' {
                        return Some(vec![0x1f]);
                    }
                }
            }
            if modifiers.alt && other.len() == 1 {
                if let Some(c) = other.chars().next() {
                    let s = c.to_string();
                    let mut bytes = vec![0x1b];
                    bytes.extend(s.into_bytes());
                    return Some(bytes);
                }
            }
            if other.len() == 1 {
                return Some(other.as_bytes().to_vec());
            }
            other.as_bytes().to_vec()
        }
    };
    Some(bytes)
}

fn escape_csi(modifiers: KeyModifiers, last: u8) -> Vec<u8> {
    let param = modifier_param(modifiers);
    if param == 1 {
        vec![0x1b, b'[', last]
    } else {
        vec![0x1b, b'[', b'1', b';', b'0' + param, last]
    }
}

fn escape_csi_special(modifiers: KeyModifiers, last: u8) -> Vec<u8> {
    let param = modifier_param(modifiers);
    if param == 1 {
        vec![0x1b, b'[', last]
    } else {
        vec![0x1b, b'[', b'1', b';', b'0' + param, last]
    }
}

fn modifier_param(m: KeyModifiers) -> u8 {
    match (m.shift, m.alt, m.ctrl) {
        (false, false, false) => 1,
        (true, false, false) => 2,
        (false, true, false) => 3,
        (true, true, false) => 4,
        (false, false, true) => 5,
        (true, false, true) => 6,
        (false, true, true) => 7,
        (true, true, true) => 8,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_mods() -> KeyModifiers {
        KeyModifiers::default()
    }

    #[test]
    fn test_enter() {
        let b = key_to_xterm_escape("enter", no_mods()).unwrap();
        assert_eq!(b, vec![b'\r']);
    }

    #[test]
    fn test_backspace() {
        let b = key_to_xterm_escape("backspace", no_mods()).unwrap();
        assert_eq!(b, vec![0x7f]);
    }

    #[test]
    fn test_escape() {
        let b = key_to_xterm_escape("escape", no_mods()).unwrap();
        assert_eq!(b, vec![0x1b]);
    }

    #[test]
    fn test_tab() {
        let b = key_to_xterm_escape("tab", no_mods()).unwrap();
        assert_eq!(b, vec![b'\t']);
    }

    #[test]
    fn test_arrow_keys() {
        assert_eq!(key_to_xterm_escape("up", no_mods()).unwrap(), vec![0x1b, b'[', b'A']);
        assert_eq!(key_to_xterm_escape("down", no_mods()).unwrap(), vec![0x1b, b'[', b'B']);
        assert_eq!(key_to_xterm_escape("left", no_mods()).unwrap(), vec![0x1b, b'[', b'D']);
        assert_eq!(key_to_xterm_escape("right", no_mods()).unwrap(), vec![0x1b, b'[', b'C']);
    }

    #[test]
    fn test_home_end() {
        assert_eq!(key_to_xterm_escape("home", no_mods()).unwrap(), vec![0x1b, b'[', b'H']);
        assert_eq!(key_to_xterm_escape("end", no_mods()).unwrap(), vec![0x1b, b'[', b'F']);
    }

    #[test]
    fn test_f1_f4() {
        assert_eq!(key_to_xterm_escape("f1", no_mods()).unwrap(), vec![0x1b, b'O', b'P']);
        assert_eq!(key_to_xterm_escape("f4", no_mods()).unwrap(), vec![0x1b, b'O', b'S']);
    }

    #[test]
    fn test_ctrl_a() {
        let m = KeyModifiers { ctrl: true, ..no_mods() };
        assert_eq!(key_to_xterm_escape("a", m).unwrap(), vec![0x01]);
    }

    #[test]
    fn test_cmd_intercepted() {
        let m = KeyModifiers { platform: true, ..no_mods() };
        assert!(key_to_xterm_escape("b", m).is_none());
        assert!(key_to_xterm_escape("a", m).is_none());
    }

    #[test]
    fn test_regular_char() {
        assert_eq!(key_to_xterm_escape("a", no_mods()).unwrap(), vec![b'a']);
        assert_eq!(key_to_xterm_escape("z", no_mods()).unwrap(), vec![b'z']);
    }
}
