use std::env;

#[derive(Debug, Clone)]
pub struct EnvSnapshot {
    pub wt_session: Option<String>,
    pub term_program: Option<String>,
    pub term: Option<String>,
    pub colorterm: Option<String>,
    pub is_windows: bool,
    pub term_size: (u16, u16),
}

impl EnvSnapshot {
    pub fn current() -> Self {
        let term_size = crossterm::terminal::size().unwrap_or((80, 24));
        Self {
            wt_session: env::var("WT_SESSION").ok(),
            term_program: env::var("TERM_PROGRAM").ok(),
            term: env::var("TERM").ok(),
            colorterm: env::var("COLORTERM").ok(),
            is_windows: cfg!(windows),
            term_size,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TerminalCapabilities {
    pub supports_truecolor: bool,
    pub supports_unicode: bool,
    pub supports_sixel: bool,
    pub width: u16,
    pub height: u16,
}

impl TerminalCapabilities {
    pub fn detect() -> Self {
        Self::detect_from(&EnvSnapshot::current())
    }

    pub fn detect_from(env: &EnvSnapshot) -> Self {
        let (width, height) = env.term_size;

        if let Some(ref wt) = env.wt_session {
            if !wt.is_empty() {
                return Self {
                    supports_truecolor: true,
                    supports_unicode: true,
                    supports_sixel: true,
                    width,
                    height,
                };
            }
        }

        if let Some(ref tp) = env.term_program {
            if tp.eq_ignore_ascii_case("vscode") {
                return Self {
                    supports_truecolor: true,
                    supports_unicode: true,
                    supports_sixel: false,
                    width,
                    height,
                };
            }
        }

        if !env.is_windows {
            return Self {
                supports_truecolor: true,
                supports_unicode: true,
                supports_sixel: false,
                width,
                height,
            };
        }

        Self {
            supports_truecolor: false,
            supports_unicode: false,
            supports_sixel: false,
            width,
            height,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(wt_session: Option<&str>, term_program: Option<&str>, is_windows: bool) -> EnvSnapshot {
        EnvSnapshot {
            wt_session: wt_session.map(String::from),
            term_program: term_program.map(String::from),
            term: None,
            colorterm: None,
            is_windows,
            term_size: (120, 40),
        }
    }

    #[test]
    fn detects_windows_terminal() {
        let caps = TerminalCapabilities::detect_from(&snap(Some("abc123"), None, true));
        assert!(caps.supports_truecolor);
        assert!(caps.supports_unicode);
        assert!(caps.supports_sixel);
    }

    #[test]
    fn detects_vscode() {
        let caps = TerminalCapabilities::detect_from(&snap(None, Some("vscode"), true));
        assert!(caps.supports_truecolor);
        assert!(caps.supports_unicode);
        assert!(!caps.supports_sixel);
    }

    #[test]
    fn windows_fallback_no_wt() {
        let caps = TerminalCapabilities::detect_from(&snap(None, None, true));
        assert!(!caps.supports_truecolor);
        assert!(!caps.supports_unicode);
        assert!(!caps.supports_sixel);
    }

    #[test]
    fn non_windows_defaults() {
        let caps = TerminalCapabilities::detect_from(&snap(None, None, false));
        assert!(caps.supports_truecolor);
        assert!(caps.supports_unicode);
        assert!(!caps.supports_sixel);
    }

    #[test]
    fn width_height_preserved() {
        let caps = TerminalCapabilities::detect_from(&snap(Some("wt"), None, true));
        assert_eq!(caps.width, 120);
        assert_eq!(caps.height, 40);
    }
}
