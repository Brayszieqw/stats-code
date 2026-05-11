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
    /// Create a demo analysis project from bundled templates.
    Init(InitArgs),
    /// Check local Stats Code environment readiness.
    Doctor(DoctorArgs),
    /// Preview the declared workflow plan without running statistics.
    Plan(PlanArgs),
    /// Validate an analysis.yaml contract without running statistics.
    Check(CheckArgs),
    Inspect(InspectArgs),
    Tableone(TableOneArgs),
    Rate(RateArgs),
    Power {
        #[command(subcommand)]
        command: PowerCommand,
    },
    Diagnostic {
        #[command(subcommand)]
        command: DiagnosticCommand,
    },
    Survival {
        #[command(subcommand)]
        command: SurvivalCommand,
    },
    Auth {
        #[command(subcommand)]
        command: AuthCommand,
    },
    Ai {
        #[command(subcommand)]
        command: AiCommand,
    },
    Audit {
        #[command(subcommand)]
        command: AuditCommand,
    },
    Model {
        #[command(subcommand)]
        command: ModelCommand,
    },
    Report {
        #[command(subcommand)]
        command: ReportCommand,
    },
    Open {
        #[command(subcommand)]
        command: OpenCommand,
    },
    /// Run the declared analysis.yaml workflow deterministically.
    Workflow {
        #[command(subcommand)]
        command: WorkflowCommand,
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

#[derive(Debug, Clone, Args, Serialize, Deserialize)]
pub struct InitArgs {
    pub project_dir: PathBuf,
}

#[derive(Debug, Clone, Args, Serialize, Deserialize)]
pub struct DoctorArgs {}

#[derive(Debug, Clone, Args, Serialize, Deserialize)]
pub struct PlanArgs {
    pub analysis: PathBuf,

    #[arg(long)]
    pub out: Option<PathBuf>,

    #[arg(long = "explore-out")]
    pub explore_out: Option<PathBuf>,

    #[arg(long)]
    #[serde(default)]
    pub include_exploratory: bool,

    #[arg(long)]
    #[serde(default)]
    pub strict: bool,

    #[arg(long)]
    #[serde(default)]
    pub allow_warnings: bool,

    #[arg(long)]
    #[serde(default)]
    pub allow_unenforced_survey: bool,

    #[arg(long)]
    #[serde(default)]
    pub allow_unenforced_privacy: bool,
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

#[derive(Debug, Clone, Subcommand)]
pub enum AuditCommand {
    Explain(AuditExplainArgs),
}

#[derive(Debug, Clone, Args, Serialize, Deserialize)]
pub struct AuditExplainArgs {
    pub artifacts: PathBuf,
}

#[derive(Debug, Clone, Args, Serialize, Deserialize)]
pub struct CheckArgs {
    pub analysis: PathBuf,
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

#[derive(Debug, Clone, Serialize, Deserialize, Subcommand)]
pub enum PowerCommand {
    /// Sample size for one-sample proportion confidence interval precision.
    OneProportion(PowerOneProportionArgs),
    /// Sample size per group for two independent proportions.
    TwoProportions(PowerTwoProportionsArgs),
    /// Sample size per group for two independent means.
    TwoMeans(PowerTwoMeansArgs),
}

#[derive(Debug, Clone, Subcommand)]
pub enum DiagnosticCommand {
    /// ROC/AUC and diagnostic test performance from binary truth and continuous score.
    Roc(DiagnosticRocArgs),
}

#[derive(Debug, Clone, Args, Serialize, Deserialize)]
pub struct DiagnosticRocArgs {
    #[arg(long)]
    pub data: Option<PathBuf>,

    #[arg(long)]
    pub analysis: Option<PathBuf>,

    #[arg(long)]
    pub truth: String,

    #[arg(long)]
    pub score: String,

    #[arg(long)]
    pub threshold: Option<f64>,
}

#[derive(Debug, Clone, Args, Serialize, Deserialize)]
pub struct PowerOneProportionArgs {
    #[arg(long)]
    pub proportion: f64,

    #[arg(long)]
    pub margin: f64,

    #[arg(long, default_value_t = 0.05)]
    pub alpha: f64,
}

#[derive(Debug, Clone, Args, Serialize, Deserialize)]
pub struct PowerTwoProportionsArgs {
    #[arg(long = "p1")]
    pub p1: f64,

    #[arg(long = "p2")]
    pub p2: f64,

    #[arg(long, default_value_t = 0.8)]
    pub power: f64,

    #[arg(long, default_value_t = 0.05)]
    pub alpha: f64,

    #[arg(long, default_value_t = 1.0)]
    pub allocation_ratio: f64,
}

#[derive(Debug, Clone, Args, Serialize, Deserialize)]
pub struct PowerTwoMeansArgs {
    #[arg(long)]
    pub mean1: f64,

    #[arg(long)]
    pub mean2: f64,

    #[arg(long)]
    pub sd: f64,

    #[arg(long, default_value_t = 0.8)]
    pub power: f64,

    #[arg(long, default_value_t = 0.05)]
    pub alpha: f64,

    #[arg(long, default_value_t = 1.0)]
    pub allocation_ratio: f64,
}

#[derive(Debug, Clone, Subcommand)]
pub enum SurvivalCommand {
    /// Kaplan-Meier survival analysis with optional log-rank test.
    Km(SurvivalKmArgs),
}

#[derive(Debug, Clone, Args, Serialize, Deserialize)]
pub struct SurvivalKmArgs {
    #[arg(long)]
    pub data: Option<PathBuf>,

    #[arg(long)]
    pub analysis: Option<PathBuf>,

    #[arg(long)]
    pub time: String,

    #[arg(long)]
    pub event: String,

    #[arg(long = "by")]
    #[serde(default)]
    pub group: Option<String>,
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
    Verify(ReportVerifyArgs),
}

#[derive(Debug, Clone, Args, Serialize, Deserialize)]
pub struct ReportBuildArgs {
    pub analysis: PathBuf,

    #[arg(long)]
    pub out: Option<PathBuf>,

    #[arg(long)]
    pub artifacts: Option<PathBuf>,

    #[arg(long)]
    #[serde(default)]
    pub include_exploratory: bool,
}

#[derive(Debug, Clone, Args, Serialize, Deserialize)]
pub struct ReportVerifyArgs {
    pub artifacts: PathBuf,

    #[arg(long)]
    #[serde(default)]
    pub fail_on_warning: bool,
}

#[derive(Debug, Clone, Subcommand)]
pub enum OpenCommand {
    Report(OpenReportArgs),
}

#[derive(Debug, Clone, Args, Serialize, Deserialize)]
pub struct OpenReportArgs {
    #[arg(default_value = "stats-code-artifacts")]
    pub artifacts: PathBuf,

    #[arg(long)]
    #[serde(default)]
    pub print_only: bool,
}

#[derive(Debug, Clone, Subcommand)]
pub enum WorkflowCommand {
    Run(WorkflowRunArgs),
}

#[derive(Debug, Clone, Args, Serialize, Deserialize)]
pub struct WorkflowRunArgs {
    pub analysis: PathBuf,

    #[arg(long)]
    pub out: Option<PathBuf>,

    #[arg(long = "explore-out")]
    pub explore_out: Option<PathBuf>,

    #[arg(long)]
    #[serde(default)]
    pub include_exploratory: bool,

    #[arg(long)]
    #[serde(default)]
    pub strict: bool,

    #[arg(long)]
    #[serde(default)]
    pub allow_warnings: bool,

    #[arg(long)]
    #[serde(default)]
    pub allow_unenforced_survey: bool,

    #[arg(long)]
    #[serde(default)]
    pub allow_unenforced_privacy: bool,

    #[arg(long)]
    #[serde(default)]
    pub no_chat: bool,
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
    use std::path::PathBuf;

    use clap::Parser;

    use super::{
        AuditCommand, Cli, Command, DiagnosticCommand, OpenCommand, PowerCommand, ReportCommand,
        WorkflowCommand,
    };

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

    #[test]
    fn parses_diagnostic_roc_command() {
        let cli = Cli::parse_from([
            "stats-code",
            "diagnostic",
            "roc",
            "--data",
            "diagnostic.csv",
            "--truth",
            "disease",
            "--score",
            "risk_score",
            "--threshold",
            "0.5",
        ]);
        match cli.command {
            Some(Command::Diagnostic {
                command: DiagnosticCommand::Roc(args),
            }) => {
                assert_eq!(args.data, Some(PathBuf::from("diagnostic.csv")));
                assert_eq!(args.truth, "disease");
                assert_eq!(args.score, "risk_score");
                assert_eq!(args.threshold, Some(0.5));
            }
            other => panic!("expected diagnostic roc command, got {other:?}"),
        }
    }

    #[test]
    fn parses_power_two_means_command() {
        let cli = Cli::parse_from([
            "stats-code",
            "power",
            "two-means",
            "--mean1",
            "10",
            "--mean2",
            "12",
            "--sd",
            "4",
            "--power",
            "0.9",
            "--alpha",
            "0.01",
        ]);
        match cli.command {
            Some(Command::Power {
                command: PowerCommand::TwoMeans(args),
            }) => {
                assert_eq!(args.mean1, 10.0);
                assert_eq!(args.mean2, 12.0);
                assert_eq!(args.sd, 4.0);
                assert_eq!(args.power, 0.9);
                assert_eq!(args.alpha, 0.01);
            }
            other => panic!("expected power two-means command, got {other:?}"),
        }
    }

    #[test]
    fn parses_workflow_run_command() {
        let cli = Cli::parse_from([
            "stats-code",
            "--model",
            "gpt-4.1",
            "workflow",
            "run",
            "analysis.yaml",
            "--out",
            "formal-artifacts",
            "--explore-out",
            "scratch-artifacts",
            "--no-chat",
        ]);
        assert_eq!(cli.model, "gpt-4.1");
        match cli.command {
            Some(Command::Workflow {
                command: WorkflowCommand::Run(args),
            }) => {
                assert_eq!(args.analysis, PathBuf::from("analysis.yaml"));
                assert_eq!(args.out, Some(PathBuf::from("formal-artifacts")));
                assert_eq!(args.explore_out, Some(PathBuf::from("scratch-artifacts")));
                assert!(args.no_chat);
                assert!(!args.strict);
                assert!(!args.allow_warnings);
                assert!(!args.allow_unenforced_survey);
                assert!(!args.allow_unenforced_privacy);
            }
            other => panic!("expected workflow run command, got {other:?}"),
        }
    }

    #[test]
    fn parses_init_command() {
        let cli = Cli::parse_from(["stats-code", "init", "demo-study"]);
        match cli.command {
            Some(Command::Init(args)) => {
                assert_eq!(args.project_dir, PathBuf::from("demo-study"));
            }
            other => panic!("expected init command, got {other:?}"),
        }
    }

    #[test]
    fn parses_doctor_command() {
        let cli = Cli::parse_from(["stats-code", "doctor"]);
        assert!(matches!(cli.command, Some(Command::Doctor(_))));
    }

    #[test]
    fn parses_plan_command() {
        let cli = Cli::parse_from([
            "stats-code",
            "plan",
            "analysis.yaml",
            "--out",
            "formal-artifacts",
            "--strict",
            "--allow-unenforced-survey",
        ]);
        match cli.command {
            Some(Command::Plan(args)) => {
                assert_eq!(args.analysis, PathBuf::from("analysis.yaml"));
                assert_eq!(args.out, Some(PathBuf::from("formal-artifacts")));
                assert!(args.strict);
                assert!(args.allow_unenforced_survey);
                assert!(!args.allow_unenforced_privacy);
            }
            other => panic!("expected plan command, got {other:?}"),
        }
    }

    #[test]
    fn parses_workflow_policy_flags() {
        let cli = Cli::parse_from([
            "stats-code",
            "workflow",
            "run",
            "analysis.yaml",
            "--strict",
            "--allow-warnings",
            "--allow-unenforced-survey",
            "--allow-unenforced-privacy",
        ]);
        match cli.command {
            Some(Command::Workflow {
                command: WorkflowCommand::Run(args),
            }) => {
                assert!(args.strict);
                assert!(args.allow_warnings);
                assert!(args.allow_unenforced_survey);
                assert!(args.allow_unenforced_privacy);
            }
            other => panic!("expected workflow run command, got {other:?}"),
        }
    }

    #[test]
    fn parses_check_command() {
        let cli = Cli::parse_from(["stats-code", "check", "analysis.yaml"]);
        match cli.command {
            Some(Command::Check(args)) => {
                assert_eq!(args.analysis, PathBuf::from("analysis.yaml"));
            }
            other => panic!("expected check command, got {other:?}"),
        }
    }

    #[test]
    fn parses_report_verify_command() {
        let cli = Cli::parse_from([
            "stats-code",
            "report",
            "verify",
            "stats-code-artifacts",
            "--fail-on-warning",
        ]);
        match cli.command {
            Some(Command::Report {
                command: ReportCommand::Verify(args),
            }) => {
                assert_eq!(args.artifacts, PathBuf::from("stats-code-artifacts"));
                assert!(args.fail_on_warning);
            }
            other => panic!("expected report verify command, got {other:?}"),
        }
    }

    #[test]
    fn parses_audit_explain_command() {
        let cli = Cli::parse_from(["stats-code", "audit", "explain", "stats-code-artifacts"]);
        match cli.command {
            Some(Command::Audit {
                command: AuditCommand::Explain(args),
            }) => {
                assert_eq!(args.artifacts, PathBuf::from("stats-code-artifacts"));
            }
            other => panic!("expected audit explain command, got {other:?}"),
        }
    }

    #[test]
    fn parses_open_report_command() {
        let cli = Cli::parse_from([
            "stats-code",
            "open",
            "report",
            "stats-code-artifacts",
            "--print-only",
        ]);
        match cli.command {
            Some(Command::Open {
                command: OpenCommand::Report(args),
            }) => {
                assert_eq!(args.artifacts, PathBuf::from("stats-code-artifacts"));
                assert!(args.print_only);
            }
            other => panic!("expected open report command, got {other:?}"),
        }
    }
}
