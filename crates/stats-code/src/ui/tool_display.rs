use std::io::{self, IsTerminal, Write};
use std::time::Duration;

use colored::Colorize;
use crossterm::{cursor, queue, terminal};

use super::caps::TerminalCapabilities;
// use super::diff::relativize_path;  // FIXME: needs `similar` crate

/// Simple relative-path helper (avoids dependency on `diff` module).
fn relativize_path(path: &str, cwd: &str) -> String {
    let path = path.replace('\\', "/");
    let cwd = cwd.replace('\\', "/");
    if let Some(stripped) = path.strip_prefix(&cwd) {
        let stripped = stripped.strip_prefix('/').unwrap_or(stripped);
        if stripped.is_empty() {
            ".".to_string()
        } else {
            stripped.to_string()
        }
    } else {
        path
    }
}

// ── Data model ────────────────────────────────────────────────────────────────

/// 单次工具调用的完整记录
#[derive(Debug, Clone)]
pub struct ToolCallEntry {
    pub tool_name: String,
    pub input_summary: String,
    pub full_input: String,
    pub output: String,
    pub is_error: bool,
    pub elapsed: Duration,
    pub collapsed: bool,
}

impl ToolCallEntry {
    /// 创建新条目，默认折叠
    pub fn new(
        tool_name: String,
        input_summary: String,
        full_input: String,
        output: String,
        is_error: bool,
        elapsed: Duration,
    ) -> Self {
        Self {
            tool_name,
            input_summary,
            full_input,
            output,
            is_error,
            elapsed,
            collapsed: true,
        }
    }
}

/// 工具调用日志，持有条目列表和光标位置
#[derive(Debug, Clone)]
pub struct ToolCallLog {
    pub entries: Vec<ToolCallEntry>,
    pub cursor: usize,
}

impl ToolCallLog {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            cursor: 0,
        }
    }

    pub fn push(&mut self, entry: ToolCallEntry) {
        self.entries.push(entry);
    }

    pub fn toggle(&mut self, index: usize) {
        if let Some(entry) = self.entries.get_mut(index) {
            entry.collapsed = !entry.collapsed;
        }
    }

    pub fn move_cursor_up(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    pub fn move_cursor_down(&mut self) {
        if !self.entries.is_empty() {
            self.cursor = (self.cursor + 1).min(self.entries.len() - 1);
        }
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// 非交互模式：强制展开所有条目
    pub fn expand_all(&mut self) {
        for entry in &mut self.entries {
            entry.collapsed = false;
        }
    }
}

impl Default for ToolCallLog {
    fn default() -> Self {
        Self::new()
    }
}

// ── New rendering functions ────────────────────────────────────────────────────

/// 渲染折叠状态的单行摘要
///
/// 格式：`{focus_indicator} {icon} {tool_name} {input_summary}  ⎿ {result_summary}`
///
/// - `focus_indicator`：聚焦时为 `▶`，否则为两个空格
/// - `input_summary`：截断至终端宽度 50% 以内（按字符数计）
/// - `result_summary`：错误时为红色 `✗`，成功时为 dimmed 耗时（如 `0.12s`）
pub fn render_collapsed(
    entry: &ToolCallEntry,
    is_focused: bool,
    caps: &TerminalCapabilities,
) -> String {
    let focus_indicator = if is_focused { "▶" } else { "  " };
    let icon = icon_for(&entry.tool_name);

    // Truncate input_summary to caps.width / 2 visible chars
    let max_summary_chars = (caps.width / 2) as usize;
    let input_summary: String = if entry.input_summary.chars().count() > max_summary_chars {
        let truncated: String = entry
            .input_summary
            .chars()
            .take(max_summary_chars)
            .collect();
        format!("{truncated}…")
    } else {
        entry.input_summary.clone()
    };

    let result_summary = if entry.is_error {
        "✗".truecolor(220, 60, 60).to_string()
    } else {
        let elapsed = format!("{:.2}s", entry.elapsed.as_secs_f32());
        elapsed.dimmed().to_string()
    };

    format!(
        "{} {} {}  {}  ⎿ {}",
        focus_indicator, icon, entry.tool_name, input_summary, result_summary
    )
}

/// 渲染展开状态的完整内容
///
/// 格式：
/// - 第一行：摘要行（带 `▼` 指示符，始终聚焦）
/// - 第二行起：2 空格缩进的 pretty-printed JSON 输入
/// - 分隔线（dimmed）
/// - 输出内容（2 空格缩进）
/// - 超过终端高度 60% 时截断并显示 `... [N more lines]`
pub fn render_expanded(entry: &ToolCallEntry, terminal_width: u16, terminal_height: u16) -> String {
    let max_lines = (terminal_height as usize) * 60 / 100;

    let mut lines: Vec<String> = Vec::new();

    // ── First line: summary with ▼ indicator ──────────────────────────────────
    let icon = icon_for(&entry.tool_name);
    let max_summary_chars = (terminal_width / 2) as usize;
    let input_summary: String = if entry.input_summary.chars().count() > max_summary_chars {
        let truncated: String = entry
            .input_summary
            .chars()
            .take(max_summary_chars)
            .collect();
        format!("{truncated}…")
    } else {
        entry.input_summary.clone()
    };
    let result_summary = if entry.is_error {
        "✗".truecolor(220, 60, 60).to_string()
    } else {
        let elapsed = format!("{:.2}s", entry.elapsed.as_secs_f32());
        elapsed.dimmed().to_string()
    };
    lines.push(format!(
        "▼ {} {}  {}  ⎿ {}",
        icon, entry.tool_name, input_summary, result_summary
    ));

    // ── Input lines: pretty-printed JSON with 2-space indent ─────────────────
    let pretty_input = match serde_json::from_str::<serde_json::Value>(&entry.full_input) {
        Ok(val) => serde_json::to_string_pretty(&val).unwrap_or_else(|_| entry.full_input.clone()),
        Err(_) => entry.full_input.clone(),
    };
    for line in pretty_input.lines() {
        lines.push(format!("  {line}"));
    }

    // ── Separator ─────────────────────────────────────────────────────────────
    lines.push("  ─────────────────".dimmed().to_string());

    // ── Output lines with 2-space indent ─────────────────────────────────────
    for line in entry.output.lines() {
        lines.push(format!("  {line}"));
    }

    // ── Truncation ────────────────────────────────────────────────────────────
    if max_lines > 0 && lines.len() > max_lines {
        let remaining = lines.len() - max_lines;
        lines.truncate(max_lines);
        lines.push(
            format!("  ... [{remaining} more lines]")
                .dimmed()
                .to_string(),
        );
    }

    lines.join("\n")
}

/// 计算一个条目当前占用的行数
///
/// - 折叠状态：始终返回 1
/// - 展开状态：调用 `render_expanded` 并统计行数
pub fn entry_line_count(entry: &ToolCallEntry, terminal_width: u16, terminal_height: u16) -> usize {
    if entry.collapsed {
        1
    } else {
        let rendered = render_expanded(entry, terminal_width, terminal_height);
        rendered.lines().count().max(1)
    }
}

// ── Redraw engine ─────────────────────────────────────────────────────────────

/// 就地重绘：光标移动时只刷新变化的行
///
/// 假设调用时终端光标停在 `new_cursor` 对应条目的第一行（即 `log.cursor` 已更新）。
/// 策略：
/// 1. 计算 `old_cursor` 相对于 `new_cursor` 的行偏移（考虑展开条目占多行）
/// 2. 移动到 `old_cursor` 行，清除并重绘为非聚焦状态
/// 3. 移动到 `new_cursor` 行，清除并重绘为聚焦状态
/// 4. 光标最终停在 `new_cursor` 行
pub fn redraw_cursor_move(
    out: &mut impl Write,
    log: &ToolCallLog,
    old_cursor: usize,
    new_cursor: usize,
    caps: &TerminalCapabilities,
    terminal_width: u16,
    terminal_height: u16,
) -> io::Result<()> {
    if old_cursor == new_cursor {
        return Ok(());
    }

    // Calculate the line offset of old_cursor from new_cursor in the rendered output.
    // We need to know how many terminal lines separate the two entries.
    //
    // Both old_cursor and new_cursor point to the *first* line of their respective entries.
    // We sum up entry_line_count for all entries between them.
    let (lo, hi) = if old_cursor < new_cursor {
        (old_cursor, new_cursor)
    } else {
        (new_cursor, old_cursor)
    };

    // Total lines between lo (inclusive first line) and hi (exclusive first line)
    let lines_between: u16 = log.entries[lo..hi]
        .iter()
        .map(|e| entry_line_count(e, terminal_width, terminal_height) as u16)
        .sum();

    // ── Step 1: move to old_cursor line and redraw as unfocused ──────────────
    if old_cursor < new_cursor {
        // old is above new: move up by lines_between
        queue!(out, cursor::MoveUp(lines_between))?;
    } else {
        // old is below new: move down by lines_between
        queue!(out, cursor::MoveDown(lines_between))?;
    }

    // Clear the current line and redraw old entry as unfocused
    queue!(out, terminal::Clear(terminal::ClearType::CurrentLine))?;
    let old_entry = &log.entries[old_cursor];
    let old_line = if old_entry.collapsed {
        render_collapsed(old_entry, false, caps)
    } else {
        // For expanded entries, only the first (header) line needs focus indicator update
        render_collapsed(
            &ToolCallEntry {
                collapsed: false,
                ..old_entry.clone()
            },
            false,
            caps,
        )
    };
    write!(out, "\r{old_line}")?;

    // ── Step 2: move to new_cursor line and redraw as focused ─────────────────
    if old_cursor < new_cursor {
        // new is below old: move down by lines_between
        queue!(out, cursor::MoveDown(lines_between))?;
    } else {
        // new is above old: move up by lines_between
        queue!(out, cursor::MoveUp(lines_between))?;
    }

    // Clear the current line and redraw new entry as focused
    queue!(out, terminal::Clear(terminal::ClearType::CurrentLine))?;
    let new_entry = &log.entries[new_cursor];
    let new_line = if new_entry.collapsed {
        render_collapsed(new_entry, true, caps)
    } else {
        render_collapsed(
            &ToolCallEntry {
                collapsed: false,
                ..new_entry.clone()
            },
            true,
            caps,
        )
    };
    write!(out, "\r{new_line}")?;

    out.flush()
}

/// 就地重绘：切换折叠/展开状态
///
/// `was_collapsed` 是切换前的状态（现在已经是 `!was_collapsed`）。
///
/// - `展开（was_collapsed` == true，现在展开）：
///   条目从 1 行变为 N 行，需要在当前行下方插入 N-1 行并重绘
/// - `折叠（was_collapsed` == false，现在折叠）：
///   条目从 N 行变为 1 行，需要清除 N-1 行并重绘
pub fn redraw_toggle(
    out: &mut impl Write,
    log: &ToolCallLog,
    index: usize,
    was_collapsed: bool,
    caps: &TerminalCapabilities,
    terminal_width: u16,
    terminal_height: u16,
) -> io::Result<()> {
    let entry = &log.entries[index];
    let is_focused = log.cursor == index;

    if was_collapsed {
        // ── Expanding: was 1 line, now N lines ───────────────────────────────
        // Render the full expanded content
        let expanded = render_expanded(entry, terminal_width, terminal_height);
        let expanded_lines: Vec<&str> = expanded.lines().collect();
        let n = expanded_lines.len();

        // Clear the current (header) line and rewrite the first line with focus indicator
        queue!(out, terminal::Clear(terminal::ClearType::CurrentLine))?;

        // Write all expanded lines, inserting newlines between them
        for (i, line) in expanded_lines.iter().enumerate() {
            if i == 0 {
                // First line: add focus indicator if focused
                if is_focused {
                    write!(out, "\r▼ {}", &line[2..])?; // replace leading "▼ " with focused version
                } else {
                    write!(out, "\r{line}")?;
                }
            } else {
                writeln!(out)?;
                write!(out, "{line}")?;
            }
        }

        // Move cursor back to the first line of this entry
        if n > 1 {
            queue!(out, cursor::MoveUp((n - 1) as u16))?;
        }
    } else {
        // ── Collapsing: was N lines, now 1 line ───────────────────────────────
        // The entry was expanded; calculate how many lines it had before collapsing.
        // Since entry.collapsed is now true (already toggled), we need to compute
        // the old expanded line count by temporarily rendering as expanded.
        let old_expanded = render_expanded(
            &ToolCallEntry {
                collapsed: false,
                ..entry.clone()
            },
            terminal_width,
            terminal_height,
        );
        let old_line_count = old_expanded.lines().count().max(1);

        // Clear the current (first) line and redraw as collapsed
        queue!(out, terminal::Clear(terminal::ClearType::CurrentLine))?;
        let collapsed_line = render_collapsed(entry, is_focused, caps);
        write!(out, "\r{collapsed_line}")?;

        // Clear the remaining N-1 lines below
        for _ in 1..old_line_count {
            queue!(out, cursor::MoveDown(1))?;
            queue!(out, terminal::Clear(terminal::ClearType::CurrentLine))?;
        }

        // Move cursor back up to the first line of this entry
        if old_line_count > 1 {
            queue!(out, cursor::MoveUp((old_line_count - 1) as u16))?;
        }
    }

    out.flush()
}

// ── Existing rendering functions ───────────────────────────────────────────────

/// Map a tool name to a semantic emoji icon.
#[must_use]
pub fn icon_for(tool_name: &str) -> &'static str {
    let lower = tool_name.to_ascii_lowercase();
    if lower.contains("read") || lower.contains("view") || lower.contains("inspect") {
        "📖"
    } else if lower.contains("write") || lower.contains("create") || lower.contains("edit") {
        "✏"
    } else if lower.contains("run")
        || lower.contains("command")
        || lower.contains("shell")
        || lower.contains("workflow")
    {
        "⚡"
    } else if lower.contains("grep") || lower.contains("search") || lower.contains("find") {
        "🔍"
    } else {
        "🔧"
    }
}

/// Print a "tool call in progress" line.
pub fn print_tool_call_line(
    out: &mut impl Write,
    tool_name: &str,
    input_summary: &str,
    _caps: &TerminalCapabilities,
) -> io::Result<()> {
    let icon = icon_for(tool_name);
    writeln!(
        out,
        "  {} {} {}  ...",
        icon,
        tool_name.truecolor(180, 140, 80),
        input_summary.dimmed()
    )?;
    out.flush()
}

/// Print a tool result anchor line.
pub fn print_tool_result_anchor(
    out: &mut impl Write,
    summary: &str,
    is_error: bool,
    _caps: &TerminalCapabilities,
) -> io::Result<()> {
    let prefix = "⎿ ";
    if is_error {
        writeln!(
            out,
            "  {}{} {}",
            prefix,
            "✗".truecolor(220, 60, 60),
            summary.truecolor(220, 60, 60)
        )?;
    } else {
        writeln!(out, "  {}{}", prefix.dimmed(), summary.dimmed())?;
    }
    out.flush()
}

/// Format a "read file" result.
#[must_use]
pub fn format_read_result(path: &str, cwd: &str, lines: usize) -> String {
    let rel = relativize_path(path, cwd);
    format!("Read {lines} lines from {rel}")
}

/// Format a "wrote file" result.
#[must_use]
pub fn format_write_result(path: &str, cwd: &str, lines: usize) -> String {
    let rel = relativize_path(path, cwd);
    format!("Wrote {lines} lines to {rel}")
}

/// Format a "ran command" result.
#[must_use]
pub fn format_run_result(elapsed_secs: f32, exit_code: i32) -> String {
    format!("Ran in {elapsed_secs:.2}s, exit {exit_code}")
}

/// Truncate an error message to 120 chars.
#[must_use]
pub fn truncate_error(msg: &str) -> String {
    if msg.chars().count() > 120 {
        format!("{}…", msg.chars().take(119).collect::<String>())
    } else {
        msg.to_string()
    }
}

// ── Non-interactive mode ───────────────────────────────────────────────────────

/// 检测 stdout 是否为交互终端
///
/// 返回 `true` 表示 stdout 连接到 TTY（交互模式），
/// 返回 `false` 表示 stdout 被重定向到管道或文件（非交互模式）。
#[must_use]
pub fn is_interactive() -> bool {
    std::io::stdout().is_terminal()
}

/// 从字符串中移除 CSI 光标移动转义序列，保留 SGR 颜色序列。
///
/// 移除的序列类型：
/// - `\x1b[<n>A` — 光标上移
/// - `\x1b[<n>B` — 光标下移
/// - `\x1b[<n>C` — 光标右移
/// - `\x1b[<n>D` — 光标左移
/// - `\x1b[<n>;<m>H` — 光标定位
/// - `\x1b[<n>J` — 清屏
/// - `\x1b[<n>K` — 清行
///
/// 保留的序列：
/// - `\x1b[<n>m` — SGR 颜色/样式序列
pub fn strip_cursor_movement_sequences(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '\x1b' {
            if chars.peek() == Some(&'[') {
                // CSI sequence: \x1b[ ... <final_byte>
                let mut seq = String::from("\x1b[");
                chars.next(); // consume '['

                // Collect parameter bytes and intermediate bytes until final byte
                let mut final_byte = None;
                for c in chars.by_ref() {
                    seq.push(c);
                    if c.is_ascii_alphabetic() {
                        final_byte = Some(c);
                        break;
                    }
                }

                // Decide whether to keep or strip based on the final byte
                match final_byte {
                    Some('m') => {
                        // SGR sequence (color/style) — keep it
                        out.push_str(&seq);
                    }
                    Some('A' | 'B' | 'C' | 'D' | 'H' | 'J' | 'K') => {
                        // Cursor movement / clear sequences — strip them
                    }
                    _ => {
                        // Unknown CSI sequence — keep it to be safe
                        out.push_str(&seq);
                    }
                }
            } else {
                // Non-CSI escape — keep the ESC character
                out.push(ch);
            }
        } else {
            out.push(ch);
        }
    }

    out
}

/// 渲染工具调用日志（非交互模式）
///
/// 强制展开所有条目，输出纯文本（无 CSI 光标移动序列）。
/// 用于 stdout 非 TTY 时（管道/重定向）。
///
/// # 行为
/// - 调用 `log.expand_all()` 将所有条目设为展开状态
/// - 对每个条目调用 `render_expanded()` 并剥离光标移动序列
/// - 保留 SGR 颜色序列（如 `\x1b[31m`）
/// - 不启用 raw mode，不监听键盘事件
pub fn render_tool_log_plain(
    log: &mut ToolCallLog,
    out: &mut impl Write,
    terminal_width: u16,
    terminal_height: u16,
) -> io::Result<()> {
    log.expand_all();
    for entry in &log.entries {
        let rendered = render_expanded(entry, terminal_width, terminal_height);
        let clean = strip_cursor_movement_sequences(&rendered);
        writeln!(out, "{clean}")?;
    }
    out.flush()
}

/// 渲染工具调用日志（非交互模式）— 使用默认终端尺寸
///
/// 与 `render_tool_log_plain` 相同功能，使用默认终端尺寸（120x50）。
/// 强制展开所有条目，输出纯文本（无 CSI 光标移动序列）。
pub fn render_tool_log_non_interactive(
    log: &mut ToolCallLog,
    out: &mut impl Write,
) -> io::Result<()> {
    render_tool_log_plain(log, out, 120, 50)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    /// Strip ANSI SGR escape sequences (e.g. `\x1b[31m`) from a string,
    /// returning only the visible text.  Works on the char level so that
    /// multi-byte UTF-8 characters (emoji, etc.) are preserved intact.
    fn strip_ansi(s: &str) -> String {
        let mut out = String::with_capacity(s.len());
        let mut chars = s.chars().peekable();
        while let Some(ch) = chars.next() {
            if ch == '\x1b' {
                // Consume the rest of the escape sequence up to and including
                // the final byte (a letter in the range 0x40–0x7E for CSI).
                if chars.peek() == Some(&'[') {
                    chars.next(); // consume '['
                                  // consume until we hit a letter that ends the sequence
                    for c in chars.by_ref() {
                        if c.is_ascii_alphabetic() {
                            break;
                        }
                    }
                }
                // other escape types (non-CSI) — just skip the ESC itself
            } else {
                out.push(ch);
            }
        }
        out
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        // Feature: tool-call-collapse, Property 1: ToolCallEntry field preservation
        //
        // Validates: Requirements 1.1, 1.2
        #[test]
        fn prop_tool_call_entry_field_preservation(
            tool_name in "[a-zA-Z][a-zA-Z0-9_]{0,20}",
            input_summary in ".*",
            full_input in ".*",
            output in ".*",
            is_error in any::<bool>(),
            elapsed_secs in 0u64..=3600,
        ) {
            let elapsed = Duration::from_secs(elapsed_secs);
            let entry = ToolCallEntry::new(
                tool_name.clone(),
                input_summary.clone(),
                full_input.clone(),
                output.clone(),
                is_error,
                elapsed,
            );
            prop_assert_eq!(&entry.tool_name, &tool_name);
            prop_assert_eq!(&entry.input_summary, &input_summary);
            prop_assert_eq!(&entry.full_input, &full_input);
            prop_assert_eq!(&entry.output, &output);
            prop_assert_eq!(entry.is_error, is_error);
            prop_assert_eq!(entry.elapsed, elapsed);
            prop_assert!(entry.collapsed, "collapsed should default to true");
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        // Feature: tool-call-collapse, Property 3: render_collapsed contains expected components
        //
        // Validates: Requirements 2.1, 2.4, 2.5, 2.6
        #[test]
        fn prop_render_collapsed_contains_expected_components(
            tool_name in "[a-zA-Z][a-zA-Z0-9_]{0,20}",
            input_summary in "[a-zA-Z0-9 ]{0,30}",
            full_input in ".*",
            output in ".*",
            is_error in any::<bool>(),
            elapsed_secs in 0u64..=3600,
            is_focused in any::<bool>(),
        ) {
            let elapsed = Duration::from_secs(elapsed_secs);
            let entry = ToolCallEntry::new(
                tool_name.clone(),
                input_summary.clone(),
                full_input,
                output,
                is_error,
                elapsed,
            );
            let caps = TerminalCapabilities {
                supports_truecolor: false,
                supports_unicode: true,
                supports_sixel: false,
                width: 120,
                height: 40,
            };

            let rendered = render_collapsed(&entry, is_focused, &caps);

            // Strip ANSI escape codes for plain-text assertions
            let plain = strip_ansi(&rendered);

            // The rendered string must contain the icon for the tool
            let expected_icon = icon_for(&tool_name);
            prop_assert!(
                plain.contains(expected_icon),
                "rendered '{}' should contain icon '{}'",
                plain,
                expected_icon
            );

            // The rendered string must contain the tool name
            prop_assert!(
                plain.contains(&tool_name),
                "rendered '{}' should contain tool_name '{}'",
                plain,
                tool_name
            );

            // Error/success indicator
            if is_error {
                prop_assert!(
                    plain.contains('✗'),
                    "rendered '{}' should contain '✗' when is_error=true",
                    plain
                );
            } else {
                // Should contain formatted elapsed time like "0.12s"
                let elapsed_str = format!("{:.2}s", elapsed.as_secs_f32());
                prop_assert!(
                    plain.contains(&elapsed_str),
                    "rendered '{}' should contain elapsed time '{}' when is_error=false",
                    plain,
                    elapsed_str
                );
            }

            // Focus indicator
            if is_focused {
                prop_assert!(
                    plain.contains('▶'),
                    "rendered '{}' should contain '▶' when is_focused=true",
                    plain
                );
            }
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        // Feature: tool-call-collapse, Property 4: input_summary truncation respects terminal width
        //
        // Validates: Requirements 2.3
        #[test]
        fn prop_input_summary_truncation_respects_terminal_width(
            tool_name in "[a-zA-Z][a-zA-Z0-9_]{0,20}",
            // Generate a long input_summary: between 61 and 200 ASCII chars
            input_summary in "[a-zA-Z0-9 ]{61,200}",
            full_input in ".*",
            output in ".*",
            is_error in any::<bool>(),
            elapsed_secs in 0u64..=3600,
            // Terminal width between 60 and 200
            width in 60u16..=200u16,
        ) {
            let elapsed = Duration::from_secs(elapsed_secs);
            let entry = ToolCallEntry::new(
                tool_name.clone(),
                input_summary.clone(),
                full_input,
                output,
                is_error,
                elapsed,
            );
            let caps = TerminalCapabilities {
                supports_truecolor: false,
                supports_unicode: true,
                supports_sixel: false,
                width,
                height: 40,
            };

            let max_summary_chars = (width / 2) as usize;

            let rendered = render_collapsed(&entry, false, &caps);

            // Strip ANSI escape codes
            let plain = strip_ansi(&rendered);

            if input_summary.chars().count() > max_summary_chars {
                // The full input_summary should NOT appear in the rendered output
                prop_assert!(
                    !plain.contains(&input_summary),
                    "rendered '{}' should NOT contain full input_summary '{}' (width={}, max={})",
                    plain,
                    input_summary,
                    width,
                    max_summary_chars
                );

                // The truncated version + ellipsis should appear
                let truncated: String = input_summary.chars().take(max_summary_chars).collect();
                let truncated_with_ellipsis = format!("{}…", truncated);
                prop_assert!(
                    plain.contains(&truncated_with_ellipsis),
                    "rendered '{}' should contain truncated summary '{}' (width={}, max={})",
                    plain,
                    truncated_with_ellipsis,
                    width,
                    max_summary_chars
                );
            }
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        // Feature: tool-call-collapse, Property 2: ToolCallLog operation invariants
        //
        // Validates: Requirements 1.3, 1.4, 4.1, 4.2
        #[test]
        fn prop_tool_call_log_operation_invariants(
            // Generate a sequence of operations: 0=push, 1=toggle_cursor, 2=move_up, 3=move_down
            ops in proptest::collection::vec(0u8..4, 1..=30),
            // Tool names for push operations
            tool_names in proptest::collection::vec("[a-zA-Z][a-zA-Z0-9_]{0,10}", 30),
        ) {
            let mut log = ToolCallLog::new();
            let mut name_idx = 0usize;

            // Invariant: starts empty with cursor == 0
            prop_assert_eq!(log.cursor, 0);
            prop_assert!(log.is_empty());

            for op in &ops {
                match op {
                    0 => {
                        // push
                        let name = tool_names[name_idx % tool_names.len()].clone();
                        name_idx += 1;
                        let before_len = log.entries.len();
                        let entry = ToolCallEntry::new(
                            name,
                            String::new(),
                            String::new(),
                            String::new(),
                            false,
                            Duration::from_secs(0),
                        );
                        log.push(entry);
                        // push increases entries.len() by 1
                        prop_assert_eq!(log.entries.len(), before_len + 1);
                    }
                    1 => {
                        // toggle at cursor position
                        if !log.entries.is_empty() {
                            let idx = log.cursor;
                            let before = log.entries[idx].collapsed;
                            log.toggle(idx);
                            let after = log.entries[idx].collapsed;
                            // toggle flips collapsed state
                            prop_assert_ne!(before, after, "toggle should flip collapsed");
                        }
                    }
                    2 => {
                        // move_cursor_up
                        log.move_cursor_up();
                    }
                    3 => {
                        // move_cursor_down
                        log.move_cursor_down();
                    }
                    _ => unreachable!(),
                }

                // Cursor invariant: always valid
                if log.entries.is_empty() {
                    prop_assert_eq!(log.cursor, 0, "cursor must be 0 when entries is empty");
                } else {
                    prop_assert!(
                        log.cursor < log.entries.len(),
                        "cursor {} must be < entries.len() {}",
                        log.cursor,
                        log.entries.len()
                    );
                }
            }
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        // Feature: tool-call-collapse, Property 5: render_expanded first line consistency
        //
        // Validates: Requirements 3.1
        #[test]
        fn prop_render_expanded_first_line_consistency(
            tool_name in "[a-zA-Z][a-zA-Z0-9_]{0,20}",
            input_summary in "[a-zA-Z0-9 ]{0,30}",
            full_input in ".*",
            output in ".*",
            is_error in any::<bool>(),
            elapsed_secs in 0u64..=3600,
        ) {
            let elapsed = Duration::from_secs(elapsed_secs);
            let entry = ToolCallEntry::new(
                tool_name.clone(),
                input_summary.clone(),
                full_input,
                output,
                is_error,
                elapsed,
            );

            let rendered = render_expanded(&entry, 120, 40);
            let first_line = rendered.lines().next().unwrap();
            let plain = strip_ansi(first_line);

            // First line must contain tool_name
            prop_assert!(
                plain.contains(&tool_name),
                "first line '{}' should contain tool_name '{}'",
                plain,
                tool_name
            );

            // First line must use "▼" as the indicator
            prop_assert!(
                plain.contains('▼'),
                "first line '{}' should contain '▼'",
                plain
            );

            // First line must NOT contain "▶"
            prop_assert!(
                !plain.contains('▶'),
                "first line '{}' should NOT contain '▶'",
                plain
            );
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        // Feature: tool-call-collapse, Property 6: render_expanded contains formatted input
        //
        // Validates: Requirements 3.2
        #[test]
        fn prop_render_expanded_contains_formatted_input(
            key_value in "[a-zA-Z0-9]{1,20}",
            tool_name in "[a-zA-Z][a-zA-Z0-9_]{0,20}",
            input_summary in "[a-zA-Z0-9 ]{0,30}",
            output in ".*",
            is_error in any::<bool>(),
            elapsed_secs in 0u64..=3600,
        ) {
            let elapsed = Duration::from_secs(elapsed_secs);
            // Construct valid JSON as full_input
            let full_input = format!("{{\"key\": \"{}\"}}", key_value);
            let entry = ToolCallEntry::new(
                tool_name,
                input_summary,
                full_input,
                output,
                is_error,
                elapsed,
            );

            let rendered = render_expanded(&entry, 120, 40);
            let plain = strip_ansi(&rendered);

            // The rendered output should contain the JSON key with indentation
            // serde_json::to_string_pretty produces `  "key": "value"` style
            prop_assert!(
                plain.contains("\"key\""),
                "rendered output should contain '\"key\"' from pretty-printed JSON, got: {}",
                plain
            );
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        // Feature: tool-call-collapse, Property 7: render_expanded output truncation
        //
        // Validates: Requirements 3.4
        #[test]
        fn prop_render_expanded_output_truncation(
            tool_name in "[a-zA-Z][a-zA-Z0-9_]{0,20}",
            input_summary in "[a-zA-Z0-9 ]{0,20}",
            is_error in any::<bool>(),
            elapsed_secs in 0u64..=3600,
            terminal_height in 10u16..=40u16,
            // Extra lines beyond the limit (1..=20 extra lines)
            extra_lines in 1usize..=20,
        ) {
            let elapsed = Duration::from_secs(elapsed_secs);
            let max_lines = (terminal_height as usize) * 60 / 100;

            // Build an output with enough lines to exceed max_lines.
            // The rendered output includes: 1 header line + input lines + 1 separator + output lines.
            // To be safe, generate output lines = max_lines + extra_lines (well above the limit).
            let total_output_lines = max_lines + extra_lines;
            let output = (0..total_output_lines)
                .map(|i| format!("output line {}", i))
                .collect::<Vec<_>>()
                .join("\n");

            let entry = ToolCallEntry::new(
                tool_name,
                input_summary,
                String::from("{}"),  // valid minimal JSON
                output,
                is_error,
                elapsed,
            );

            let rendered = render_expanded(&entry, 120, terminal_height);
            let plain = strip_ansi(&rendered);

            // The rendered output must contain the truncation indicator
            prop_assert!(
                plain.contains("more lines]"),
                "rendered output should contain 'more lines]' truncation indicator (terminal_height={}, max_lines={}), got:\n{}",
                terminal_height,
                max_lines,
                plain
            );
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        // Feature: tool-call-collapse, Property 9: Non-interactive mode forces expand
        //
        // Validates: Requirements 6.2
        #[test]
        fn prop_expand_all_forces_all_collapsed_false(
            num_entries in 0usize..=20,
        ) {
            let mut log = ToolCallLog::new();
            for i in 0..num_entries {
                log.push(ToolCallEntry::new(
                    format!("tool_{}", i),
                    String::new(),
                    String::new(),
                    String::new(),
                    false,
                    Duration::from_secs(0),
                ));
            }
            log.expand_all();
            for entry in &log.entries {
                prop_assert!(!entry.collapsed, "all entries should be expanded after expand_all()");
            }
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        // Feature: tool-call-collapse, Property 10: Non-interactive rendering has no cursor-movement sequences
        //
        // Validates: Requirements 6.4
        #[test]
        fn prop_non_interactive_rendering_no_csi_cursor_movement(
            tool_name in "[a-zA-Z][a-zA-Z0-9_]{0,10}",
            input_summary in "[a-zA-Z0-9 ]{0,30}",
            full_input in "\\{\"[a-z]{1,5}\": \"[a-z]{1,10}\"\\}",
            output in "[a-zA-Z0-9 \n]{0,100}",
            is_error in any::<bool>(),
            elapsed_secs in 0u64..=60,
        ) {
            let mut log = ToolCallLog::new();
            log.push(ToolCallEntry::new(
                tool_name,
                input_summary,
                full_input,
                output,
                is_error,
                Duration::from_secs(elapsed_secs),
            ));

            let mut buf = Vec::new();
            render_tool_log_plain(&mut log, &mut buf, 120, 50).unwrap();
            let rendered = String::from_utf8_lossy(&buf);

            // Check for CSI cursor movement sequences
            // These are \x1b[ followed by digits/semicolons then A/B/C/D/H/J/K
            let has_cursor_movement = rendered.contains("\x1b[") && {
                let mut found = false;
                let bytes = rendered.as_bytes();
                let mut i = 0;
                while i < bytes.len().saturating_sub(2) {
                    if bytes[i] == 0x1b && bytes[i + 1] == b'[' {
                        i += 2;
                        // Skip parameter bytes (digits and semicolons)
                        while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b';') {
                            i += 1;
                        }
                        if i < bytes.len()
                            && matches!(bytes[i], b'A' | b'B' | b'C' | b'D' | b'H' | b'J' | b'K')
                        {
                            found = true;
                            break;
                        }
                    }
                    i += 1;
                }
                found
            };

            prop_assert!(
                !has_cursor_movement,
                "non-interactive rendering should not contain CSI cursor movement sequences"
            );
        }
    }

    #[test]
    fn icon_read() {
        assert_eq!(icon_for("ReadFile"), "📖");
        assert_eq!(icon_for("view"), "📖");
        assert_eq!(icon_for("inspect"), "📖");
    }

    #[test]
    fn icon_write() {
        assert_eq!(icon_for("WriteFile"), "✏");
        assert_eq!(icon_for("create"), "✏");
        assert_eq!(icon_for("edit"), "✏");
    }

    #[test]
    fn icon_run() {
        assert_eq!(icon_for("RunCommand"), "⚡");
        assert_eq!(icon_for("shell"), "⚡");
    }

    #[test]
    fn icon_search() {
        assert_eq!(icon_for("grep"), "🔍");
        assert_eq!(icon_for("search"), "🔍");
    }

    #[test]
    fn icon_default() {
        assert_eq!(icon_for(""), "🔧");
        assert_eq!(icon_for("some_unknown_tool"), "🔧");
    }

    #[test]
    fn icon_case_insensitive() {
        assert_eq!(icon_for("ReadFile"), icon_for("readfile"));
        assert_eq!(icon_for("READ"), icon_for("read"));
    }

    #[test]
    fn truncate_error_short() {
        assert_eq!(truncate_error("short"), "short");
    }

    #[test]
    fn truncate_error_long() {
        let long = "x".repeat(200);
        let truncated = truncate_error(&long);
        assert!(truncated.chars().count() <= 120);
    }
}
