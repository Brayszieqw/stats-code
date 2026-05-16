mod bubble;
pub(crate) mod caps;
// mod diff;        // FIXME: needs `similar` crate
mod gradient;
// mod highlight;   // FIXME: needs `syntect` crate
mod input_box;
pub mod progress;
pub mod spinner;
pub mod stream;
pub mod tool_display;
mod welcome;
mod wrap;

#[cfg(test)]
mod proptest_ui;

pub use stream::{StreamOutcome, StreamRenderer};

use std::io::{self, Write};

use colored::Colorize;

use crate::gugugaga_art::print_gugugaga_image;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatEntryKind {
    User,
    Assistant,
    System,
    Tool,
    Error,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChatUiStatus {
    pub model: String,
    pub workspace: String,
    pub tools_enabled: bool,
    pub fast_mode: bool,
    pub vim_mode: bool,
    pub turns: usize,
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub estimated_cost_usd: Option<f64>,
    pub session_loaded: bool,
}

#[derive(Debug)]
pub struct ChatUi {
    pub caps: caps::TerminalCapabilities,
}

impl ChatUi {
    #[must_use]
    pub fn new() -> Self {
        Self {
            caps: caps::TerminalCapabilities::detect(),
        }
    }

    #[must_use]
    pub fn with_caps(caps: caps::TerminalCapabilities) -> Self {
        Self { caps }
    }

    #[must_use]
    pub fn current_width(&self) -> usize {
        match crossterm::terminal::size() {
            Ok((w, _)) => (w as usize).max(60),
            Err(_) => 80,
        }
    }

    pub fn print_welcome(&self, out: &mut impl Write, status: &ChatUiStatus) -> io::Result<()> {
        welcome::render_welcome(out, &self.caps, status)
    }

    pub fn print_turn(
        &self,
        out: &mut impl Write,
        kind: ChatEntryKind,
        body: &str,
    ) -> io::Result<()> {
        match kind {
            ChatEntryKind::User | ChatEntryKind::Assistant => {
                let bubble_kind = match kind {
                    ChatEntryKind::User => bubble::BubbleKind::User,
                    ChatEntryKind::Assistant => bubble::BubbleKind::Assistant,
                    _ => unreachable!(),
                };
                let bubble = bubble::ChatBubble::new(
                    bubble_kind,
                    body,
                    self.current_width(),
                    self.caps.supports_unicode,
                );
                bubble.render(out)
            }
            ChatEntryKind::System | ChatEntryKind::Tool | ChatEntryKind::Error => {
                let (label, color) = match kind {
                    ChatEntryKind::System => ("Info", (130, 170, 220)),
                    ChatEntryKind::Tool => ("Tool", (180, 150, 100)),
                    ChatEntryKind::Error => ("Error", (220, 90, 90)),
                    _ => unreachable!(),
                };
                let label_str = label
                    .truecolor(color.0, color.1, color.2)
                    .bold()
                    .to_string();
                writeln!(out, "{label_str}")?;
                for raw_line in body.lines() {
                    writeln!(out, "  {raw_line}")?;
                }
                writeln!(out)?;
                out.flush()
            }
        }
    }

    pub fn print_status_bar(
        &self,
        out: &mut impl Write,
        status: &ChatUiStatus,
        pending: Option<&str>,
    ) -> io::Result<()> {
        input_box::print_top(out, &self.caps)?;
        if let Some(msg) = pending {
            writeln!(out, "  {}", msg.truecolor(220, 170, 90))?;
        } else {
            let cost = status
                .estimated_cost_usd
                .map(|v| format!("  cost=${v:.4}"))
                .unwrap_or_default();
            let left = "  `/` commands  `!` shell  Enter send  Ctrl+D exit"
                .truecolor(120, 120, 110)
                .to_string();
            let right = format!(
                "tokens={}/{} · ⊙ auto{}",
                status.input_tokens, status.output_tokens, cost
            )
            .truecolor(130, 130, 120)
            .to_string();
            let left_visible = 50usize;
            let right_visible = right_plain_len(status, &cost);
            let width = self.current_width();
            let pad = width
                .saturating_sub(left_visible)
                .saturating_sub(right_visible);
            writeln!(out, "{}{}{}", left, " ".repeat(pad), right)?;
        }
        out.flush()
    }

    pub fn print_input_bottom(&self, out: &mut impl Write) -> io::Result<()> {
        input_box::print_bottom(out, &self.caps, None)
    }

    pub fn print_input_bottom_status(
        &self,
        out: &mut impl Write,
        status: &ChatUiStatus,
    ) -> io::Result<()> {
        input_box::print_bottom(out, &self.caps, Some(status))
    }

    /// Render tool call display during execution
    pub fn print_tool_call(
        &self,
        out: &mut impl Write,
        tool_name: &str,
        tool_input_summary: &str,
    ) -> io::Result<()> {
        tool_display::print_tool_call_line(out, tool_name, tool_input_summary, &self.caps)
    }

    /// Render tool result anchor after execution
    pub fn print_tool_result(
        &self,
        out: &mut impl Write,
        summary: &str,
        is_error: bool,
    ) -> io::Result<()> {
        tool_display::print_tool_result_anchor(out, summary, is_error, &self.caps)
    }

    /// Render a diff view (currently stubbed — awaiting `similar` dep)
    pub fn print_diff(
        &self,
        out: &mut impl Write,
        _path: &str,
        _cwd: &str,
        _before: Option<&str>,
        _after: Option<&str>,
        _is_binary: bool,
    ) -> io::Result<()> {
        writeln!(out, "  (diff rendering unavailable)")
    }

    /// Print the /about Sixel penguin
    pub fn print_about(&self, out: &mut impl Write) -> io::Result<()> {
        if self.caps.supports_sixel {
            print_gugugaga_image();
        } else {
            writeln!(
                out,
                "Sixel support not detected. Use a Sixel-capable terminal (e.g. Windows Terminal)."
            )?;
        }
        Ok(())
    }

    /// Create a `StreamRenderer` for streaming assistant output
    pub fn stream_renderer<'a>(&'a self, out: &'a mut impl Write) -> stream::StreamRenderer<'a> {
        stream::StreamRenderer::new(out, &self.caps)
    }

    pub fn render_markdown_body(&self, out: &mut impl Write, body: &str) -> io::Result<()> {
        let mut sr = self.stream_renderer(out);
        sr.push_text(body)?;
        sr.flush()
    }

    /// 渲染完整的工具调用日志（初始显示）
    ///
    /// 所有条目默认折叠显示，光标位置由 `log.cursor` 决定。
    pub fn render_tool_log(
        &self,
        out: &mut impl Write,
        log: &tool_display::ToolCallLog,
    ) -> io::Result<()> {
        for (i, entry) in log.entries.iter().enumerate() {
            let is_focused = i == log.cursor;
            let line = tool_display::render_collapsed(entry, is_focused, &self.caps);
            writeln!(out, "{}", line)?;
        }
        out.flush()
    }
}

impl Default for ChatUi {
    fn default() -> Self {
        Self::new()
    }
}

fn right_plain_len(status: &ChatUiStatus, cost: &str) -> usize {
    format!(
        "tokens={}/{} · ⊙ auto{}",
        status.input_tokens, status.output_tokens, cost
    )
    .chars()
    .count()
}
