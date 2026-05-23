use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};

use crate::bridge::Engine;

/// Strategy for handling missing values in statistical analyses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum NaStrategy {
    /// Exclude rows with any missing value in required columns (default).
    #[default]
    Drop,
    /// Return a descriptive error if any required column has missing values.
    Error,
}

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

    /// Significance level / confidence level control (default 0.05 → 95% CI).
    #[arg(long, global = true, default_value_t = 0.05)]
    pub alpha: f64,

    /// Missing-value handling strategy: drop (default) or error.
    #[arg(long = "na-strategy", global = true, default_value = "drop")]
    pub na_strategy: NaStrategy,

    #[command(subcommand)]
    pub command: Option<Command>,
}

// All variants in `Command` are hidden from `stats-code --help`. They remain
// fully parseable so internal callers (e.g. `SkillInvoker::StatsCli`) can still
// invoke `stats-code workflow run ...`, but the user-facing help surface stays
// minimal per Requirements 2.2 / 2.4 / 2.5 of the single-command-launcher spec.
#[derive(Debug, Clone, Subcommand)]
pub enum Command {
    #[command(hide = true)]
    Chat(ChatArgs),
    #[command(hide = true)]
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
    /// Create a demo analysis project from bundled templates.
    #[command(hide = true)]
    Init(InitArgs),
    /// Check local Stats Code environment readiness.
    #[command(hide = true)]
    Doctor(DoctorArgs),
    /// Preview the declared workflow plan without running statistics.
    #[command(hide = true)]
    Plan(PlanArgs),
    /// Validate an analysis.yaml contract without running statistics.
    #[command(hide = true)]
    Check(CheckArgs),
    #[command(hide = true)]
    Inspect(InspectArgs),
    #[command(hide = true)]
    Tableone(TableOneArgs),
    #[command(hide = true)]
    Rate(RateArgs),
    #[command(hide = true)]
    Power {
        #[command(subcommand)]
        command: PowerCommand,
    },
    #[command(hide = true)]
    Diagnostic {
        #[command(subcommand)]
        command: DiagnosticCommand,
    },
    #[command(hide = true)]
    Survival {
        #[command(subcommand)]
        command: SurvivalCommand,
    },
    #[command(hide = true)]
    Auth {
        #[command(subcommand)]
        command: AuthCommand,
    },
    #[command(hide = true)]
    Ai {
        #[command(subcommand)]
        command: AiCommand,
    },
    #[command(hide = true)]
    Audit {
        #[command(subcommand)]
        command: AuditCommand,
    },
    #[command(hide = true)]
    Model {
        #[command(subcommand)]
        command: ModelCommand,
    },
    #[command(hide = true)]
    Report {
        #[command(subcommand)]
        command: ReportCommand,
    },
    #[command(hide = true)]
    Open {
        #[command(subcommand)]
        command: OpenCommand,
    },
    /// Run the declared analysis.yaml workflow deterministically.
    #[command(hide = true)]
    Workflow {
        #[command(subcommand)]
        command: WorkflowCommand,
    },
    /// Run a custom Python or R script via the bridge.
    #[command(hide = true)]
    Run {
        #[command(subcommand)]
        command: RunCommand,
    },
    /// Statistical and epidemiological analysis methods.
    #[command(hide = true)]
    Stats {
        #[command(subcommand)]
        command: StatsCommand,
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

// ─── Stats subcommand tree ────────────────────────────────────────────────────

/// Top-level subcommands for the `stats` group.
#[derive(Debug, Clone, Subcommand)]
pub enum StatsCommand {
    /// Paired and one-sample t-tests.
    Ttest {
        #[command(subcommand)]
        command: TtestCommand,
    },
    /// One-way, randomized-block, repeated-measures ANOVA, and post-hoc tests.
    Anova {
        #[command(subcommand)]
        command: AnovaCommand,
    },
    /// Nonparametric tests (`McNemar`, Wilcoxon signed-rank, Mann-Whitney U,
    /// Cochran-Armitage trend).
    Nonparam {
        #[command(subcommand)]
        command: NonparamCommand,
    },
    /// Normality and homogeneity-of-variance diagnostic tests.
    Diagnostic {
        #[command(subcommand)]
        command: DiagnosticStatsCommand,
    },
    /// Epidemiological measures (OR/RR, standardization, attributable risk,
    /// dose-response).
    Epi {
        #[command(subcommand)]
        command: EpiStatsCommand,
    },
    /// Inter-rater agreement (kappa, Bland-Altman).
    Agreement {
        #[command(subcommand)]
        command: AgreementCommand,
    },
    /// Multivariate methods (PCA, LDA, cluster analysis).
    Multivariate {
        #[command(subcommand)]
        command: MultivariateCommand,
    },
    /// Sample-size calculators.
    SampleSize {
        #[command(subcommand)]
        command: SampleSizeCommand,
    },
    /// Survival analysis extensions (life table, competing risks).
    Survival {
        #[command(subcommand)]
        command: StatsSurvivalCommand,
    },
    /// Regression models (Poisson, ordinal logistic, multinomial logistic).
    Model {
        #[command(subcommand)]
        command: StatsModelCommand,
    },
    /// Pearson r and Spearman ρ correlation analysis.
    Correlation(StatsCorrelationArgs),
    /// Fixed-effect and random-effects meta-analysis.
    Meta(StatsMetaArgs),
    /// Linear mixed-effects models (REML).
    Mixed(StatsMixedArgs),
    /// Propensity score matching.
    Psm(StatsPsmArgs),
}

/// T-test subcommands.
#[derive(Debug, Clone, Subcommand)]
pub enum TtestCommand {
    /// Paired t-test (before/after measurements).
    Paired(TtestPairedArgs),
    /// One-sample t-test against a known population mean.
    OneSample(TtestOneSampleArgs),
}

/// ANOVA subcommands.
#[derive(Debug, Clone, Subcommand)]
pub enum AnovaCommand {
    /// One-way (CRD) or randomized-block (RBD) ANOVA.
    Oneway(AnovaOnewayArgs),
    /// Repeated-measures ANOVA.
    Repeated(AnovaRepeatedArgs),
    /// Post-hoc pairwise comparisons (Bonferroni or Tukey HSD).
    Posthoc(AnovaPosthocArgs),
}

/// Nonparametric test subcommands.
#[derive(Debug, Clone, Subcommand)]
pub enum NonparamCommand {
    /// `McNemar` test for paired binary outcomes.
    Mcnemar(NonparamMcnemarArgs),
    /// Wilcoxon signed-rank test.
    Wilcoxon(NonparamWilcoxonArgs),
    /// Mann-Whitney U test for two independent groups.
    Mannwhitney(NonparamMannwhitneyArgs),
    /// Cochran-Armitage trend test.
    CochranArmitage(NonparamCochranArmitageArgs),
}

/// Diagnostic (normality / variance) subcommands — distinct from the existing
/// `DiagnosticCommand` (ROC) to avoid name collision.
#[derive(Debug, Clone, Subcommand)]
pub enum DiagnosticStatsCommand {
    /// Shapiro-Wilk and Lilliefors K-S normality tests.
    Normality(DiagnosticNormalityArgs),
    /// Levene and Bartlett homogeneity-of-variance tests.
    Variance(DiagnosticVarianceArgs),
}

/// Epidemiology subcommands.
#[derive(Debug, Clone, Subcommand)]
pub enum EpiStatsCommand {
    /// Odds ratio and relative risk with optional Mantel-Haenszel stratification.
    OrRr(EpiOrRrArgs),
    /// Direct and indirect (SMR) rate standardization.
    Standardize(EpiStandardizeArgs),
    /// Attributable risk measures (AR, AR%, PAR, PAR%).
    Attributable(EpiAttributableArgs),
    /// Dose-response analysis (log-linear trend via Poisson GLM).
    DoseResponse(EpiDoseResponseArgs),
}

/// Agreement subcommands.
#[derive(Debug, Clone, Subcommand)]
pub enum AgreementCommand {
    /// Cohen's kappa and weighted kappa.
    Kappa(AgreementKappaArgs),
    /// Bland-Altman method-comparison analysis.
    BlandAltman(AgreementBlandAltmanArgs),
}

/// Multivariate subcommands.
#[derive(Debug, Clone, Subcommand)]
pub enum MultivariateCommand {
    /// Principal component analysis.
    Pca(MultivariatePcaArgs),
    /// Linear discriminant analysis.
    Lda(MultivariateLdaArgs),
    /// K-means and Ward hierarchical cluster analysis.
    Cluster(MultivariateClusterArgs),
}

/// Sample-size subcommands.
#[derive(Debug, Clone, Subcommand)]
pub enum SampleSizeCommand {
    /// Schoenfeld log-rank sample-size calculator.
    LogRank(SampleSizeLogRankArgs),
}

/// Survival-analysis subcommands (stats group extension).
#[derive(Debug, Clone, Subcommand)]
pub enum StatsSurvivalCommand {
    /// Actuarial life-table survival analysis.
    Lifetable(StatsSurvivalLifetableArgs),
    /// Competing-risks analysis (cause-specific Cox + CIF).
    Competing(StatsSurvivalCompetingArgs),
}

/// Regression-model subcommands.
#[derive(Debug, Clone, Subcommand)]
pub enum StatsModelCommand {
    /// Poisson regression with offset for person-time.
    Poisson(StatsModelPoissonArgs),
    /// Ordinal logistic regression (proportional odds).
    Ordinal(StatsModelOrdinalArgs),
    /// Multinomial logistic regression.
    Multinomial(StatsModelMultinomialArgs),
}

// ─── Placeholder Args structs (filled per-method in later tasks) ──────────────

#[derive(Debug, Clone, Args, Serialize, Deserialize)]
pub struct TtestPairedArgs {
    #[arg(long)]
    pub data: Option<PathBuf>,
    #[arg(long)]
    pub analysis: Option<PathBuf>,
    #[arg(long)]
    pub before: String,
    #[arg(long)]
    pub after: String,
}

#[derive(Debug, Clone, Args, Serialize, Deserialize)]
pub struct TtestOneSampleArgs {
    #[arg(long)]
    pub data: Option<PathBuf>,
    #[arg(long)]
    pub analysis: Option<PathBuf>,
    #[arg(long)]
    pub var: String,
    /// Hypothesized population mean.
    #[arg(long)]
    pub mu: f64,
}

#[derive(Debug, Clone, Args, Serialize, Deserialize)]
pub struct AnovaOnewayArgs {
    #[arg(long)]
    pub data: Option<PathBuf>,
    #[arg(long)]
    pub analysis: Option<PathBuf>,
    #[arg(long)]
    pub var: String,
    #[arg(long)]
    pub group: String,
    #[arg(long)]
    pub block: Option<String>,
}

#[derive(Debug, Clone, Args, Serialize, Deserialize)]
pub struct AnovaRepeatedArgs {
    #[arg(long)]
    pub data: Option<PathBuf>,
    #[arg(long)]
    pub analysis: Option<PathBuf>,
    #[arg(long)]
    pub var: String,
    #[arg(long)]
    pub subject: String,
    #[arg(long)]
    pub time: String,
}

#[derive(Debug, Clone, Args, Serialize, Deserialize)]
pub struct AnovaPosthocArgs {
    #[arg(long)]
    pub data: Option<PathBuf>,
    #[arg(long)]
    pub analysis: Option<PathBuf>,
    #[arg(long)]
    pub var: String,
    #[arg(long)]
    pub group: String,
    #[arg(long, default_value = "bonferroni")]
    pub method: String,
}

#[derive(Debug, Clone, Args, Serialize, Deserialize)]
pub struct NonparamMcnemarArgs {
    #[arg(long)]
    pub data: Option<PathBuf>,
    #[arg(long)]
    pub analysis: Option<PathBuf>,
    #[arg(long)]
    pub var1: String,
    #[arg(long)]
    pub var2: String,
    #[arg(long, default_value_t = 25)]
    pub exact_threshold: usize,
}

#[derive(Debug, Clone, Args, Serialize, Deserialize)]
pub struct NonparamWilcoxonArgs {
    #[arg(long)]
    pub data: Option<PathBuf>,
    #[arg(long)]
    pub analysis: Option<PathBuf>,
    #[arg(long)]
    pub var1: String,
    #[arg(long)]
    pub var2: String,
}

#[derive(Debug, Clone, Args, Serialize, Deserialize)]
pub struct NonparamMannwhitneyArgs {
    #[arg(long)]
    pub data: Option<PathBuf>,
    #[arg(long)]
    pub analysis: Option<PathBuf>,
    #[arg(long)]
    pub var: String,
    #[arg(long)]
    pub group: String,
}

#[derive(Debug, Clone, Args, Serialize, Deserialize)]
pub struct NonparamCochranArmitageArgs {
    #[arg(long)]
    pub data: Option<PathBuf>,
    #[arg(long)]
    pub analysis: Option<PathBuf>,
    #[arg(long)]
    pub exposure: String,
    #[arg(long)]
    pub outcome: String,
    /// Comma-separated integer scores for ordered categories.
    #[arg(long, value_delimiter = ',')]
    #[serde(default)]
    pub scores: Vec<f64>,
}

#[derive(Debug, Clone, Args, Serialize, Deserialize)]
pub struct DiagnosticNormalityArgs {
    #[arg(long)]
    pub data: Option<PathBuf>,
    #[arg(long)]
    pub analysis: Option<PathBuf>,
    #[arg(long)]
    pub var: String,
}

#[derive(Debug, Clone, Args, Serialize, Deserialize)]
pub struct DiagnosticVarianceArgs {
    #[arg(long)]
    pub data: Option<PathBuf>,
    #[arg(long)]
    pub analysis: Option<PathBuf>,
    #[arg(long)]
    pub var: String,
    #[arg(long)]
    pub group: String,
    #[arg(long, default_value = "median")]
    pub center: String,
}

#[derive(Debug, Clone, Args, Serialize, Deserialize)]
pub struct EpiOrRrArgs {
    #[arg(long)]
    pub data: Option<PathBuf>,
    #[arg(long)]
    pub analysis: Option<PathBuf>,
    #[arg(long)]
    pub exposure: String,
    #[arg(long)]
    pub outcome: String,
    #[arg(long, value_delimiter = ',')]
    #[serde(default)]
    pub strata: Vec<String>,
    #[arg(long)]
    pub exposure_event: Option<String>,
    #[arg(long)]
    pub outcome_event: Option<String>,
}

#[derive(Debug, Clone, Args, Serialize, Deserialize)]
pub struct EpiStandardizeArgs {
    #[arg(long)]
    pub data: Option<PathBuf>,
    #[arg(long)]
    pub analysis: Option<PathBuf>,
    #[arg(long, default_value = "direct")]
    pub method: String,
    #[arg(long)]
    pub event: String,
    #[arg(long)]
    pub person_time: String,
    #[arg(long)]
    pub age_group: String,
    /// Built-in name (`who_world_2000`, `china_census_2010`, `segi_world`) or path to CSV.
    #[arg(long)]
    pub standard_pop: String,
}

#[derive(Debug, Clone, Args, Serialize, Deserialize)]
pub struct EpiAttributableArgs {
    #[arg(long)]
    pub data: Option<PathBuf>,
    #[arg(long)]
    pub analysis: Option<PathBuf>,
    #[arg(long)]
    pub exposure: String,
    #[arg(long)]
    pub outcome: String,
    #[arg(long)]
    pub person_time: Option<String>,
    #[arg(long)]
    pub exposure_prevalence: Option<f64>,
}

#[derive(Debug, Clone, Args, Serialize, Deserialize)]
pub struct EpiDoseResponseArgs {
    #[arg(long)]
    pub data: Option<PathBuf>,
    #[arg(long)]
    pub analysis: Option<PathBuf>,
    #[arg(long)]
    pub exposure: String,
    #[arg(long)]
    pub outcome: String,
    #[arg(long)]
    pub person_time: String,
    #[arg(long, value_delimiter = ',')]
    #[serde(default)]
    pub scores: Vec<f64>,
}

#[derive(Debug, Clone, Args, Serialize, Deserialize)]
pub struct AgreementKappaArgs {
    #[arg(long)]
    pub data: Option<PathBuf>,
    #[arg(long)]
    pub analysis: Option<PathBuf>,
    #[arg(long)]
    pub rater1: String,
    #[arg(long)]
    pub rater2: String,
    /// Weight scheme: none, linear, or quadratic.
    #[arg(long, default_value = "none")]
    pub weights: String,
}

#[derive(Debug, Clone, Args, Serialize, Deserialize)]
pub struct AgreementBlandAltmanArgs {
    #[arg(long)]
    pub data: Option<PathBuf>,
    #[arg(long)]
    pub analysis: Option<PathBuf>,
    #[arg(long)]
    pub method1: String,
    #[arg(long)]
    pub method2: String,
}

#[derive(Debug, Clone, Args, Serialize, Deserialize)]
pub struct MultivariatePcaArgs {
    #[arg(long)]
    pub data: Option<PathBuf>,
    #[arg(long)]
    pub analysis: Option<PathBuf>,
    #[arg(long, value_delimiter = ',')]
    pub vars: Vec<String>,
    #[arg(long)]
    pub n_components: Option<usize>,
    /// Matrix type: correlation (default) or covariance.
    #[arg(long, default_value = "correlation")]
    pub matrix: String,
}

#[derive(Debug, Clone, Args, Serialize, Deserialize)]
pub struct MultivariateLdaArgs {
    #[arg(long)]
    pub data: Option<PathBuf>,
    #[arg(long)]
    pub analysis: Option<PathBuf>,
    #[arg(long)]
    pub group: String,
    #[arg(long, value_delimiter = ',')]
    pub vars: Vec<String>,
}

#[derive(Debug, Clone, Args, Serialize, Deserialize)]
pub struct MultivariateClusterArgs {
    #[arg(long)]
    pub data: Option<PathBuf>,
    #[arg(long)]
    pub analysis: Option<PathBuf>,
    #[arg(long, value_delimiter = ',')]
    pub vars: Vec<String>,
    #[arg(long)]
    pub k: usize,
    /// Clustering method: kmeans (default) or hierarchical.
    #[arg(long, default_value = "kmeans")]
    pub method: String,
    #[arg(long)]
    pub seed: Option<u64>,
}

#[derive(Debug, Clone, Args, Serialize, Deserialize)]
pub struct SampleSizeLogRankArgs {
    #[arg(long)]
    pub median1: f64,
    #[arg(long)]
    pub median2: f64,
    #[arg(long)]
    pub accrual: f64,
    #[arg(long)]
    pub followup: f64,
    #[arg(long, default_value_t = 0.8)]
    pub power: f64,
    #[arg(long, default_value_t = 1.0)]
    pub allocation_ratio: f64,
    #[arg(long)]
    pub dropout_rate: Option<f64>,
}

#[derive(Debug, Clone, Args, Serialize, Deserialize)]
pub struct StatsSurvivalLifetableArgs {
    #[arg(long)]
    pub data: Option<PathBuf>,
    #[arg(long)]
    pub analysis: Option<PathBuf>,
    #[arg(long)]
    pub intervals: String,
    #[arg(long)]
    pub events: Option<String>,
    #[arg(long)]
    pub withdrawals: Option<String>,
    #[arg(long)]
    pub entering: Option<String>,
    #[arg(long)]
    pub time: Option<String>,
    #[arg(long)]
    pub status: Option<String>,
    /// Input format: grouped (default) or individual.
    #[arg(long, default_value = "grouped")]
    pub input_format: String,
}

#[derive(Debug, Clone, Args, Serialize, Deserialize)]
pub struct StatsSurvivalCompetingArgs {
    #[arg(long)]
    pub data: Option<PathBuf>,
    #[arg(long)]
    pub analysis: Option<PathBuf>,
    #[arg(long)]
    pub time: String,
    #[arg(long)]
    pub event_type: String,
    #[arg(long)]
    pub cause: String,
    #[arg(long, value_delimiter = ',')]
    pub x: Vec<String>,
    #[arg(long)]
    #[serde(default)]
    pub point_estimate_only: bool,
}

#[derive(Debug, Clone, Args, Serialize, Deserialize)]
pub struct StatsModelPoissonArgs {
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
    /// Offset column (already on log scale).
    #[arg(long)]
    pub offset: Option<String>,
    /// Exposure column (raw; internally log-transformed).
    #[arg(long)]
    pub exposure: Option<String>,
}

#[derive(Debug, Clone, Args, Serialize, Deserialize)]
pub struct StatsModelOrdinalArgs {
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
}

#[derive(Debug, Clone, Args, Serialize, Deserialize)]
pub struct StatsModelMultinomialArgs {
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
    #[arg(long)]
    pub reference: Option<String>,
}

#[derive(Debug, Clone, Args, Serialize, Deserialize)]
pub struct StatsCorrelationArgs {
    #[arg(long)]
    pub data: Option<PathBuf>,
    #[arg(long)]
    pub analysis: Option<PathBuf>,
    #[arg(long)]
    pub x: String,
    #[arg(long)]
    pub y: String,
    /// Correlation method: pearson, spearman, or both.
    #[arg(long, default_value = "both")]
    pub method: String,
}

#[derive(Debug, Clone, Args, Serialize, Deserialize)]
pub struct StatsMetaArgs {
    #[arg(long)]
    pub data: Option<PathBuf>,
    #[arg(long)]
    pub analysis: Option<PathBuf>,
    #[arg(long)]
    pub effect: String,
    #[arg(long)]
    pub se: String,
    #[arg(long)]
    pub study_label: Option<String>,
    /// Model: fixed, random, or both (default).
    #[arg(long, default_value = "both")]
    pub model: String,
}

#[derive(Debug, Clone, Args, Serialize, Deserialize)]
pub struct StatsMixedArgs {
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
    #[arg(long)]
    pub random: String,
}

#[derive(Debug, Clone, Args, Serialize, Deserialize)]
pub struct StatsPsmArgs {
    #[arg(long)]
    pub data: Option<PathBuf>,
    #[arg(long)]
    pub analysis: Option<PathBuf>,
    #[arg(long)]
    pub treatment: String,
    #[arg(long, value_delimiter = ',')]
    pub covariates: Vec<String>,
    #[arg(long, default_value_t = 0.2)]
    pub caliper: f64,
    #[arg(long, default_value_t = 1)]
    pub ratio: usize,
    #[arg(long)]
    pub seed: Option<u64>,
    #[arg(long)]
    pub output: Option<PathBuf>,
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
