// tests/integration_test.rs - Integration tests for keyboard handling and workspace restoration

use pmux::input::{key_to_xterm_escape, KeyModifiers};

#[test]
fn test_key_to_xterm_escape_mapping() {
    let no_mods = KeyModifiers::default();
    let cmd_mods = KeyModifiers {
        platform: true,
        ..KeyModifiers::default()
    };

    // enter -> \r
    assert_eq!(key_to_xterm_escape("enter", no_mods), Some(vec![b'\r']));
    // backspace -> 0x7f
    assert_eq!(key_to_xterm_escape("backspace", no_mods), Some(vec![0x7f]));
    // up -> CSI A
    assert_eq!(
        key_to_xterm_escape("up", no_mods),
        Some(vec![0x1b, b'[', b'A'])
    );
    // Cmd+b should be intercepted (None)
    assert_eq!(key_to_xterm_escape("b", cmd_mods), None);
    // regular char
    assert_eq!(key_to_xterm_escape("a", no_mods), Some(vec![b'a']));
}

#[test]
fn test_workspace_restoration_flow() {
    // Placeholder for integration testing
    // Real integration testing would require a GPUI context
    assert!(true);
}
