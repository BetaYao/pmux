// terminal/mod.rs - Terminal stream adapters for gpui-terminal
pub mod content_extractor;
pub mod stream_adapter;
pub mod terminal_core;

pub use content_extractor::ContentExtractor;
pub use stream_adapter::{RuntimeReader, RuntimeWriter, tee_output};
pub use terminal_core::{Terminal, TerminalSize};
