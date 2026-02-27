//! input - Keyboard and mouse input handling for terminal forwarding
//!
//! Converts GPUI key events to xterm escape sequences for PTY write.

mod xterm_escape;

pub use xterm_escape::{key_to_xterm_escape, KeyModifiers};
