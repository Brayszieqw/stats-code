use std::io::{self, Write};

use colored::Colorize;
use similar::{ChangeTag, TextDiff};

const MAX_DIFF_LINES: usize = 50;
const CONTEXT_LINES: usize = 3;

/// Holds all data needed to render a unified diff view.
pub struct DiffView<'a> {
    pub path: &'a str,
    pub cwd: &'a str,
    pub before: Option<&'a str>,
    pub after: Option<&'a str>,
    pub is_binary: bool,
}

/// Render a unified diff view.
pub fn render_diff(
    out: &mut impl Write,
    path: &str,
    cwd: &str,
    before: Option<&str>,
    after: Option<&str>,
    is_binary: bool,
) -> io::Result<()> {
    let view = DiffView { path, cwd, before, after, is_binary };
    render_diff_view(out, &view)
}

/// Render a diff from a `DiffView`.
pub fn render_diff_view(out: &mut impl Write, view: &DiffView<'_>) -> io::Result<()> {
    let rel = relativize_path(view.path, view.cwd);

    if view.is_binary {
        writeln!(out, "  Binary file: {rel}")?;
        return Ok(());
    }

    match (view.before, view.after) {
        (None, Some(new)) => render_new_file(out, &rel, new),
        (Some(old), Some(new)) => render_modified(out, &rel, old, new),
        (Some(_), None) => writeln!(out, "  Removed: {rel}"),
        (None, None) => Ok(()),
    }
}

/// Render a new file — all lines as additions.
fn render_new_file(out: &mut impl Write, rel: &str, content: &str) -> io::Result<()> {
    let lines: Vec<&str> = content.lines().collect();
    writeln!(out, "  New file: {rel}")?;
    if lines.is_empty() {
        return Ok(());
    }
    let truncated = lines.len() > MAX_DIFF_LINES;
    let shown = if truncated { MAX_DIFF_LINES } else { lines.len() };
    for line in &lines[..shown] {
        writeln!(out, "{}", format!("+{line}").truecolor(80, 200, 140))?;
    }
    if truncated {
        let remaining = lines.len() - MAX_DIFF_LINES;
        writeln!(out, "  ... (+{remaining} more lines)")?;
    }
    Ok(())
}

/// Render a modified file with unified diff hunks and 3 lines of context.
fn render_modified(out: &mut impl Write, rel: &str, old: &str, new: &str) -> io::Result<()> {
    writeln!(out, "  {rel}")?;

    let diff = TextDiff::from_lines(old, new);
    let mut unified = diff.unified_diff();
    unified.context_radius(CONTEXT_LINES);

    // Collect all output lines (hunk headers + content lines)
    let mut output_lines: Vec<DiffLine> = Vec::new();
    let mut total_adds = 0usize;
    let mut total_rems = 0usize;

    for hunk in unified.iter_hunks() {
        let header = hunk.header().to_string();
        output_lines.push(DiffLine::Header(header));

        for change in hunk.iter_changes() {
            match change.tag() {
                ChangeTag::Insert => {
                    total_adds += 1;
                    output_lines.push(DiffLine::Add(change.value().trim_end_matches('\n').to_string()));
                }
                ChangeTag::Delete => {
                    total_rems += 1;
                    output_lines.push(DiffLine::Remove(change.value().trim_end_matches('\n').to_string()));
                }
                ChangeTag::Equal => {
                    output_lines.push(DiffLine::Context(change.value().trim_end_matches('\n').to_string()));
                }
            }
        }
    }

    // Render with truncation at MAX_DIFF_LINES
    let mut shown = 0usize;
    let mut shown_adds = 0usize;
    let mut shown_rems = 0usize;

    for line in &output_lines {
        if shown >= MAX_DIFF_LINES {
            break;
        }
        match line {
            DiffLine::Header(h) => {
                writeln!(out, "{h}")?;
            }
            DiffLine::Add(text) => {
                writeln!(out, "{}", format!("+{text}").truecolor(80, 200, 140))?;
                shown_adds += 1;
            }
            DiffLine::Remove(text) => {
                writeln!(out, "{}", format!("-{text}").truecolor(220, 90, 90))?;
                shown_rems += 1;
            }
            DiffLine::Context(text) => {
                writeln!(out, " {text}")?;
            }
        }
        shown += 1;
    }

    if output_lines.len() > MAX_DIFF_LINES {
        let remaining_adds = total_adds.saturating_sub(shown_adds);
        let remaining_rems = total_rems.saturating_sub(shown_rems);
        writeln!(out, "  ... (+{remaining_adds} -{remaining_rems} more)")?;
    }

    Ok(())
}

#[derive(Debug)]
enum DiffLine {
    Header(String),
    Add(String),
    Remove(String),
    Context(String),
}

/// Compute a relative path from `cwd` for display.
pub fn relativize_path(path: &str, cwd: &str) -> String {
    if let Ok(p) = std::path::Path::new(path).strip_prefix(cwd) {
        p.display().to_string()
    } else {
        path.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_file_all_additions() {
        let mut buf = Vec::new();
        render_diff(
            &mut buf,
            "/proj/new.txt",
            "/proj",
            None,
            Some("line1\nline2\nline3"),
            false,
        )
        .unwrap();
        let s = String::from_utf8_lossy(&buf);
        assert!(s.contains("New file"), "no new file header: {s}");
        assert!(s.contains("+line1"), "no addition: {s}");
    }

    #[test]
    fn binary_file_summary() {
        let mut buf = Vec::new();
        render_diff(
            &mut buf,
            "/proj/img.png",
            "/proj",
            Some("..."),
            Some("..."),
            true,
        )
        .unwrap();
        let s = String::from_utf8_lossy(&buf);
        assert!(s.contains("Binary"), "no binary label: {s}");
    }

    #[test]
    fn relativize_inside_project() {
        let rel = relativize_path("/proj/src/main.rs", "/proj");
        assert_eq!(rel, "src/main.rs");
    }

    #[test]
    fn relativize_outside_project() {
        let rel = relativize_path("/other/file.txt", "/proj");
        assert_eq!(rel, "/other/file.txt");
    }

    #[test]
    fn modified_file_shows_hunk_header() {
        let mut buf = Vec::new();
        render_diff(
            &mut buf,
            "/proj/file.rs",
            "/proj",
            Some("line1\nline2\nline3\n"),
            Some("line1\nmodified\nline3\n"),
            false,
        )
        .unwrap();
        let s = String::from_utf8_lossy(&buf);
        assert!(s.contains("@@"), "no hunk header: {s}");
        assert!(s.contains("-line2"), "no removal: {s}");
        assert!(s.contains("+modified"), "no addition: {s}");
    }

    #[test]
    fn truncation_at_50_lines() {
        // Generate a diff with more than 50 lines of output
        let old_lines: Vec<String> = (0..60).map(|i| format!("old_line_{i}")).collect();
        let new_lines: Vec<String> = (0..60).map(|i| format!("new_line_{i}")).collect();
        let old = old_lines.join("\n") + "\n";
        let new = new_lines.join("\n") + "\n";

        let mut buf = Vec::new();
        render_diff(&mut buf, "/proj/big.rs", "/proj", Some(&old), Some(&new), false).unwrap();
        let s = String::from_utf8_lossy(&buf);
        assert!(s.contains("... (+"), "no truncation summary: {s}");
        assert!(s.contains("more)"), "no truncation summary: {s}");
    }

    #[test]
    fn context_lines_prefixed_with_space() {
        let mut buf = Vec::new();
        render_diff(
            &mut buf,
            "/proj/ctx.rs",
            "/proj",
            Some("a\nb\nc\nd\ne\n"),
            Some("a\nb\nX\nd\ne\n"),
            false,
        )
        .unwrap();
        let s = String::from_utf8_lossy(&buf);
        // Context lines should be prefixed with a space
        assert!(s.contains(" a") || s.contains(" b") || s.contains(" d") || s.contains(" e"),
            "no context lines with space prefix: {s}");
    }
}
