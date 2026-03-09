// syntax_highlight.rs - Syntect-based syntax highlighting for diff view
use std::path::Path;
use std::sync::OnceLock;
use syntect::highlighting::{Theme, ThemeSet};
use syntect::parsing::{SyntaxReference, SyntaxSet};

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

/// Find syntax for a file path by extension, with fallback mappings for
/// languages not in Syntect's default set (TypeScript, TSX, JSX, Vue, etc.).
/// Uses `find_syntax_by_extension` to avoid file I/O (git diff paths are relative).
fn find_syntax_for_path(file_path: &str) -> &'static SyntaxReference {
    let ss = syntax_set();
    let ext = Path::new(file_path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");

    // Try direct extension match first
    if let Some(syn) = ss.find_syntax_by_extension(ext) {
        return syn;
    }

    // Fallback mappings for common extensions not in default set
    let fallback_ext = match ext {
        "ts" | "mts" | "cts" => "js",
        "tsx" => "jsx",
        "jsx" => "js",
        "mjs" | "cjs" => "js",
        "vue" | "svelte" => "html",
        "mdx" => "md",
        "jsonc" | "json5" => "json",
        "zsh" | "fish" => "sh",
        "dockerfile" => "sh",
        "toml" => "ini",        // rough fallback
        "graphql" | "gql" => "js", // passable
        _ => "",
    };

    if !fallback_ext.is_empty() {
        if let Some(syn) = ss.find_syntax_by_extension(fallback_ext) {
            return syn;
        }
    }

    ss.find_syntax_plain_text()
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
    let syntax = find_syntax_for_path(file_path);

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
        let syntax = find_syntax_for_path(file_path);
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
