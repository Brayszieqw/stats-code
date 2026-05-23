//! Launcher 命令行参数定义。
//!
//! 当 `stats-code` 在无子命令模式下被调用时，由本模块负责解析公开旗标。
//! 设计文档（`.kiro/specs/single-command-launcher/design.md`）要求公开 CLI
//! 仅暴露三个旗标：`--version`、`--help`、`--no-browser`。其中 `--version`
//! 与 `--help` 由 clap 通过 `#[command(version)]` 内置派发，故此处仅显式
//! 声明 `--no-browser` 一个布尔旗标。
//!
//! 本结构体与 [`crate::cli::Cli`] 是两套独立的 clap 解析器：
//! - `Cli` 用于 `SkillInvoker::StatsCli` 的子命令调用路径，所有内部子命令
//!   均带 `#[command(hide = true)]`，仍可解析但在 `--help` 中不可见。
//! - `LauncherArgs` 用于 `main` 在 `classify_invocation` 判定为 Launcher
//!   模式时的 argv 解析，只接受 `--no-browser`。
//!
//! 这种「双解析器、按 argv 分流」的做法见设计文档 §Architecture
//! "Subcommand fast-path"。

use clap::Parser;

/// 用户敲 `stats-code` 无子命令时使用的旗标集合。
///
/// 仅承载 `--no-browser`：`--version` / `--help` 由 clap 在 `#[command]`
/// 上自动派发。
///
/// _Validates Requirement 2.3_。
#[derive(Debug, Default, Clone, Copy, Parser)]
#[command(version, name = "stats-code")]
pub struct LauncherArgs {
    /// 不调用系统默认浏览器，仅在 stdout 打印可访问的 URL。
    #[arg(long = "no-browser")]
    pub no_browser: bool,
}

impl LauncherArgs {
    /// 创建一个默认参数集（`--no-browser` 关闭）。
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 从原始 argv 切片中提取 launcher 旗标。
    ///
    /// 使用简单的字符串匹配而非 clap 完整解析，以避免 clap 因未识别的
    /// 参数（如 `--version` / `--help`）自动退出进程。`--version` 和
    /// `--help` 由 clap 在子命令路径中处理。
    #[must_use]
    pub fn from_argv(argv: &[String]) -> Self {
        let no_browser = argv.iter().any(|a| a == "--no-browser");
        Self { no_browser }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    /// 默认 argv（仅包含程序名）应解析为 `no_browser = false`。
    #[test]
    fn parses_default_argv_with_no_flag() {
        let args = LauncherArgs::try_parse_from(["stats-code"]).expect("parse default argv");
        assert!(
            !args.no_browser,
            "expected no_browser=false when --no-browser absent"
        );
    }

    /// `--no-browser` 应将 `no_browser` 置为 `true`。
    #[test]
    fn parses_no_browser_flag() {
        let args = LauncherArgs::try_parse_from(["stats-code", "--no-browser"])
            .expect("parse --no-browser argv");
        assert!(
            args.no_browser,
            "expected no_browser=true when --no-browser provided"
        );
    }

    /// 公开 CLI 仅承认 `--no-browser`：任何其他位置参数或未知旗标都应被
    /// clap 拒绝。这同时构成对「公开命令行接口仅保留三个旗标」（设计文档
    /// Requirement 2）的局部回归。
    #[test]
    fn rejects_unknown_flag() {
        let err = LauncherArgs::try_parse_from(["stats-code", "--definitely-not-a-flag"])
            .expect_err("unknown flag must be rejected");
        // clap 会区分 "unknown argument" 与 "missing value"，此处只关心解析失败。
        assert!(
            !err.to_string().is_empty(),
            "clap error message must not be empty"
        );
    }

    /// `--help` 输出只暴露公开 launcher 旗标，不包含内部统计算法子命令名
    /// 或内部 `Cli` 全局旗标。这道断言保证两个解析器之间不会因为复制粘贴
    /// 导致用户面对实现细节。
    #[test]
    fn help_lists_only_public_launcher_flags() {
        let mut cmd = LauncherArgs::command();
        let help = cmd.render_help().to_string();

        for public_flag in ["--no-browser", "--help", "--version"] {
            assert!(
                help.contains(public_flag),
                "help must document public flag `{public_flag}`, got:\n{help}"
            );
        }

        let forbidden_tokens = include_str!("../../tests/fixtures/launcher/help_negative.txt");
        for hidden in forbidden_tokens
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
        {
            if hidden == "stats" {
                assert!(
                    !help.contains("  stats"),
                    "LauncherArgs help must not list hidden `stats` subcommand, got:\n{help}"
                );
                continue;
            }
            assert!(
                !help.contains(hidden),
                "LauncherArgs help must not mention internal subcommand `{hidden}`, got:\n{help}"
            );
        }

        for internal_flag in [
            "--json",
            "--artifacts-dir",
            "--session",
            "--model",
            "--system",
            "--max-tokens",
            "--engine",
            "--alpha",
            "--na-strategy",
        ] {
            assert!(
                !help.contains(internal_flag),
                "LauncherArgs help must not mention internal flag `{internal_flag}`, got:\n{help}"
            );
        }
    }
}
