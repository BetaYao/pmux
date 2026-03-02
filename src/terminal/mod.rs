// terminal/mod.rs - Terminal stream adapters for gpui-terminal
pub mod box_drawing;
pub mod colors;
pub mod content_extractor;
pub mod stream_adapter;
pub mod terminal_core;
pub mod terminal_rendering;
pub mod terminal_element;
pub mod input;

pub use colors::ColorPalette;
pub use input::key_to_bytes;
pub use terminal_element::TerminalElement;
pub use content_extractor::ContentExtractor;
pub use stream_adapter::{RuntimeReader, RuntimeWriter, tee_output};
pub use terminal_core::{Terminal, TerminalSize};
