use std::io::{self, Write};

use super::caps::TerminalCapabilities;
// use super::highlight;  // FIXME: needs `syntect` crate

#[derive(Debug, PartialEq, Eq)]
pub enum FenceState {
    Outside,
    Inside { lang: Option<String> },
}

pub struct StreamRenderer<'a> {
    caps: &'a TerminalCapabilities,
    buffer: String,
    fence_state: FenceState,
    out: &'a mut dyn Write,
}

impl<'a> StreamRenderer<'a> {
    pub fn new(out: &'a mut impl Write, caps: &'a TerminalCapabilities) -> Self {
        Self {
            caps,
            buffer: String::with_capacity(4096),
            fence_state: FenceState::Outside,
            out,
        }
    }

    /// Push text into the renderer. Complete lines are rendered immediately;
    /// incomplete lines are held in the buffer.
    pub fn push_text(&mut self, text: &str) -> io::Result<()> {
        self.buffer.push_str(text);

        // Force-flush when buffer exceeds 10,000 chars without a newline
        if self.buffer.len() > 10_000 && !self.buffer.contains('\n') {
            let content = std::mem::take(&mut self.buffer);
            self.emit_line(&content)?;
            return Ok(());
        }

        // Extract complete lines
        while let Some(pos) = self.buffer.find('\n') {
            let line = self.buffer[..pos].to_string();
            self.buffer.drain(..=pos);
            // Carriage-return handling
            let clean = line.trim_end_matches('\r');
            self.emit_line(clean)?;
        }
        Ok(())
    }

    /// Flush any remaining buffered content.
    pub fn flush(&mut self) -> io::Result<()> {
        if !self.buffer.is_empty() {
            let content = std::mem::take(&mut self.buffer);
            self.emit_line(&content)?;
        }
        // Add trailing newline
        writeln!(self.out)?;
        self.out.flush()
    }

    fn emit_line(&mut self, line: &str) -> io::Result<()> {
        let trimmed = line.trim_start();
        // Check for code fence
        if trimmed.starts_with("```") {
            match &self.fence_state {
                FenceState::Outside => {
                    let lang = trimmed.strip_prefix("```").unwrap_or("").trim().to_string();
                    let lang = if lang.is_empty() { None } else { Some(lang) };
                    self.fence_state = FenceState::Inside { lang };
                    return Ok(());
                }
                FenceState::Inside { .. } => {
                    self.fence_state = FenceState::Outside;
                    return Ok(());
                }
            }
        }

        match &self.fence_state {
            FenceState::Inside { lang: Some(lang) } => {
                // let highlighted = highlight::highlight_line(line, lang, self.caps);
                let _ = lang;
                writeln!(self.out, "  {line}")?;
            }
            FenceState::Inside { lang: None } => {
                use colored::Colorize;
                writeln!(self.out, "  {}", line.truecolor(180, 220, 255))?;
            }
            FenceState::Outside => {
                writeln!(self.out, "  {line}")?;
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct StreamOutcome {
    pub text: String,
    pub tool_use_blocks: Vec<api::OutputContentBlock>,
    pub usage: Option<api::Usage>,
    pub interrupted: bool,
    pub error: Option<String>,
    pub request_id: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_caps() -> TerminalCapabilities {
        TerminalCapabilities {
            supports_truecolor: true,
            supports_unicode: true,
            supports_sixel: false,
            width: 80,
            height: 40,
        }
    }

    #[test]
    fn fence_toggling() {
        let caps = test_caps();
        let mut buf = Vec::new();
        {
            let mut sr = StreamRenderer::new(&mut buf, &caps);
            sr.push_text("outside1\n").unwrap();
            sr.push_text("```rust\n").unwrap();
            sr.push_text("let x = 1;\n").unwrap();
            sr.push_text("```\n").unwrap();
            sr.push_text("outside2\n").unwrap();
            sr.flush().unwrap();
        }
        let s = String::from_utf8_lossy(&buf);
        assert!(s.contains("outside1"), "missing outside1: {s}");
        assert!(s.contains("outside2"), "missing outside2: {s}");
    }

    #[test]
    fn partial_line_buffering() {
        let caps = test_caps();
        let mut buf = Vec::new();
        {
            let mut sr = StreamRenderer::new(&mut buf, &caps);
            sr.push_text("hello ").unwrap();
            sr.push_text("world\n").unwrap();
            sr.flush().unwrap();
        }
        let s = String::from_utf8_lossy(&buf);
        assert!(s.contains("hello world"), "missing combined line: {s}");
    }

    #[test]
    fn flush_empty_buffer_adds_newline() {
        let caps = test_caps();
        let mut buf = Vec::new();
        {
            let mut sr = StreamRenderer::new(&mut buf, &caps);
            sr.flush().unwrap();
        }
        // Should end with \n
        let s = String::from_utf8_lossy(&buf);
        assert!(s.ends_with('\n'), "no trailing newline: {s:?}");
    }
}
