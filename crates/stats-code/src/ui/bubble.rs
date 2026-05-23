use std::io::{self, Write};

use colored::Colorize;

use super::wrap::wrap_text;

#[derive(Debug, Clone, Copy)]
pub enum BubbleKind {
    User,
    Assistant,
}

impl BubbleKind {
    fn color(self) -> (u8, u8, u8) {
        match self {
            BubbleKind::User => (240, 180, 60),      // amber
            BubbleKind::Assistant => (80, 200, 140), // green/teal
        }
    }

    fn label(self) -> &'static str {
        match self {
            BubbleKind::User => "You",
            BubbleKind::Assistant => "Stats",
        }
    }
}

pub struct ChatBubble {
    kind: BubbleKind,
    lines: Vec<String>,
    max_content_width: usize,
    unicode: bool,
}

impl ChatBubble {
    /// Create a new chat bubble.
    ///
    /// `term_width` should be the current terminal width, or 0 if unknown (defaults to 80).
    /// `unicode` controls whether to use rounded box-drawing chars or ASCII fallback.
    ///
    /// Bubble width = `min(content_width` + 4, `terminal_width` − 4) per spec requirement 8.4.
    pub fn new(kind: BubbleKind, body: &str, term_width: usize, unicode: bool) -> Self {
        // Default to 80 if width unknown (0 or unreasonable)
        let effective_width = if term_width == 0 { 80 } else { term_width };

        // Compute longest content line width (for content_width)
        let longest_line = body.lines().map(|l| l.chars().count()).max().unwrap_or(0);

        // Bubble width = min(content_width + 4, terminal_width − 4)
        // content_width is the inner text area; +4 accounts for │ + space + space + │
        // terminal_width − 4 ensures the bubble doesn't overflow the terminal
        let bubble_width = (longest_line + 4).min(effective_width.saturating_sub(4));
        // Inner content width is bubble_width minus the 4 border/padding chars
        let content_width = bubble_width.saturating_sub(4).max(12);

        let wrapped = wrap_text(body, content_width);
        Self {
            kind,
            lines: wrapped,
            max_content_width: content_width,
            unicode,
        }
    }

    pub fn render(&self, out: &mut impl Write) -> io::Result<()> {
        let (r, g, b) = self.kind.color();
        let label = self.kind.label();

        if self.unicode {
            self.render_unicode(out, r, g, b, label)
        } else {
            self.render_ascii(out, r, g, b, label)
        }
    }

    fn render_minimal(
        &self,
        out: &mut impl Write,
        r: u8,
        g: u8,
        b: u8,
        label: &str,
    ) -> io::Result<()> {
        // Label line: colored and bold
        writeln!(out, "{}", label.truecolor(r, g, b).bold())?;

        // Content lines: 2-space indent, default terminal color
        for line in &self.lines {
            writeln!(out, "  {line}")?;
        }

        // Blank line separator
        writeln!(out)?;
        out.flush()
    }

    fn render_unicode(
        &self,
        out: &mut impl Write,
        r: u8,
        g: u8,
        b: u8,
        label: &str,
    ) -> io::Result<()> {
        // Top border: ╭─ You ──────────╮
        let top_text = format!(" {label} ");
        let top_fill_width = self
            .max_content_width
            .saturating_sub(top_text.chars().count())
            .saturating_add(2); // +2 for the two border padding spaces
        let top_fill = "─".repeat(top_fill_width);
        writeln!(
            out,
            "{}",
            format!("╭─{top_text}{top_fill}╮").truecolor(r, g, b)
        )?;

        // Content lines
        for line in &self.lines {
            let padding = self.max_content_width.saturating_sub(line.chars().count());
            writeln!(
                out,
                "{}",
                format!("│ {}{} │", line, " ".repeat(padding)).truecolor(r, g, b)
            )?;
        }

        // Bottom border: ╰──────────────────╯
        let bottom_fill = "─".repeat(self.max_content_width + 2);
        writeln!(out, "{}", format!("╰─{bottom_fill}─╯").truecolor(r, g, b))?;

        out.flush()
    }

    fn render_ascii(
        &self,
        out: &mut impl Write,
        r: u8,
        g: u8,
        b: u8,
        label: &str,
    ) -> io::Result<()> {
        // Top border: +-- You ----------------+
        let top_text = format!(" {label} ");
        let top_fill_width = self
            .max_content_width
            .saturating_sub(top_text.chars().count())
            .saturating_add(2);
        let top_fill = "-".repeat(top_fill_width);
        writeln!(
            out,
            "{}",
            format!("+-{top_text}{top_fill}+").truecolor(r, g, b)
        )?;

        // Content lines
        for line in &self.lines {
            let padding = self.max_content_width.saturating_sub(line.chars().count());
            writeln!(
                out,
                "{}",
                format!("| {}{} |", line, " ".repeat(padding)).truecolor(r, g, b)
            )?;
        }

        // Bottom border: +--------------------+
        let bottom_fill = "-".repeat(self.max_content_width + 2);
        writeln!(out, "{}", format!("+-{bottom_fill}-+").truecolor(r, g, b))?;

        out.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bubble_width_spec_formula() {
        // "hello world" = 11 chars, so content_width + 4 = 15
        // terminal_width - 4 = 76
        // min(15, 76) = 15, inner = 15 - 4 = 11 (but min 12)
        let bubble = ChatBubble::new(BubbleKind::Assistant, "hello world", 80, true);
        assert_eq!(bubble.max_content_width, 12); // min 12
    }

    #[test]
    fn bubble_width_long_content() {
        // 60-char content line → content_width + 4 = 64
        // terminal_width - 4 = 76
        // min(64, 76) = 64, inner = 60
        let long = "a".repeat(60);
        let bubble = ChatBubble::new(BubbleKind::User, &long, 80, true);
        assert_eq!(bubble.max_content_width, 60);
    }

    #[test]
    fn bubble_width_narrow_terminal() {
        // 60-char content, terminal = 40
        // content_width + 4 = 64, terminal_width - 4 = 36
        // min(64, 36) = 36, inner = 32
        let long = "a".repeat(60);
        let bubble = ChatBubble::new(BubbleKind::Assistant, &long, 40, true);
        assert_eq!(bubble.max_content_width, 32);
    }

    #[test]
    fn bubble_width_defaults_to_80_when_unknown() {
        // term_width = 0 → effective = 80
        // "hello" = 5 chars, content_width + 4 = 9
        // terminal_width - 4 = 76
        // min(9, 76) = 9, inner = max(5, 12) = 12
        let bubble = ChatBubble::new(BubbleKind::User, "hello", 0, true);
        assert_eq!(bubble.max_content_width, 12);
    }

    #[test]
    fn bubble_renders_unicode_content() {
        let bubble = ChatBubble::new(BubbleKind::Assistant, "hello world", 80, true);
        let mut buf = Vec::new();
        bubble.render(&mut buf).unwrap();
        let s = String::from_utf8_lossy(&buf);
        assert!(s.contains("hello world"), "missing content: {s}");
        assert!(s.contains("╭"), "missing top border: {s}");
        assert!(s.contains("╰"), "missing bottom border: {s}");
        assert!(s.contains("│"), "missing side border: {s}");
    }

    #[test]
    fn bubble_renders_ascii_fallback() {
        let bubble = ChatBubble::new(BubbleKind::User, "test message", 80, false);
        let mut buf = Vec::new();
        bubble.render(&mut buf).unwrap();
        let s = String::from_utf8_lossy(&buf);
        assert!(s.contains("test message"), "missing content: {s}");
        assert!(s.contains("+"), "missing ASCII corner: {s}");
        assert!(s.contains("-"), "missing ASCII horizontal: {s}");
        assert!(s.contains("|"), "missing ASCII vertical: {s}");
        // Should NOT contain Unicode box chars
        assert!(!s.contains("╭"), "should not have unicode: {s}");
        assert!(!s.contains("│"), "should not have unicode vertical: {s}");
    }
}
