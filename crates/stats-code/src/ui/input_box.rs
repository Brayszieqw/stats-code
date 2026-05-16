use std::io::{self, Write};

use colored::Colorize;

use super::caps::TerminalCapabilities;
use super::ChatUiStatus;

/// Print the top frame of the input box.
///
/// Unicode: `╭─ chat ─── / commands  ! shell  Enter send  Ctrl+D exit ──╮`
/// ASCII:   `+- chat --- / commands  ! shell  Enter send  Ctrl+D exit --+`
pub fn print_top(out: &mut impl Write, caps: &TerminalCapabilities) -> io::Result<()> {
    let width = caps.width as usize;
    let left_text = " chat ";
    let right_text = " / commands  ! shell  Enter send  Ctrl+D exit ";
    let fixed_len = 2 + left_text.len() + right_text.len(); // corners + texts
    let fill = width.saturating_sub(fixed_len);
    let (tl, tr, h) = if caps.supports_unicode {
        ('╭', '╮', '─')
    } else {
        ('+', '+', '-')
    };
    let bar: String = std::iter::repeat_n(h, fill).collect();
    // Enhanced visual: "chat" label in bright color, hints dimmed, border in subtle gray
    let border_color = (80, 80, 90);
    let tl_s = format!("{tl}").truecolor(border_color.0, border_color.1, border_color.2);
    let tr_s = format!("{tr}").truecolor(border_color.0, border_color.1, border_color.2);
    let bar_s = bar.truecolor(border_color.0, border_color.1, border_color.2);
    let label = left_text.truecolor(200, 180, 100).bold();
    let hints = right_text.truecolor(100, 100, 95);
    write!(out, "{tl_s}{label}{bar_s}{hints}{tr_s}")?;
    writeln!(out)?;
    out.flush()
}

/// Print the bottom frame of the input box, optionally with model/token/cost info.
pub fn print_bottom(
    out: &mut impl Write,
    caps: &TerminalCapabilities,
    status: Option<&ChatUiStatus>,
) -> io::Result<()> {
    let width = caps.width as usize;
    let (bl, br, h) = if caps.supports_unicode {
        ('╰', '╯', '─')
    } else {
        ('+', '+', '-')
    };

    let inner = match status {
        Some(s) if s.input_tokens > 0 || s.output_tokens > 0 => {
            let cost_part = match s.estimated_cost_usd {
                Some(c) => format!(" cost=${c:.4}"),
                None => String::new(),
            };
            format!(
                " {} · tokens={}/{}{} ",
                s.model, s.input_tokens, s.output_tokens, cost_part
            )
        }
        _ => String::from(" "),
    };

    let fill = width.saturating_sub(2 + inner.chars().count());
    let bar: String = std::iter::repeat_n(h, fill).collect();
    let border_color = (80, 80, 90);
    let bl_s = format!("{bl}").truecolor(border_color.0, border_color.1, border_color.2);
    let br_s = format!("{br}").truecolor(border_color.0, border_color.1, border_color.2);
    let bar_s = bar.truecolor(border_color.0, border_color.1, border_color.2);
    let inner_s = inner.truecolor(130, 130, 120);
    write!(out, "{bl_s}{inner_s}{bar_s}{br_s}")?;
    writeln!(out)?;
    out.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_caps(unicode: bool) -> TerminalCapabilities {
        TerminalCapabilities {
            supports_truecolor: true,
            supports_unicode: unicode,
            supports_sixel: false,
            width: 80,
            height: 40,
        }
    }

    #[test]
    fn top_with_unicode() {
        let mut buf = Vec::new();
        print_top(&mut buf, &test_caps(true)).unwrap();
        let s = String::from_utf8_lossy(&buf);
        assert!(s.contains('╭'), "missing unicode corner: {s}");
        assert!(s.contains('╮'), "missing unicode corner: {s}");
    }

    #[test]
    fn top_with_ascii() {
        let mut buf = Vec::new();
        print_top(&mut buf, &test_caps(false)).unwrap();
        let s = String::from_utf8_lossy(&buf);
        assert!(s.contains('+'), "missing ascii corner: {s}");
    }

    #[test]
    fn bottom_zero_tokens() {
        let mut buf = Vec::new();
        print_bottom(&mut buf, &test_caps(true), None).unwrap();
        let s = String::from_utf8_lossy(&buf);
        assert!(s.contains('╰'), "missing corner: {s}");
    }

    #[test]
    fn bottom_with_tokens() {
        let status = ChatUiStatus {
            model: "gpt-5".into(),
            workspace: String::new(),
            tools_enabled: true,
            fast_mode: false,
            vim_mode: false,
            turns: 1,
            input_tokens: 100,
            output_tokens: 200,
            estimated_cost_usd: Some(0.0015),
            session_loaded: false,
        };
        let mut buf = Vec::new();
        print_bottom(&mut buf, &test_caps(true), Some(&status)).unwrap();
        let s = String::from_utf8_lossy(&buf);
        assert!(s.contains("100"), "missing input tokens: {s}");
        assert!(s.contains("200"), "missing output tokens: {s}");
        assert!(s.contains("cost"), "missing cost: {s}");
    }
}
