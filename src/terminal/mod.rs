// terminal/mod.rs - Terminal layer for pmux
pub mod box_drawing;
pub mod colors;
pub mod content_extractor;
pub mod terminal_core;
pub mod terminal_rendering;
pub mod terminal_element;
pub mod input;
pub mod terminal_input_handler;

pub use colors::ColorPalette;
pub use input::key_to_bytes;
pub use terminal_element::TerminalElement;
pub use content_extractor::ContentExtractor;
pub use terminal_core::{DetectedLink, SearchMatch, Terminal, TerminalSize};

pub use alacritty_terminal::selection::SelectionType;
pub use alacritty_terminal::index::Side;
pub use input::{sgr_mouse_press, sgr_mouse_release, sgr_mouse_motion, sgr_mouse_scroll};
