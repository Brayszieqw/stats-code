pub mod bridge;
/// Internal math module exposed for integration testing only.
#[doc(hidden)]
pub mod math;
/// Release packaging helpers (Feature: single-command-launcher).
pub mod release;
/// Stats Code Launcher 模块树（Feature: single-command-launcher，task 1.2）。
pub mod launcher;
mod chat;
mod cli;
mod config;
mod cox;
mod diagnostic;
mod error;
mod gugugaga_art;
mod handlers;
mod helpers;
mod input;
mod linear;
mod logistic;
mod modeling;
mod power;
mod rate;
mod render;
mod report;
mod schema;
mod stats;
mod survival;
mod tableone;
mod ui;

pub use bridge::Engine;
pub use cli::{
    AiAskArgs, AiCommand, AuditCommand, AuditExplainArgs, AuthCommand, AuthDoctorArgs,
    AuthProvider, AuthSetArgs, ChatArgs, CheckArgs, Cli, Command, ConfigCommand, ConfigModelArgs,
    DiagnosticCommand, DiagnosticRocArgs, InspectArgs, ModelCommand, ModelCoxArgs, ModelLinearArgs,
    ModelLogisticArgs, NaStrategy, OpenCommand, OpenReportArgs, PlanArgs, PowerCommand,
    PowerOneProportionArgs, PowerTwoMeansArgs, PowerTwoProportionsArgs, RateArgs, ReportBuildArgs,
    ReportCommand, ReportVerifyArgs, RunCommand, RunScriptArgs, StatsCommand, SurvivalCommand,
    SurvivalKmArgs, TableOneArgs, WorkflowCommand, WorkflowRunArgs,
};
pub use error::{StatsCodeError, StatsCodeResult};
pub use handlers::{dispatch, run};
pub use schema::{
    AnalysisCheckResult, AnalysisSpec, DataFormat, ReportVerifyResult, WorkflowRunResult,
    // Statistical methods result types (task 3.1)
    TtestPairedResult, TtestOneSampleResult,
    AnovaGroupSummary, OneWayAnovaResult, RbdAnovaResult, RepeatedAnovaResult,
    PosthocPair, PosthocResult,
    McNemarResult, WilcoxonSignedRankResult, MannWhitneyResult,
    CategoryProportion, CochranArmitageResult,
    CorrelationResult,
    NormalityResult,
    GroupVarianceSummary, VarianceHomogeneityResult,
    TwoByTwoCells, MhStratum, OrRrResult,
    StandardizationStratum, StandardizationResult,
    AttributableRiskResult,
    DoseResponseCategory, DoseResponseResult,
    PoissonCoefficient, PoissonResult,
    MultinomialCoefficientGroup, OrdinalLogitResult, MultinomialLogitResult,
    MixedFixedEffect, MixedLmmResult,
    LifeTableRow, LifeTableResult,
    CompetingRisksCauseFit, CompetingRisksCif, CompetingRisksResult,
    PowerLogRankResult,
    MetaStudy, MetaAnalysisResult,
    KappaResult,
    BlandAltmanPoint, BlandAltmanResult,
    PcaComponent, PcaResult,
    LdaResult,
    ClusterAssignment, ClusterResult,
    PsmCovariateSmd, PsmResult,
};
