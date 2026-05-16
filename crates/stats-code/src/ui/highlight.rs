use std::fmt::Write;
use std::sync::OnceLock;

use colored::Colorize;

use super::caps::TerminalCapabilities;

/// Wraps `SyntaxSet` and `Theme` for syntax highlighting.
pub struct Highlighter {
    pub syntax_set: syntect::parsing::SyntaxSet,
    pub theme: syntect::highlighting::Theme,
}

static HIGHLIGHTER: OnceLock<Highlighter> = OnceLock::new();

fn get_highlighter() -> &'static Highlighter {
    HIGHLIGHTER.get_or_init(|| {
        let syntax_set = syntect::parsing::SyntaxSet::load_defaults_newlines();
        let ts = syntect::highlighting::ThemeSet::load_defaults();
        let theme = ts
            .themes
            .get("Monokai Extended")
            .or_else(|| ts.themes.get("base16-ocean.dark"))
            .cloned()
            .unwrap_or_else(|| {
                syntect::highlighting::ThemeSet::load_defaults()
                    .themes
                    .into_values()
                    .next()
                    .unwrap()
            });
        Highlighter { syntax_set, theme }
    })
}

/// Highlight a single line of code.
/// Returns a String with ANSI escape sequences.
pub fn highlight_line(line: &str, lang: &str, caps: &TerminalCapabilities) -> String {
    let hl = get_highlighter();
    let syntax = hl
        .syntax_set
        .find_syntax_by_token(lang)
        .or_else(|| hl.syntax_set.find_syntax_by_extension(lang));

    match syntax {
        Some(syn) => {
            let mut h = syntect::easy::HighlightLines::new(syn, &hl.theme);
            let ranges: Vec<(syntect::highlighting::Style, &str)> =
                syntect::util::LinesWithEndings::from(line)
                    .flat_map(|l| h.highlight_line(l, &hl.syntax_set).unwrap_or_default())
                    .collect();

            let mut result = String::new();
            for (style, text) in &ranges {
                let r = style.foreground.r;
                let g = style.foreground.g;
                let b = style.foreground.b;
                if caps.supports_truecolor {
                    let _ = write!(result, "\x1b[38;2;{r};{g};{b}m{text}\x1b[0m");
                } else {
                    let idx = super::gradient::quantize_to_256((r, g, b));
                    let _ = write!(result, "\x1b[38;5;{idx}m{text}\x1b[0m");
                }
            }
            result
        }
        None => {
            // Unknown language — blue-white text
            line.truecolor(180, 220, 255).to_string()
        }
    }
}

/// Render a code block header line: `╭─ {lang} ─────…─╮` (min width 40).
/// ASCII fallback: `+- {lang} ----+`
pub fn code_block_header(lang: &str, caps: &TerminalCapabilities) -> String {
    let min_width = 40;

    if caps.supports_unicode {
        // Unicode header: ╭─ {lang} ─────…─╮
        let inner_label = format!("─ {} ", lang);
        let overhead = 2; // ╭ and ╮
        let content_width = min_width - overhead;
        let fill_needed = if inner_label.len() < content_width {
            content_width - inner_label.len()
        } else {
            1
        };
        let fill: String = "─".repeat(fill_needed);
        format!("╭{inner_label}{fill}╮")
    } else {
        // ASCII fallback: +- {lang} ----+
        let inner_label = format!("- {} ", lang);
        let overhead = 2; // + and +
        let content_width = min_width - overhead;
        let fill_needed = if inner_label.len() < content_width {
            content_width - inner_label.len()
        } else {
            1
        };
        let fill: String = "-".repeat(fill_needed);
        format!("+{inner_label}{fill}+")
    }
}
