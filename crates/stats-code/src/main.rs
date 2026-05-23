use clap::Parser;
use stats_code::launcher::args::LauncherArgs;
use stats_code::launcher::{classify_invocation, Launcher, Mode};

fn main() {
    let argv: Vec<String> = std::env::args().collect();

    match classify_invocation(&argv) {
        Mode::Launcher => {
            // 尝试用 clap 解析 launcher 旗标（--no-browser / --help / --version）。
            // clap 遇到 --help 或 --version 时会自动 print + exit(0)。
            let args = LauncherArgs::parse();
            let launcher = Launcher::new(args);
            if let Err(error) = launcher.run() {
                eprintln!("error: {error}");
                std::process::exit(error.exit_code());
            }
        }
        Mode::Subcommand => {
            // 子命令路径：完全复用现有逻辑，零行为差异
            if let Err(error) = stats_code::run() {
                eprintln!(
                    "error: {error}

Run `stats-code --help` for usage."
                );
                std::process::exit(1);
            }
        }
    }
}
