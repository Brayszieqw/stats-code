pub mod bridge;
mod chat;
mod cli;
mod config;
mod cox;
mod error;
mod gugugaga_art;
mod handlers;
mod helpers;
mod input;
mod linear;
mod logistic;
mod math;
mod modeling;
mod rate;
mod render;
mod report;
mod schema;
mod tableone;
mod ui;

pub use bridge::Engine;
pub use cli::{
    AiAskArgs, AiCommand, AuditCommand, AuditExplainArgs, AuthCommand, AuthDoctorArgs,
    AuthProvider, AuthSetArgs, ChatArgs, CheckArgs, Cli, Command, ConfigCommand, ConfigModelArgs,
    InspectArgs, ModelCommand, ModelCoxArgs, ModelLogisticArgs, OpenCommand, OpenReportArgs,
    PlanArgs, RateArgs, ReportBuildArgs, ReportCommand, ReportVerifyArgs, RunCommand,
    RunScriptArgs, TableOneArgs, WorkflowCommand, WorkflowRunArgs,
};
pub use error::{StatsCodeError, StatsCodeResult};
pub use handlers::{dispatch, run};
pub use schema::{
    AnalysisCheckResult, AnalysisSpec, DataFormat, ReportVerifyResult, WorkflowRunResult,
};
