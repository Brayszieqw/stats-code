use std::io::{self, Write};

use colored::Colorize;

use crate::gugugaga_art::{print_gugugaga_image, SIXEL_COLS};

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

#[derive(Debug, Default)]
pub struct ChatUi {}

impl ChatUi {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 打印欢迎头部：咕咕嘎嘎图片（左）+ 信息（右）并排布局 + 底部横线
    ///
    /// 布局策略：Sixel 输出后光标在图片底部，向上移 N 行、逐行在右侧打印 N 行文字，
    /// 光标自动回到图片底部——不依赖精确的图片行数估算，杜绝光标偏移问题。
    pub fn print_welcome(&self, out: &mut impl Write, status: &ChatUiStatus) -> io::Result<()> {
        let width = self.term_width();

        // ── 1. 输出 Sixel 企鹅图片（直接写 stdout）──
        print_gugugaga_image();

        // ── 2. 准备右侧文本内容 ──
        let col = SIXEL_COLS + 3; // 图片宽(字符列) + 间距
        let right_w = width.saturating_sub(col + 1); // 右侧可用宽度

        let session_str = if status.session_loaded {
            "resumed".truecolor(90, 180, 120).to_string()
        } else {
            "new".truecolor(120, 140, 200).to_string()
        };

        // 装饰线 ════ 填满右侧
        let sep = "═".repeat(right_w).truecolor(100, 100, 115).to_string();

        // 标题行
        let title = format!(
            "  {} {}  {}",
            "★".truecolor(255, 210, 50),
            "Stats Code 咕咕嘎嘎版".truecolor(235, 120, 60).bold(),
            session_str
        );

        // 三行信息（用户指定顺序）
        let line_model = format!(
            "  {}  {}",
            "model=".truecolor(150, 150, 150),
            status.model.truecolor(200, 200, 255).bold()
        );
        let line_workspace = format!(
            "  {}  {}",
            "workspace=".truecolor(150, 150, 150),
            status.workspace.truecolor(180, 220, 200)
        );
        let line_tools = format!(
            "  tools={}   fast={}   vim={}",
            on_off_colored(status.tools_enabled),
            on_off_colored(status.fast_mode),
            on_off_colored(status.vim_mode)
        );

        // 右侧行列表（10 行，位于图片底部区域）
        let lines: Vec<&str> = vec![
            &sep,           // ═══ 顶部装饰线
            "",
            &title,         // ★ Stats Code 咕咕嘎嘎版  resumed
            "",
            &line_model,    // model= gpt-5.4
            &line_workspace,// workspace= C:\Users\ljx
            &line_tools,    // tools=on  fast=off  vim=off
            "",
            &sep,           // ═══ 底部装饰线
        ];

        let n = lines.len(); // 总行数

        // ── 3. 光标上移 n 行（从图片底部向上）──
        // 这样打印完 n 行后光标自动回到图片底部，不会偏移
        let _ = write!(out, "\x1b[{n}A");
        let _ = out.flush();

        // ── 4. 逐行在右侧打印 ──
        for text in &lines {
            // \r 确保在行首，然后右移 col 列
            let _ = write!(out, "\r\x1b[{col}C{text}");
            let _ = writeln!(out);
        }
        let _ = out.flush();

        // ── 5. 底部分隔横线（全宽）──
        let bottom = "─".repeat(width).truecolor(70, 70, 80);
        writeln!(out, "{bottom}")?;
        out.flush()
    }

    /// 打印对话轮次（无任何边框，纯净类 Claude Code 风格）
    pub fn print_turn(&self, out: &mut impl Write, kind: ChatEntryKind, body: &str) -> io::Result<()> {
        let (label, color) = match kind {
            ChatEntryKind::User      => ("You",   (240, 200, 120)),
            ChatEntryKind::Assistant => ("Stats", (120, 210, 140)),
            ChatEntryKind::System    => ("Info",  (130, 170, 220)),
            ChatEntryKind::Tool      => ("Tool",  (180, 150, 100)),
            ChatEntryKind::Error     => ("Error", (220, 90,  90)),
        };

        // 彩色标签，不带任何 │
        let label_str = label
            .truecolor(color.0, color.1, color.2)
            .bold()
            .to_string();
        writeln!(out, "{label_str}")?;

        let width = self.term_width().saturating_sub(4);
        let mut in_code_block = false;

        for raw_line in body.lines() {
            if raw_line.trim_start().starts_with("```") {
                in_code_block = !in_code_block;
                let fence = raw_line.truecolor(100, 100, 120).to_string();
                writeln!(out, "  {fence}")?;
            } else if in_code_block {
                // 代码块：蓝白高亮
                writeln!(out, "  {}", raw_line.truecolor(180, 220, 255))?;
            } else if raw_line.chars().count() > width {
                // 长文本自动折行
                for w_line in wrap_text(raw_line, width) {
                    writeln!(out, "  {w_line}")?;
                }
            } else {
                writeln!(out, "  {raw_line}")?;
            }
        }
        writeln!(out)?;
        out.flush()
    }

    /// 打印输入提示前的状态栏：上下两条横线，内容字在中间（Claude Code 对话框三明治风格）
    ///
    /// 布局：
    ///   ──────────────────────────────  (上线)
    ///   `/` commands · `!` shell          tokens=.../...  (中间状态行)
    ///   > [rustyline 渲染输入光标在此处]
    /// > ──────────────────────────────  (下线，实际上不在这里单独输出，由 print_input_bottom 输出)
    pub fn print_status_bar(
        &self,
        out: &mut impl Write,
        status: &ChatUiStatus,
        pending: Option<&str>,
    ) -> io::Result<()> {
        let width = self.term_width();
        let line = "─".repeat(width).truecolor(70, 70, 80).to_string();

        // 上横线
        writeln!(out, "{line}")?;

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

            // 计算纯文本长度（不含 ANSI 码）
            let left_visible = 50usize;
            let right_visible = right_plain_len(status, &cost);
            let pad = width
                .saturating_sub(left_visible)
                .saturating_sub(right_visible);
            writeln!(out, "{}{}{}", left, " ".repeat(pad), right)?;
        }

        // 注意：不在这里打印下横线，下横线留给 print_input_bottom 在用户按 Enter 后输出
        out.flush()
    }

    /// 用户 Enter 后打印回车下横线（完成对话框闭合）
    pub fn print_input_bottom(&self, out: &mut impl Write) -> io::Result<()> {
        let width = self.term_width();
        let line = "─".repeat(width).truecolor(70, 70, 80).to_string();
        writeln!(out, "{line}")?;
        out.flush()
    }

    fn term_width(&self) -> usize {
        match crossterm::terminal::size() {
            Ok((w, _)) => (w as usize).max(60),
            Err(_) => 80,
        }
    }
}

fn on_off_colored(value: bool) -> String {
    if value {
        "on".truecolor(90, 200, 120).to_string()
    } else {
        "off".truecolor(150, 150, 150).to_string()
    }
}

fn right_plain_len(status: &ChatUiStatus, cost: &str) -> usize {
    // rough estimate of visible chars in right side string
    format!(
        "tokens={}/{} · ⊙ auto{}",
        status.input_tokens, status.output_tokens, cost
    )
    .chars()
    .count()
}

fn wrap_text(text: &str, width: usize) -> Vec<String> {
    if text.is_empty() {
        return Vec::new();
    }
    let max_width = width.max(12);
    let mut lines = Vec::new();
    let mut current = String::new();

    for word in text.split_whitespace() {
        if current.is_empty() {
            if word.chars().count() > max_width {
                lines.extend(split_long_word(word, max_width));
            } else {
                current.push_str(word);
            }
            continue;
        }
        let next_width = current.chars().count() + 1 + word.chars().count();
        if next_width <= max_width {
            current.push(' ');
            current.push_str(word);
        } else {
            lines.push(current);
            current = String::new();
            if word.chars().count() > max_width {
                lines.extend(split_long_word(word, max_width));
            } else {
                current.push_str(word);
            }
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

fn split_long_word(word: &str, width: usize) -> Vec<String> {
    let mut chunk = String::new();
    let mut lines = Vec::new();
    for ch in word.chars() {
        if chunk.chars().count() >= width {
            lines.push(chunk);
            chunk = String::new();
        }
        chunk.push(ch);
    }
    if !chunk.is_empty() {
        lines.push(chunk);
    }
    lines
}
