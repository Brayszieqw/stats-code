use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};

use crate::bridge::Engine;

#[derive(Debug, Clone, Parser)]
#[command(
    name = "stats-code",
    version,
    about = "Stats Code: reproducible epidemiology and preventive medicine statistics CLI"
)]
pub struct Cli {
    #[arg(long, global = true)]
    pub json: bool,

    #[arg(long, global = true)]
    pub artifacts_dir: Option<PathBuf>,

    #[arg(long, global = true)]
    pub session: Option<PathBuf>,

    #[arg(long, global = true, default_value = "gpt")]
    pub model: String,

    #[arg(long, global = true)]
    pub system: Option<String>,

    #[arg(long, global = true)]
    pub max_tokens: Option<u32>,

    /// Execution engine: rust (default), python, or r.
    #[arg(long, global = true, default_value = "rust")]
    pub engine: Engine,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Clone, Subcommand)]
pub enum Command {
    Chat(ChatArgs),
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
    Inspect(InspectArgs),
    Tableone(TableOneArgs),
    Rate(RateArgs),
    Auth {
        #[command(subcommand)]
        command: AuthCommand,
    },
    Ai {
        #[command(subcommand)]
        command: AiCommand,
    },
    Model {
        #[command(subcommand)]
        command: ModelCommand,
    },
    Report {
        #[command(subcommand)]
        command: ReportCommand,
    },
    /// Run a custom Python or R script via the bridge.
    Run {
        #[command(subcommand)]
        command: RunCommand,
    },
}

#[derive(Debug, Clone, Subcommand)]
pub enum ConfigCommand {
    Show,
    DefaultModel(ConfigModelArgs),
    AddModel(ConfigModelArgs),
    RemoveModel(ConfigModelArgs),
}

#[derive(Debug, Clone, Args, Serialize, Deserialize)]
pub struct ConfigModelArgs {
    pub model: String,
}

#[derive(Debug, Clone, Default, Args, Serialize, Deserialize)]
pub struct ChatArgs {
    #[arg(long)]
    pub no_tools: bool,

    #[arg(long)]
    pub new_session: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum AuthProvider {
    Openai,
    Gemini,
    Deepseek,
    Dashscope,
    Moonshot,
    Xai,
}

#[derive(Debug, Clone, Subcommand)]
pub enum AuthCommand {
    Set(AuthSetArgs),
    Doctor(AuthDoctorArgs),
}

#[derive(Debug, Clone, Args, Serialize, Deserialize)]
pub struct AuthSetArgs {
    pub provider: AuthProvider,

    #[arg(long)]
    pub api_key: String,

    #[arg(long)]
    pub base_url: Option<String>,
}

#[derive(Debug, Clone, Args, Serialize, Deserialize)]
pub struct AuthDoctorArgs {
    #[arg(long)]
    pub provider: Option<AuthProvider>,
}

#[derive(Debug, Clone, Subcommand)]
pub enum AiCommand {
    Ask(AiAskArgs),
}

#[derive(Debug, Clone, Args, Serialize, Deserialize)]
pub struct AiAskArgs {
    #[arg(long, default_value = "gpt")]
    pub model: String,

    #[arg(long)]
    pub system: Option<String>,

    #[arg(long)]
    pub max_tokens: Option<u32>,

    #[arg(required = true, trailing_var_arg = true)]
    pub prompt: Vec<String>,
}

#[derive(Debug, Clone, Args, Serialize, Deserialize)]
pub struct InspectArgs {
    pub data_path: PathBuf,
}

#[derive(Debug, Clone, Args, Serialize, Deserialize)]
pub struct TableOneArgs {
    #[arg(long)]
    pub data: Option<PathBuf>,

    #[arg(long)]
    pub analysis: Option<PathBuf>,

    #[arg(long)]
    pub by: String,

    #[arg(long, value_delimiter = ',')]
    #[serde(default)]
    pub vars: Vec<String>,
}

#[derive(Debug, Clone, Args, Serialize, Deserialize)]
pub struct RateArgs {
    #[arg(long)]
    pub data: Option<PathBuf>,

    #[arg(long)]
    pub analysis: Option<PathBuf>,

    #[arg(long)]
    pub event: String,

    #[arg(long)]
    pub person_time: String,

    #[arg(long, value_delimiter = ',')]
    #[serde(default)]
    pub strata: Vec<String>,
}

#[derive(Debug, Clone, Subcommand)]
pub enum ModelCommand {
    Logistic(ModelLogisticArgs),
    Cox(ModelCoxArgs),
    Linear(ModelLinearArgs),
}

#[derive(Debug, Clone, Args, Serialize, Deserialize)]
pub struct ModelLogisticArgs {
    #[arg(long)]
    pub data: Option<PathBuf>,

    #[arg(long)]
    pub analysis: Option<PathBuf>,

    #[arg(long = "y")]
    #[serde(alias = "y")]
    pub outcome: String,

    #[arg(long = "x", value_delimiter = ',')]
    #[serde(alias = "x")]
    pub predictors: Vec<String>,

    #[arg(long, value_delimiter = ',')]
    #[serde(default)]
    pub adjust: Vec<String>,

    #[arg(long, value_delimiter = ',')]
    #[serde(default)]
    pub strata: Vec<String>,
}

#[derive(Debug, Clone, Args, Serialize, Deserialize)]
pub struct ModelCoxArgs {
    #[arg(long)]
    pub data: Option<PathBuf>,

    #[arg(long)]
    pub analysis: Option<PathBuf>,

    #[arg(long)]
    pub time: String,

    #[arg(long)]
    pub event: String,

    #[arg(long = "x", value_delimiter = ',')]
    #[serde(alias = "x")]
    pub predictors: Vec<String>,

    #[arg(long, value_delimiter = ',')]
    #[serde(default)]
    pub adjust: Vec<String>,

    #[arg(long, value_delimiter = ',')]
    #[serde(default)]
    pub strata: Vec<String>,
}

#[derive(Debug, Clone, Args, Serialize, Deserialize)]
pub struct ModelLinearArgs {
    #[arg(long)]
    pub data: Option<PathBuf>,

    #[arg(long)]
    pub analysis: Option<PathBuf>,

    #[arg(long = "y")]
    #[serde(alias = "y")]
    pub outcome: String,

    #[arg(long = "x", value_delimiter = ',')]
    #[serde(alias = "x")]
    pub predictors: Vec<String>,

    #[arg(long, value_delimiter = ',')]
    #[serde(default)]
    pub adjust: Vec<String>,

    #[arg(long, value_delimiter = ',')]
    #[serde(default)]
    pub strata: Vec<String>,
}

#[derive(Debug, Clone, Subcommand)]
pub enum ReportCommand {
    Build(ReportBuildArgs),
}

#[derive(Debug, Clone, Args, Serialize, Deserialize)]
pub struct ReportBuildArgs {
    pub analysis: PathBuf,

    #[arg(long)]
    pub out: Option<PathBuf>,

    #[arg(long)]
    pub artifacts: Option<PathBuf>,
}

#[derive(Debug, Clone, Subcommand)]
pub enum RunCommand {
    /// Run a Python script via the bridge.
    Python(RunScriptArgs),
    /// Run an R script via the bridge.
    R(RunScriptArgs),
}

#[derive(Debug, Clone, Args, Serialize, Deserialize)]
pub struct RunScriptArgs {
    /// Path to the script file.
    pub script: PathBuf,

    /// Path to the data file.
    #[arg(long)]
    pub data: Option<PathBuf>,

    /// JSON string with additional parameters.
    #[arg(long)]
    pub params: Option<String>,
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::{Cli, Command};

    #[test]
    fn allows_no_subcommand_for_interactive_mode() {
        let cli = Cli::parse_from(["stats-code"]);
        assert!(cli.command.is_none());
        assert_eq!(cli.model, "gpt");
        assert_eq!(cli.engine, crate::bridge::Engine::Rust);
    }

    #[test]
    fn parses_chat_subcommand_and_global_model() {
        let cli = Cli::parse_from([
            "stats-code",
            "--model",
            "gemini",
            "--session",
            "saved-session.json",
            "chat",
            "--no-tools",
            "--new-session",
        ]);
        assert_eq!(cli.model, "gemini");
        assert_eq!(cli.session, Some("saved-session.json".into()));
        match cli.command {
            Some(Command::Chat(args)) => {
                assert!(args.no_tools);
                assert!(args.new_session);
            }
            other => panic!("expected chat command, got {other:?}"),
        }
    }
}
