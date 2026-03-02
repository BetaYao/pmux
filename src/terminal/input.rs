//! Terminal keyboard input: converts GPUI key events to terminal escape sequences.

use alacritty_terminal::term::TermMode;
use gpui::KeyDownEvent;

/// Convert a GPUI KeyDownEvent to terminal bytes.
///
/// Returns None for text-producing keystrokes that will be handled
/// by the InputHandler (IME path). Returns Some(bytes) for control
/// sequences, special keys, and modifier combos.
pub fn key_to_bytes(event: &KeyDownEvent, mode: TermMode) -> Option<Vec<u8>> {
    let keystroke = &event.keystroke;
    let mods = &keystroke.modifiers;
    let app_cursor = mode.contains(TermMode::APP_CURSOR);

    // Ctrl+letter → control character (0x01–0x1A)
    if mods.control && !mods.shift && !mods.alt && !mods.platform {
        let key = keystroke.key.as_str();
        if let Some(c) = key.chars().next() {
            if key.len() == 1 && c.is_ascii_alphabetic() {
                return Some(vec![(c.to_ascii_lowercase() as u8) - b'a' + 1]);
            }
        }
        // Ctrl+[ = ESC
        if key == "[" {
            return Some(b"\x1b".to_vec());
        }
    }

    // Tab / Shift+Tab
    match keystroke.key.as_str() {
        "tab" => {
            return if mods.shift {
                Some(b"\x1b[Z".to_vec())
            } else {
                Some(b"\t".to_vec())
            };
        }
        "enter" | "return" | "kp_enter" => {
            return if mods.shift {
                Some(b"\n".to_vec())
            } else {
                Some(b"\r".to_vec())
            };
        }
        _ => {}
    }

    // macOS Cmd+Arrow: line navigation
    #[cfg(target_os = "macos")]
    if mods.platform && !mods.alt && !mods.control {
        match keystroke.key.as_str() {
            "left" => return Some(vec![0x01]), // Ctrl+A (start of line)
            "right" => return Some(vec![0x05]), // Ctrl+E (end of line)
            "up" => return Some(b"\x1b[1;5A".to_vec()),
            "down" => return Some(b"\x1b[1;5B".to_vec()),
            "backspace" => return Some(vec![0x15]), // Ctrl+U (kill line)
            _ => {}
        }
    }

    // macOS Option+Arrow: word navigation
    #[cfg(target_os = "macos")]
    if mods.alt && !mods.platform && !mods.control {
        match keystroke.key.as_str() {
            "left" => return Some(b"\x1bb".to_vec()),
            "right" => return Some(b"\x1bf".to_vec()),
            "backspace" => return Some(vec![0x17]), // Ctrl+W
            _ => {}
        }
    }

    // Modifier code for CSI sequences: 1=none, 2=Shift, 3=Alt, 4=Shift+Alt, 5=Ctrl, ...
    let modifier_code = 1
        + (if mods.shift { 1 } else { 0 })
        + (if mods.alt { 2 } else { 0 })
        + (if mods.control { 4 } else { 0 });

    // Arrow keys
    match keystroke.key.as_str() {
        "up" | "down" | "right" | "left" => {
            let ch = match keystroke.key.as_str() {
                "up" => 'A',
                "down" => 'B',
                "right" => 'C',
                "left" => 'D',
                _ => unreachable!(),
            };
            if modifier_code > 1 {
                return Some(format!("\x1b[1;{}{}", modifier_code, ch).into_bytes());
            }
            return if app_cursor {
                Some(format!("\x1bO{}", ch).into_bytes())
            } else {
                Some(format!("\x1b[{}", ch).into_bytes())
            };
        }
        _ => {}
    }

    // Text-producing keystrokes go via InputHandler (IME path)
    if keystroke.key_char.is_some() {
        return None;
    }

    // Special keys
    match keystroke.key.as_str() {
        "backspace" => Some(b"\x7f".to_vec()),
        "escape" => Some(b"\x1b".to_vec()),
        "home" => {
            if modifier_code > 1 {
                Some(format!("\x1b[1;{}H", modifier_code).into_bytes())
            } else {
                Some(b"\x1b[H".to_vec())
            }
        }
        "end" => {
            if modifier_code > 1 {
                Some(format!("\x1b[1;{}F", modifier_code).into_bytes())
            } else {
                Some(b"\x1b[F".to_vec())
            }
        }
        "pageup" => Some(b"\x1b[5~".to_vec()),
        "pagedown" => Some(b"\x1b[6~".to_vec()),
        "delete" => Some(b"\x1b[3~".to_vec()),
        "f1" => Some(b"\x1bOP".to_vec()),
        "f2" => Some(b"\x1bOQ".to_vec()),
        "f3" => Some(b"\x1bOR".to_vec()),
        "f4" => Some(b"\x1bOS".to_vec()),
        "f5" => Some(b"\x1b[15~".to_vec()),
        "f6" => Some(b"\x1b[17~".to_vec()),
        "f7" => Some(b"\x1b[18~".to_vec()),
        "f8" => Some(b"\x1b[19~".to_vec()),
        "f9" => Some(b"\x1b[20~".to_vec()),
        "f10" => Some(b"\x1b[21~".to_vec()),
        "f11" => Some(b"\x1b[23~".to_vec()),
        "f12" => Some(b"\x1b[24~".to_vec()),
        key => {
            if key.len() == 1 {
                Some(key.as_bytes().to_vec())
            } else {
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alacritty_terminal::term::TermMode;
    use gpui::Keystroke;

    fn empty_mode() -> TermMode {
        TermMode::empty()
    }
    fn app_cursor_mode() -> TermMode {
        TermMode::APP_CURSOR
    }

    fn make_event(key: &str, ctrl: bool, shift: bool, alt: bool) -> KeyDownEvent {
        KeyDownEvent {
            keystroke: Keystroke {
                key: key.into(),
                key_char: None,
                modifiers: gpui::Modifiers {
                    control: ctrl,
                    shift,
                    alt,
                    platform: false,
                    function: false,
                },
                ime_key: None,
            },
            is_held: false,
        }
    }

    #[test]
    fn test_ctrl_c() {
        let ev = make_event("c", true, false, false);
        assert_eq!(key_to_bytes(&ev, empty_mode()), Some(vec![3]));
    }

    #[test]
    fn test_enter() {
        let ev = make_event("enter", false, false, false);
        assert_eq!(key_to_bytes(&ev, empty_mode()), Some(b"\r".to_vec()));
    }

    #[test]
    fn test_arrow_normal_mode() {
        let ev = make_event("up", false, false, false);
        assert_eq!(key_to_bytes(&ev, empty_mode()), Some(b"\x1b[A".to_vec()));
    }

    #[test]
    fn test_arrow_app_cursor_mode() {
        let ev = make_event("up", false, false, false);
        assert_eq!(key_to_bytes(&ev, app_cursor_mode()), Some(b"\x1bOA".to_vec()));
    }

    #[test]
    fn test_backspace() {
        let ev = make_event("backspace", false, false, false);
        assert_eq!(key_to_bytes(&ev, empty_mode()), Some(b"\x7f".to_vec()));
    }

    #[test]
    fn test_tab() {
        let ev = make_event("tab", false, false, false);
        assert_eq!(key_to_bytes(&ev, empty_mode()), Some(b"\t".to_vec()));
        let shift_ev = make_event("tab", false, true, false);
        assert_eq!(key_to_bytes(&shift_ev, empty_mode()), Some(b"\x1b[Z".to_vec()));
    }

    #[test]
    fn test_key_char_returns_none() {
        let mut ev = make_event("a", false, false, false);
        ev.keystroke.key_char = Some('a');
        assert_eq!(key_to_bytes(&ev, empty_mode()), None);
    }
}
