use std::fmt::Write as _;
use std::io::{self, Write};

use colored::Colorize;

use super::caps::TerminalCapabilities;
use super::gradient;
use super::ChatUiStatus;

const STATS_CODE_LOGO: &str = r"
  ███████╗████████╗ █████╗ ████████╗███████╗
  ██╔════╝╚══██╔══╝██╔══██╗╚══██╔══╝██╔════╝
  ███████╗   ██║   ███████║   ██║   ███████╗
  ╚════██║   ██║   ██╔══██║   ██║   ╚════██║
  ███████║   ██║   ██║  ██║   ██║   ███████║
  ╚══════╝   ╚═╝   ╚═╝  ╚═╝   ╚═╝   ╚══════╝
   ██████╗ ██████╗ ██████╗ ███████╗
  ██╔════╝██╔═══██╗██╔══██╗██╔════╝
  ██║     ██║   ██║██║  ██║█████╗
  ██║     ██║   ██║██║  ██║██╔══╝
  ╚██████╗╚██████╔╝██████╔╝███████╗
   ╚═════╝ ╚═════╝ ╚═════╝ ╚══════╝";

/// Returns the width in chars of the widest line in the ASCII art.
fn logo_max_width() -> usize {
    STATS_CODE_LOGO
        .lines()
        .map(|line| line.chars().count())
        .max()
        .unwrap_or(0)
}

const GRADIENT_START: (u8, u8, u8) = (0x00, 0xD9, 0xFF); // cyan
const GRADIENT_END: (u8, u8, u8) = (0xA8, 0x55, 0xF7); // purple

pub fn render_welcome(
    out: &mut impl Write,
    caps: &TerminalCapabilities,
    status: &ChatUiStatus,
) -> io::Result<()> {
    let width = caps.width as usize;

    if width >= logo_max_width() {
        let logo_lines: Vec<&str> = STATS_CODE_LOGO.lines().collect();
        let max_cols: Vec<usize> = logo_lines.iter().map(|l| l.chars().count()).collect();
        let global_max = max_cols.iter().copied().max().unwrap_or(40);

        for (row, line) in logo_lines.iter().enumerate() {
            let line_len = max_cols[row];
            if line_len == 0 {
                writeln!(out)?;
                continue;
            }

            let mut rendered = String::new();
            for (col, ch) in line.chars().enumerate() {
                if ch == ' ' || ch == '\t' {
                    rendered.push(' ');
                } else {
                    let t = if global_max > 1 {
                        col as f32 / (global_max - 1) as f32
                    } else {
                        0.5
                    };
                    let rgb = gradient::interpolate_rgb(GRADIENT_START, GRADIENT_END, t);
                    if caps.supports_truecolor {
                        let _ = write!(
                            rendered,
                            "\x1b[38;2;{};{};{}m{}\x1b[0m",
                            rgb.0, rgb.1, rgb.2, ch
                        );
                    } else {
                        let idx = gradient::quantize_to_256(rgb);
                        let _ = write!(rendered, "\x1b[38;5;{idx}m{ch}\x1b[0m");
                    }
                }
            }
            writeln!(out, "{rendered}")?;
        }
    }

    // System info
    let session_str = if status.session_loaded {
        "resumed".truecolor(90, 180, 120).to_string()
    } else {
        "new".truecolor(120, 140, 200).to_string()
    };
    writeln!(
        out,
        "  {} Stats Code  {}",
        "★".truecolor(255, 210, 50),
        session_str
    )?;
    writeln!(
        out,
        "  {} {}",
        "model=".truecolor(150, 150, 150),
        status.model.truecolor(200, 200, 255).bold()
    )?;
    writeln!(
        out,
        "  {} {}",
        "workspace=".truecolor(150, 150, 150),
        status.workspace.truecolor(180, 220, 200)
    )?;
    writeln!(
        out,
        "  tools={}   fast={}   vim={}",
        if status.tools_enabled {
            "on".truecolor(90, 200, 120)
        } else {
            "off".truecolor(150, 150, 150)
        },
        if status.fast_mode {
            "on".truecolor(90, 200, 120)
        } else {
            "off".truecolor(150, 150, 150)
        },
        if status.vim_mode {
            "on".truecolor(90, 200, 120)
        } else {
            "off".truecolor(150, 150, 150)
        },
    )?;
    let sep = "─".repeat(width).truecolor(70, 70, 80);
    writeln!(out, "{sep}")?;
    out.flush()
}
