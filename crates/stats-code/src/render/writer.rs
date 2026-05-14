use std::fmt::Write;

/// A builder for composing aligned plain-text reports suitable for terminal display.
///
/// Each method returns `&mut Self` to allow chaining, except `finish` which
/// consumes the writer and returns the accumulated output.
#[allow(dead_code)]
pub(crate) struct TextReportWriter {
    buf: String,
}

#[allow(dead_code)]
impl TextReportWriter {
    /// Create a new empty writer.
    pub fn new() -> Self {
        Self { buf: String::new() }
    }

    /// Write a top-level title line (no indentation).
    pub fn title(&mut self, text: &str) -> &mut Self {
        let _ = writeln!(self.buf, "{text}");
        self
    }

    /// Write a labeled field with 2-space indent and the label padded to 17 characters.
    pub fn field(&mut self, label: &str, value: impl std::fmt::Display) -> &mut Self {
        let _ = writeln!(self.buf, "  {label:<17}{value}");
        self
    }

    /// Write a labeled field only if the value is `Some`; skip entirely if `None`.
    pub fn field_opt(
        &mut self,
        label: &str,
        value: Option<impl std::fmt::Display>,
    ) -> &mut Self {
        if let Some(v) = value {
            let _ = writeln!(self.buf, "  {label:<17}{v}");
        }
        self
    }

    /// Write an empty line.
    pub fn blank_line(&mut self) -> &mut Self {
        let _ = writeln!(self.buf);
        self
    }

    /// Write a section heading with box-drawing decoration.
    pub fn section(&mut self, heading: &str) -> &mut Self {
        let _ = writeln!(self.buf, "── {heading} ──");
        self
    }

    /// Write a table header row with columns aligned to the given widths.
    ///
    /// Each entry in `columns` is `(name, width)`. Column names are left-padded
    /// to their specified width, separated by spaces.
    pub fn table_header(&mut self, columns: &[(&str, usize)]) -> &mut Self {
        let mut line = String::new();
        for (i, (name, width)) in columns.iter().enumerate() {
            if i > 0 {
                line.push(' ');
            }
            let _ = write!(line, "{name:<width$}");
        }
        let _ = writeln!(self.buf, "  {line}");
        // Write a separator line matching the total width
        let total_width = columns.iter().map(|(_, w)| w).sum::<usize>() + columns.len().saturating_sub(1);
        let separator: String = "─".repeat(total_width);
        let _ = writeln!(self.buf, "  {separator}");
        self
    }

    /// Write a table data row with cells aligned to the given widths.
    ///
    /// Each cell is formatted using its `Display` impl and left-aligned within
    /// the corresponding width.
    pub fn table_row(&mut self, cells: &[&dyn std::fmt::Display], widths: &[usize]) -> &mut Self {
        let mut line = String::new();
        for (i, cell) in cells.iter().enumerate() {
            if i > 0 {
                line.push(' ');
            }
            let width = widths.get(i).copied().unwrap_or(0);
            let formatted = format!("{cell}");
            let _ = write!(line, "{formatted:<width$}");
        }
        let _ = writeln!(self.buf, "  {line}");
        self
    }

    /// Consume the writer and return the accumulated text.
    pub fn finish(self) -> String {
        self.buf
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_writes_plain_line() {
        let mut w = TextReportWriter::new();
        w.title("Logistic Model");
        assert_eq!(w.finish(), "Logistic Model\n");
    }

    #[test]
    fn field_aligns_label_to_17_chars() {
        let mut w = TextReportWriter::new();
        w.field("Status", "ok");
        assert_eq!(w.finish(), "  Status           ok\n");
    }

    #[test]
    fn field_opt_skips_none() {
        let mut w = TextReportWriter::new();
        w.field_opt("Missing", None::<&str>);
        w.field_opt("Present", Some("yes"));
        assert_eq!(w.finish(), "  Present          yes\n");
    }

    #[test]
    fn blank_line_inserts_empty_line() {
        let mut w = TextReportWriter::new();
        w.title("A");
        w.blank_line();
        w.title("B");
        assert_eq!(w.finish(), "A\n\nB\n");
    }

    #[test]
    fn section_writes_decorated_heading() {
        let mut w = TextReportWriter::new();
        w.section("Coefficients");
        assert_eq!(w.finish(), "── Coefficients ──\n");
    }

    #[test]
    fn table_header_and_row_align() {
        let mut w = TextReportWriter::new();
        w.table_header(&[("Name", 10), ("Value", 8)]);
        w.table_row(&[&"age", &"0.45"], &[10, 8]);
        let output = w.finish();
        let lines: Vec<&str> = output.lines().collect();
        assert_eq!(lines[0], "  Name       Value   ");
        assert_eq!(lines[1], "  ───────────────────");
        assert_eq!(lines[2], "  age        0.45    ");
    }

    #[test]
    fn chaining_produces_complete_report() {
        let mut w = TextReportWriter::new();
        w.title("Test Report");
        w.field("Status", "ok");
        w.field("Count", 42);
        w.blank_line();
        w.section("Details");
        let output = w.finish();
        assert!(output.contains("Test Report\n"));
        assert!(output.contains("  Status           ok\n"));
        assert!(output.contains("  Count            42\n"));
        assert!(output.contains("── Details ──\n"));
    }
}
