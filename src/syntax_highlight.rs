// syntax_highlight.rs - Syntect-based syntax highlighting for diff view
use std::sync::OnceLock;
use syntect::highlighting::{Theme, ThemeSet};
use syntect::parsing::SyntaxSet;

static SYNTAX_SET: OnceLock<SyntaxSet> = OnceLock::new();
static THEME: OnceLock<Theme> = OnceLock::new();

fn syntax_set() -> &'static SyntaxSet {
    SYNTAX_SET.get_or_init(|| SyntaxSet::load_defaults_newlines())
}

fn theme() -> &'static Theme {
    THEME.get_or_init(|| {
        let ts = ThemeSet::load_defaults();
        ts.themes["base16-ocean.dark"].clone()
    })
}

/// A highlighted span: (foreground RGBA, text)
#[derive(Debug, Clone)]
pub struct HighlightSpan {
    pub fg: (u8, u8, u8, u8), // r, g, b, a
    pub text: String,
}

/// A line of highlighted text
#[derive(Debug, Clone)]
pub struct HighlightedLine {
    pub spans: Vec<HighlightSpan>,
}

/// Highlight lines of code for a given file extension.
/// Returns one HighlightedLine per input line.
pub fn highlight_lines(file_path: &str, lines: &[&str]) -> Vec<HighlightedLine> {
    let ss = syntax_set();
    let syntax = ss
        .find_syntax_for_file(file_path)
        .ok()
        .flatten()
        .unwrap_or_else(|| ss.find_syntax_plain_text());

    let mut highlighter = syntect::easy::HighlightLines::new(syntax, theme());

    lines
        .iter()
        .map(|line| {
            let regions = highlighter
                .highlight_line(line, ss)
                .unwrap_or_default();
            HighlightedLine {
                spans: regions
                    .into_iter()
                    .map(|(style, text)| HighlightSpan {
                        fg: (
                            style.foreground.r,
                            style.foreground.g,
                            style.foreground.b,
                            style.foreground.a,
                        ),
                        text: text.to_string(),
                    })
                    .collect(),
            }
        })
        .collect()
}

/// Highlight a single line for a given file extension, reusing a persistent highlighter state.
/// This is more efficient when highlighting lines incrementally.
pub struct LineHighlighter {
    highlighter: syntect::easy::HighlightLines<'static>,
}

impl LineHighlighter {
    pub fn new(file_path: &str) -> Self {
        let ss = syntax_set();
        let syntax = ss
            .find_syntax_for_file(file_path)
            .ok()
            .flatten()
            .unwrap_or_else(|| ss.find_syntax_plain_text());

        Self {
            highlighter: syntect::easy::HighlightLines::new(syntax, theme()),
        }
    }

    pub fn highlight_line(&mut self, line: &str) -> HighlightedLine {
        let ss = syntax_set();
        let regions = self
            .highlighter
            .highlight_line(line, ss)
            .unwrap_or_default();
        HighlightedLine {
            spans: regions
                .into_iter()
                .map(|(style, text)| HighlightSpan {
                    fg: (
                        style.foreground.r,
                        style.foreground.g,
                        style.foreground.b,
                        style.foreground.a,
                    ),
                    text: text.to_string(),
                })
                .collect(),
        }
    }
}
