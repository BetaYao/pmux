// terminal/mod.rs - Terminal layer for pmux
pub mod content_extractor;

pub use content_extractor::{ContentExtractor, extract_last_line, extract_last_line_filtered};

// gpui-ghostty re-exports (terminal engine)
pub use gpui_ghostty_terminal::{TerminalConfig, TerminalSession};
pub use gpui_ghostty_terminal::view::{TerminalInput, TerminalView as GhosttyTerminalView};
