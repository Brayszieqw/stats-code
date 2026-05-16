use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::contract::AnalysisCheckItem;
use super::types::{DataFormat, VariableKind};

// ---------------------------------------------------------------------------
// result_prelude! — injects the standard header fields shared by every new
// statistical *Result struct (Requirements G1.5).
//
// Usage:
//   result_prelude!(MyResult {
//       pub extra_field: f64,
//   });
//
// Expands to a public struct with:
//   status, data_path, analysis_path, n_total, n_used, n_excluded_missing,
//   notes, warnings  — plus whatever extra fields are listed.
// ---------------------------------------------------------------------------
macro_rules! result_prelude {
    (
        $(#[$meta:meta])*
        pub struct $name:ident {
            $(pub $field:ident : $ty:ty,)*
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Serialize, Deserialize)]
        pub struct $name {
            pub status: String,
            pub data_path: String,
            pub analysis_path: Option<String>,
            pub n_total: usize,
            pub n_used: usize,
            pub n_excluded_missing: usize,
            pub notes: Vec<String>,
            pub warnings: Vec<String>,
            $(pub $field: $ty,)*
        }
    };
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NumericSummary {
    pub min: f64,
    pub max: f64,
    pub mean: f64,
    pub zero_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnInspection {
    pub name: String,
    pub inferred_kind: VariableKind,
    pub missing_count: usize,
    pub non_missing_count: usize,
    pub distinct_count: usize,
    pub sample_values: Vec<String>,
    #[serde(default)]
    pub numeric_summary: Option<NumericSummary>,
    #[serde(default)]
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InspectResult {
    pub status: String,
    pub data_path: String,
    pub format: DataFormat,
    pub rows: Option<usize>,
    pub columns: usize,
    pub variables: Vec<ColumnInspection>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateRow {
    pub stratum: String,
    pub total_records: usize,
    pub included_records: usize,
    pub events: f64,
    pub person_time: f64,
    pub rate: f64,
    pub rate_per_1000: f64,
    pub lower_ci_per_1000: f64,
    pub upper_ci_per_1000: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateResult {
    pub status: String,
    pub data_path: String,
    pub analysis_path: Option<String>,
    pub event: String,
    pub person_time: String,
    pub strata: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub survey_weight: Option<String>,
    pub rows: Vec<RateRow>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableOneCell {
    pub display: String,
    pub n_total: usize,
    pub n_non_missing: usize,
    pub missing_count: usize,
    #[serde(default)]
    pub count: Option<usize>,
    #[serde(default)]
    pub percent: Option<f64>,
    #[serde(default)]
    pub mean: Option<f64>,
    #[serde(default)]
    pub sd: Option<f64>,
    #[serde(default)]
    pub median: Option<f64>,
    #[serde(default)]
    pub q1: Option<f64>,
    #[serde(default)]
    pub q3: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weighted_count: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weighted_percent: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weighted_mean: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weighted_sd: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weight_sum: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableOneGroupCell {
    pub group: String,
    pub cell: TableOneCell,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableOneRow {
    pub variable: String,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub level: Option<String>,
    pub kind: VariableKind,
    pub overall: TableOneCell,
    pub groups: Vec<TableOneGroupCell>,
    #[serde(default)]
    pub test_name: Option<String>,
    #[serde(default)]
    pub p_value: Option<f64>,
    #[serde(default)]
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableOneResult {
    pub status: String,
    pub data_path: String,
    pub analysis_path: Option<String>,
    pub by: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub survey_weight: Option<String>,
    pub group_levels: Vec<String>,
    pub rows: Vec<TableOneRow>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogisticCoefficient {
    pub term: String,
    pub variable: String,
    #[serde(default)]
    pub level: Option<String>,
    #[serde(default)]
    pub reference: Option<String>,
    pub beta: f64,
    pub standard_error: f64,
    pub odds_ratio: f64,
    pub ci_lower: f64,
    pub ci_upper: f64,
    pub p_value: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticSeverity {
    Info,
    Warning,
    Blocking,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diagnostic {
    pub code: String,
    pub severity: DiagnosticSeverity,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence: Option<Value>,
}

impl Diagnostic {
    pub fn new(
        code: impl Into<String>,
        severity: DiagnosticSeverity,
        message: impl Into<String>,
        evidence: Option<Value>,
    ) -> Self {
        Self {
            code: code.into(),
            severity,
            message: message.into(),
            evidence,
        }
    }

    pub fn blocking(
        code: impl Into<String>,
        message: impl Into<String>,
        evidence: Option<Value>,
    ) -> Self {
        Self::new(code, DiagnosticSeverity::Blocking, message, evidence)
    }

    #[must_use]
    pub fn is_blocking(&self) -> bool {
        matches!(
            self.severity,
            DiagnosticSeverity::Blocking | DiagnosticSeverity::Error
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogisticResult {
    pub status: String,
    #[serde(default)]
    pub validity_status: String,
    pub data_path: String,
    pub analysis_path: Option<String>,
    pub formula: String,
    pub outcome: String,
    pub predictors: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub survey_weight: Option<String>,
    pub n_total: usize,
    pub n_used: usize,
    pub n_excluded_missing: usize,
    pub n_excluded_invalid: usize,
    pub n_events: usize,
    pub n_nonevents: usize,
    pub iterations: usize,
    pub converged: bool,
    pub log_likelihood: f64,
    #[serde(default)]
    pub null_log_likelihood: Option<f64>,
    #[serde(default)]
    pub pseudo_r2_nagelkerke: Option<f64>,
    #[serde(default)]
    pub aic: Option<f64>,
    #[serde(default)]
    pub bic: Option<f64>,
    #[serde(default)]
    pub c_statistic: Option<f64>,
    pub coefficients: Vec<LogisticCoefficient>,
    pub notes: Vec<String>,
    #[serde(default)]
    pub diagnostics: Vec<Diagnostic>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoxPhDiagnostic {
    pub term: String,
    pub correlation: f64,
    pub chi_square: f64,
    pub p_value: f64,
    pub event_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoxCoefficient {
    pub term: String,
    pub variable: String,
    #[serde(default)]
    pub level: Option<String>,
    #[serde(default)]
    pub reference: Option<String>,
    pub beta: f64,
    pub standard_error: f64,
    pub hazard_ratio: f64,
    pub ci_lower: f64,
    pub ci_upper: f64,
    pub p_value: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoxResult {
    pub status: String,
    pub data_path: String,
    pub analysis_path: Option<String>,
    pub formula: String,
    pub time: String,
    pub event: String,
    pub predictors: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub survey_weight: Option<String>,
    pub n_total: usize,
    pub n_used: usize,
    pub n_excluded_missing: usize,
    pub n_excluded_invalid: usize,
    pub n_events: usize,
    pub n_censored: usize,
    pub tied_event_times: usize,
    pub iterations: usize,
    pub converged: bool,
    pub log_partial_likelihood: f64,
    #[serde(default)]
    pub concordance: Option<f64>,
    pub coefficients: Vec<CoxCoefficient>,
    #[serde(default)]
    pub ph_diagnostics: Vec<CoxPhDiagnostic>,
    pub notes: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinearCoefficient {
    pub term: String,
    pub variable: String,
    #[serde(default)]
    pub level: Option<String>,
    #[serde(default)]
    pub reference: Option<String>,
    pub beta: f64,
    pub standard_error: f64,
    pub t_statistic: f64,
    pub ci_lower: f64,
    pub ci_upper: f64,
    pub p_value: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinearResult {
    pub status: String,
    pub data_path: String,
    pub analysis_path: Option<String>,
    pub formula: String,
    pub outcome: String,
    pub predictors: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub survey_weight: Option<String>,
    pub n_total: usize,
    pub n_used: usize,
    pub n_excluded_missing: usize,
    pub n_excluded_invalid: usize,
    pub converged: bool,
    pub r_squared: f64,
    pub adjusted_r_squared: f64,
    pub f_statistic: Option<f64>,
    pub f_p_value: Option<f64>,
    pub residual_std_error: f64,
    #[serde(default)]
    pub aic: Option<f64>,
    #[serde(default)]
    pub bic: Option<f64>,
    pub coefficients: Vec<LinearCoefficient>,
    pub notes: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RocPoint {
    pub threshold: f64,
    pub sensitivity: f64,
    pub specificity: f64,
    pub false_positive_rate: f64,
    pub true_positive_rate: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticThresholdMetrics {
    pub threshold: f64,
    pub tp: usize,
    pub fp: usize,
    pub tn: usize,
    pub fn_count: usize,
    pub sensitivity: f64,
    pub specificity: f64,
    pub ppv: f64,
    pub npv: f64,
    pub accuracy: f64,
    pub balanced_accuracy: f64,
    pub f1_score: f64,
    pub positive_likelihood_ratio: Option<f64>,
    pub negative_likelihood_ratio: Option<f64>,
    pub diagnostic_odds_ratio: Option<f64>,
    pub youden_j: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticRocResult {
    pub status: String,
    pub data_path: String,
    pub analysis_path: Option<String>,
    pub truth: String,
    pub score: String,
    pub n_total: usize,
    pub n_used: usize,
    pub n_excluded_missing: usize,
    pub n_excluded_invalid: usize,
    pub n_cases: usize,
    pub n_controls: usize,
    pub auc: f64,
    pub roc_points: Vec<RocPoint>,
    pub youden: DiagnosticThresholdMetrics,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub threshold_metrics: Option<DiagnosticThresholdMetrics>,
    pub notes: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PowerResult {
    pub status: String,
    pub method: String,
    pub alpha: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub power: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allocation_ratio: Option<f64>,
    pub total_n: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group1_n: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group2_n: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effect_size: Option<f64>,
    pub notes: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SurvivalKmStep {
    pub group: String,
    pub time: f64,
    pub n_risk: usize,
    pub n_event: usize,
    pub n_censored: usize,
    pub survival: f64,
    pub standard_error: f64,
    pub ci_lower: f64,
    pub ci_upper: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogRankResult {
    pub chi_square: f64,
    pub degrees_freedom: usize,
    pub p_value: f64,
    pub groups: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SurvivalKmResult {
    pub status: String,
    pub data_path: String,
    pub analysis_path: Option<String>,
    pub time: String,
    pub event: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    pub n_total: usize,
    pub n_used: usize,
    pub n_excluded_missing: usize,
    pub n_excluded_invalid: usize,
    pub groups: Vec<String>,
    pub steps: Vec<SurvivalKmStep>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub log_rank: Option<LogRankResult>,
    pub notes: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlannedCommandResult {
    pub status: String,
    pub command: String,
    pub data_path: String,
    pub analysis_path: Option<String>,
    pub formula: Option<String>,
    pub expected_outputs: Vec<String>,
    pub notes: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportBuildResult {
    pub status: String,
    pub analysis_path: String,
    pub output_dir: String,
    pub written_files: Vec<String>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportVerifyResult {
    pub status: String,
    pub artifacts_dir: String,
    pub accepted_count: usize,
    pub rejected_count: usize,
    pub error_count: usize,
    pub warning_count: usize,
    pub items: Vec<AnalysisCheckItem>,
    pub notes: Vec<String>,
}

impl ReportVerifyResult {
    #[must_use]
    pub fn has_errors(&self) -> bool {
        self.error_count > 0
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowStepRunResult {
    pub step_index: usize,
    pub command: String,
    pub artifact_dir: String,
    pub status: String,
    pub notes: Vec<String>,
}

/// Result of executing a full analysis workflow (inspect → model → report).
///
/// Contains the overall status, individual step results, and the final report
/// build output.
///
/// # Examples
///
/// ```no_run
/// use stats_code::WorkflowRunResult;
///
/// // After running a workflow, check if any step failed:
/// # fn example(result: WorkflowRunResult) {
/// let has_errors = result.steps.iter().any(|s| s.status != "ok");
/// if has_errors {
///     eprintln!("Workflow {} completed with errors", result.run_id);
/// } else {
///     println!("Workflow {} succeeded — report in {}", result.run_id, result.report_output_dir);
/// }
/// # }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowRunResult {
    pub status: String,
    pub run_id: String,
    pub analysis_path: String,
    pub data_path: String,
    pub artifacts_dir: String,
    pub report_output_dir: String,
    pub steps: Vec<WorkflowStepRunResult>,
    pub report: ReportBuildResult,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitProjectResult {
    pub status: String,
    pub project_dir: String,
    pub analysis_path: String,
    pub data_dir: String,
    pub written_files: Vec<String>,
    pub next_steps: Vec<String>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorResult {
    pub status: String,
    pub version: String,
    pub current_dir: String,
    pub executable: String,
    pub error_count: usize,
    pub warning_count: usize,
    pub items: Vec<AnalysisCheckItem>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditExplainArtifact {
    pub command: String,
    pub status: String,
    #[serde(default)]
    pub report_decision: Option<String>,
    #[serde(default)]
    pub analysis_step_index: Option<usize>,
    pub reason: String,
    #[serde(default)]
    pub result_path: Option<String>,
    #[serde(default)]
    pub context_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditExplainResult {
    pub status: String,
    pub artifacts_dir: String,
    pub evidence_index_path: String,
    pub accepted_count: usize,
    pub rejected_count: usize,
    pub policy_exception_count: usize,
    pub accepted_artifacts: Vec<AuditExplainArtifact>,
    pub rejected_artifacts: Vec<AuditExplainArtifact>,
    pub policy_exceptions: Vec<String>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenReportResult {
    pub status: String,
    pub artifacts_dir: String,
    pub report_path: String,
    pub opened: bool,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthSetResult {
    pub status: String,
    pub provider: String,
    pub config_path: String,
    pub api_key_env: String,
    #[serde(default)]
    pub base_url_env: Option<String>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthDoctorProviderStatus {
    pub provider: String,
    pub model_hint: String,
    pub api_key_env: String,
    #[serde(default)]
    pub base_url_env: Option<String>,
    pub credential_source: String,
    pub api_key_present: bool,
    #[serde(default)]
    pub base_url_present: bool,
    #[serde(default)]
    pub configured_base_url: Option<String>,
    #[serde(default)]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthDoctorResult {
    pub status: String,
    pub config_path: String,
    pub providers: Vec<AuthDoctorProviderStatus>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiAskResult {
    pub status: String,
    pub provider: String,
    pub credential_source: String,
    pub model: String,
    pub prompt: String,
    pub response_text: String,
    #[serde(default)]
    pub request_id: Option<String>,
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub total_tokens: u32,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigResult {
    pub status: String,
    pub action: String,
    pub config_path: String,
    #[serde(default)]
    pub default_model: Option<String>,
    #[serde(default)]
    pub saved_models: Vec<String>,
    pub message: String,
    pub notes: Vec<String>,
}

// ============================================================================
// New statistical-methods result structs (Requirements G1.5, task 3.1)
// ============================================================================

// --- T-Tests (Req 1, 2) -----------------------------------------------------

result_prelude! {
    pub struct TtestPairedResult {
        pub method: String,
        pub before_variable: String,
        pub after_variable: String,
        pub n_pairs: usize,
        pub mean_diff: f64,
        pub sd_diff: f64,
        pub se_diff: f64,
        pub t_statistic: f64,
        pub df: f64,
        pub p_value: f64,
        pub ci_lower: f64,
        pub ci_upper: f64,
        pub alpha: f64,
    }
}

result_prelude! {
    pub struct TtestOneSampleResult {
        pub method: String,
        pub variable: String,
        pub hypothesized_mean: f64,
        pub n: usize,
        pub sample_mean: f64,
        pub sample_sd: f64,
        pub se: f64,
        pub t_statistic: f64,
        pub df: f64,
        pub p_value: f64,
        pub ci_lower: f64,
        pub ci_upper: f64,
        pub alpha: f64,
    }
}

// --- ANOVA Family (Req 3, 11, 12) -------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnovaGroupSummary {
    pub group: String,
    pub n: usize,
    pub mean: f64,
    pub sd: f64,
}

result_prelude! {
    pub struct OneWayAnovaResult {
        pub variable: String,
        pub group: String,
        pub overall_mean: f64,
        pub groups: Vec<AnovaGroupSummary>,
        pub ss_between: f64,
        pub ss_within: f64,
        pub ss_total: f64,
        pub df_between: usize,
        pub df_within: usize,
        pub ms_between: f64,
        pub ms_within: f64,
        pub f_statistic: f64,
        pub p_value: f64,
    }
}

result_prelude! {
    pub struct RbdAnovaResult {
        pub variable: String,
        pub group: String,
        pub block: String,
        pub treatment_f: f64,
        pub treatment_df1: usize,
        pub treatment_df2: usize,
        pub treatment_p: f64,
        pub block_f: f64,
        pub block_df1: usize,
        pub block_df2: usize,
        pub block_p: f64,
        pub error_ms: f64,
    }
}

result_prelude! {
    pub struct RepeatedAnovaResult {
        pub variable: String,
        pub subject: String,
        pub time: String,
        pub n_subjects: usize,
        pub n_timepoints: usize,
        pub time_f: f64,
        pub time_df1: usize,
        pub time_df2: usize,
        pub time_p: f64,
        pub mauchly_w: Option<f64>,
        pub mauchly_p: Option<f64>,
        pub gg_epsilon: Option<f64>,
        pub gg_df1: Option<f64>,
        pub gg_df2: Option<f64>,
        pub gg_p: Option<f64>,
        pub hf_epsilon: Option<f64>,
        pub hf_df1: Option<f64>,
        pub hf_df2: Option<f64>,
        pub hf_p: Option<f64>,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PosthocPair {
    pub group_a: String,
    pub group_b: String,
    pub mean_difference: f64,
    pub standard_error: f64,
    pub test_statistic: f64,
    pub adjusted_p_value: f64,
    pub ci_lower: f64,
    pub ci_upper: f64,
}

result_prelude! {
    pub struct PosthocResult {
        pub variable: String,
        pub group: String,
        pub method: String,
        pub pairs: Vec<PosthocPair>,
    }
}

// --- Nonparametric Tests (Req 5, 6, 7) --------------------------------------

result_prelude! {
    pub struct McNemarResult {
        pub var1: String,
        pub var2: String,
        pub b: usize,
        pub c: usize,
        pub n_concordant: usize,
        pub chi_square: f64,
        pub continuity_correction_used: bool,
        pub p_value: f64,
        pub exact_p_value: Option<f64>,
    }
}

result_prelude! {
    pub struct WilcoxonSignedRankResult {
        pub var1: String,
        pub var2: String,
        pub w_plus: f64,
        pub expected_w: f64,
        pub variance_w: f64,
        pub z_statistic: f64,
        pub p_value: f64,
        pub n_zero_pairs_excluded: usize,
        pub n_ties_corrected: usize,
    }
}

result_prelude! {
    pub struct MannWhitneyResult {
        pub variable: String,
        pub group: String,
        pub group_a_label: String,
        pub group_b_label: String,
        pub n_a: usize,
        pub n_b: usize,
        pub median_a: f64,
        pub median_b: f64,
        pub u_statistic: f64,
        pub z_statistic: f64,
        pub p_value: f64,
    }
}

// --- Trend (Req 4) ----------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryProportion {
    pub category: String,
    pub score: f64,
    pub n: usize,
    pub events: usize,
    pub proportion: f64,
}

result_prelude! {
    pub struct CochranArmitageResult {
        pub exposure: String,
        pub outcome: String,
        pub categories: Vec<CategoryProportion>,
        pub trend_statistic: f64,
        pub p_value: f64,
    }
}

// --- Correlation (Req 8) ----------------------------------------------------

result_prelude! {
    pub struct CorrelationResult {
        pub method: String,
        pub x_variable: String,
        pub y_variable: String,
        pub n_pairs: usize,
        pub r: f64,
        pub r_squared: f64,
        pub se_fisher_z: f64,
        pub ci_lower: f64,
        pub ci_upper: f64,
        pub t_statistic: f64,
        pub df: f64,
        pub p_value: f64,
        pub alpha: f64,
        pub spearman_rho: Option<f64>,
        pub spearman_p_value: Option<f64>,
    }
}

// --- Normality (Req 16) -----------------------------------------------------

result_prelude! {
    pub struct NormalityResult {
        pub variable: String,
        pub n: usize,
        pub skewness: f64,
        pub kurtosis: f64,
        pub shapiro_w: Option<f64>,
        pub shapiro_p: Option<f64>,
        pub shapiro_p_unreliable: bool,
        pub ks_d: f64,
        pub ks_p: f64,
        pub lilliefors_used: bool,
    }
}

// --- Variance Homogeneity (Req 17) ------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupVarianceSummary {
    pub group: String,
    pub n: usize,
    pub variance: f64,
    pub sd: f64,
}

result_prelude! {
    pub struct VarianceHomogeneityResult {
        pub variable: String,
        pub group: String,
        pub groups: Vec<GroupVarianceSummary>,
        pub levene_statistic: f64,
        pub levene_p: f64,
        pub bartlett_statistic: f64,
        pub bartlett_p: f64,
    }
}

// --- Epidemiology (Req 9, 10, 19, 20) ---------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TwoByTwoCells {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub d: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MhStratum {
    pub label: String,
    pub cells: TwoByTwoCells,
    pub or_stratum: f64,
}

result_prelude! {
    pub struct OrRrResult {
        pub exposure: String,
        pub outcome: String,
        pub cells: TwoByTwoCells,
        pub continuity_correction: bool,
        pub odds_ratio: f64,
        pub or_ci_lower: f64,
        pub or_ci_upper: f64,
        pub relative_risk: f64,
        pub rr_ci_lower: f64,
        pub rr_ci_upper: f64,
        pub chi_square: f64,
        pub chi_p_value: f64,
        pub mh_or: Option<f64>,
        pub mh_or_ci_lower: Option<f64>,
        pub mh_or_ci_upper: Option<f64>,
        pub mh_strata: Vec<MhStratum>,
        pub homogeneity_p: Option<f64>,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StandardizationStratum {
    pub age_group: String,
    pub observed: f64,
    pub expected: f64,
    pub weight: f64,
    pub stratum_rate: f64,
}

result_prelude! {
    pub struct StandardizationResult {
        pub method: String,
        pub strata: Vec<StandardizationStratum>,
        pub standardized_rate: Option<f64>,
        pub direct_ci_lower: Option<f64>,
        pub direct_ci_upper: Option<f64>,
        pub smr: Option<f64>,
        pub smr_ci_lower: Option<f64>,
        pub smr_ci_upper: Option<f64>,
    }
}

result_prelude! {
    pub struct AttributableRiskResult {
        pub exposure: String,
        pub outcome: String,
        pub rate_exposed: f64,
        pub rate_unexposed: f64,
        pub ar: f64,
        pub ar_ci_lower: f64,
        pub ar_ci_upper: f64,
        pub ar_percent: f64,
        pub par: Option<f64>,
        pub par_ci_lower: Option<f64>,
        pub par_ci_upper: Option<f64>,
        pub par_percent: Option<f64>,
        pub exposure_prevalence: Option<f64>,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoseResponseCategory {
    pub category: String,
    pub score: f64,
    pub events: usize,
    pub person_time: f64,
    pub rate: f64,
    pub rate_ratio: f64,
    pub rr_ci_lower: f64,
    pub rr_ci_upper: f64,
}

result_prelude! {
    pub struct DoseResponseResult {
        pub exposure: String,
        pub outcome: String,
        pub categories: Vec<DoseResponseCategory>,
        pub trend_beta: f64,
        pub trend_se: f64,
        pub trend_ci_lower: f64,
        pub trend_ci_upper: f64,
        pub trend_p_value: f64,
        pub linearity_p_value: f64,
    }
}

// --- GLMs (Req 13, 14, 15) --------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoissonCoefficient {
    pub term: String,
    pub variable: String,
    pub beta: f64,
    pub standard_error: f64,
    pub irr: f64,
    pub ci_lower: f64,
    pub ci_upper: f64,
    pub p_value: f64,
}

result_prelude! {
    pub struct PoissonResult {
        pub outcome: String,
        pub predictors: Vec<String>,
        pub offset: Option<String>,
        pub offset_kind: String,
        pub iterations: usize,
        pub converged: bool,
        pub log_likelihood: f64,
        pub deviance: f64,
        pub pearson_chi_square: f64,
        pub aic: f64,
        pub coefficients: Vec<PoissonCoefficient>,
    }
}

result_prelude! {
    pub struct OrdinalLogitResult {
        pub outcome: String,
        pub predictors: Vec<String>,
        pub thresholds: Vec<f64>,
        pub coefficients: Vec<LogisticCoefficient>,
        pub brant_chi_square: Option<f64>,
        pub brant_p: Option<f64>,
        pub log_likelihood: f64,
        pub aic: f64,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultinomialCoefficientGroup {
    pub category: String,
    pub coefficients: Vec<LogisticCoefficient>,
}

result_prelude! {
    pub struct MultinomialLogitResult {
        pub outcome: String,
        pub predictors: Vec<String>,
        pub reference: String,
        pub categories: Vec<String>,
        pub coefficients_per_category: Vec<MultinomialCoefficientGroup>,
        pub log_likelihood: f64,
        pub aic: f64,
        pub pseudo_r2: f64,
    }
}

// --- Mixed Effects (Req 27) -------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MixedFixedEffect {
    pub term: String,
    pub estimate: f64,
    pub standard_error: f64,
    pub ci_lower: f64,
    pub ci_upper: f64,
    pub p_value: f64,
}

result_prelude! {
    pub struct MixedLmmResult {
        pub outcome: String,
        pub predictors: Vec<String>,
        pub random_group: String,
        pub n_groups: usize,
        pub iterations: usize,
        pub converged: bool,
        pub fixed_effects: Vec<MixedFixedEffect>,
        pub random_intercept_variance: f64,
        pub residual_variance: f64,
        pub icc: f64,
        pub log_likelihood: f64,
        pub aic: f64,
        pub bic: f64,
    }
}

// --- Survival Extensions (Req 18, 29) ---------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LifeTableRow {
    pub interval_index: usize,
    pub start: f64,
    pub end: f64,
    pub entering: usize,
    pub withdrawals: usize,
    pub events: usize,
    pub effective_at_risk: f64,
    pub conditional_survival: f64,
    pub cumulative_survival: f64,
    pub se_cumulative: f64,
    pub ci_lower: f64,
    pub ci_upper: f64,
    pub hazard_rate: f64,
    pub cumulative_hazard: f64,
}

result_prelude! {
    pub struct LifeTableResult {
        pub time: String,
        pub intervals: Vec<LifeTableRow>,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompetingRisksCauseFit {
    pub cause: String,
    pub coefficients: Vec<CoxCoefficient>,
    pub log_partial_likelihood: f64,
    pub n_events: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompetingRisksCif {
    pub time: f64,
    pub cif: f64,
    pub se: f64,
}

result_prelude! {
    pub struct CompetingRisksResult {
        pub time: String,
        pub event_type: String,
        pub causes: Vec<String>,
        pub cause_fits: Vec<CompetingRisksCauseFit>,
        pub cif_curves: BTreeMap<String, Vec<CompetingRisksCif>>,
        pub gray_chi_square: Option<f64>,
        pub gray_df: Option<usize>,
        pub gray_p: Option<f64>,
    }
}

// --- Power / Sample Size (Req 30) -------------------------------------------
// Reuses existing PowerResult; PowerLogRankResult is a type alias for clarity.
pub type PowerLogRankResult = PowerResult;

// --- Meta-Analysis (Req 21) -------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetaStudy {
    pub label: String,
    pub effect: f64,
    pub se: f64,
    pub weight_fixed: f64,
    pub weight_random: f64,
}

result_prelude! {
    pub struct MetaAnalysisResult {
        pub studies: Vec<MetaStudy>,
        pub fixed_effect: f64,
        pub fixed_ci_lower: f64,
        pub fixed_ci_upper: f64,
        pub fixed_z: f64,
        pub fixed_p: f64,
        pub random_effect: f64,
        pub random_ci_lower: f64,
        pub random_ci_upper: f64,
        pub random_z: f64,
        pub random_p: f64,
        pub q_statistic: f64,
        pub q_df: usize,
        pub q_p: f64,
        pub i_squared: f64,
        pub tau_squared: f64,
    }
}

// --- Agreement (Req 22, 23) -------------------------------------------------

result_prelude! {
    pub struct KappaResult {
        pub rater1: String,
        pub rater2: String,
        pub categories: Vec<String>,
        pub agreement_matrix: Vec<Vec<usize>>,
        pub observed_agreement: f64,
        pub expected_agreement: f64,
        pub kappa: f64,
        pub kappa_se: f64,
        pub kappa_ci_lower: f64,
        pub kappa_ci_upper: f64,
        pub weighted_kappa: Option<f64>,
        pub weights_kind: String,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlandAltmanPoint {
    pub mean: f64,
    pub diff: f64,
}

result_prelude! {
    pub struct BlandAltmanResult {
        pub method1: String,
        pub method2: String,
        pub n: usize,
        pub bias: f64,
        pub bias_ci_lower: f64,
        pub bias_ci_upper: f64,
        pub sd_difference: f64,
        pub loa_lower: f64,
        pub loa_upper: f64,
        pub loa_lower_ci_lower: f64,
        pub loa_lower_ci_upper: f64,
        pub loa_upper_ci_lower: f64,
        pub loa_upper_ci_upper: f64,
        pub n_outside_loa: usize,
        pub points: Vec<BlandAltmanPoint>,
    }
}

// --- Multivariate (Req 24, 25, 26) ------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PcaComponent {
    pub component: usize,
    pub eigenvalue: f64,
    pub variance_explained: f64,
    pub cumulative_variance: f64,
}

result_prelude! {
    pub struct PcaResult {
        pub variables: Vec<String>,
        pub components: Vec<PcaComponent>,
        pub loadings: Vec<Vec<f64>>,
        pub kmo: f64,
        pub bartlett_chi_square: f64,
        pub bartlett_df: usize,
        pub bartlett_p: f64,
        pub excluded_variables: Vec<String>,
    }
}

result_prelude! {
    pub struct LdaResult {
        pub group: String,
        pub groups: Vec<String>,
        pub variables: Vec<String>,
        pub wilks_lambda: f64,
        pub wilks_chi_square: f64,
        pub wilks_p: f64,
        pub function_coefficients: Vec<Vec<f64>>,
        pub standardized_coefficients: Vec<Vec<f64>>,
        pub centroids: Vec<Vec<f64>>,
        pub confusion_matrix: Vec<Vec<usize>>,
        pub correct_rate_per_group: Vec<f64>,
        pub overall_correct_rate: f64,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterAssignment {
    pub row_index: usize,
    pub cluster: usize,
    pub silhouette: f64,
}

result_prelude! {
    pub struct ClusterResult {
        pub method: String,
        pub k: usize,
        pub variables: Vec<String>,
        pub assignments: Vec<usize>,
        pub centroids: Vec<Vec<f64>>,
        pub within_cluster_ss: Vec<f64>,
        pub total_within_ss: f64,
        pub silhouette_per_observation: Vec<f64>,
        pub silhouette_avg: f64,
        pub merge_distances: Vec<f64>,
        pub excluded_variables: Vec<String>,
    }
}

// --- Propensity Score Matching (Req 28) -------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PsmCovariateSmd {
    pub covariate: String,
    pub smd_before: f64,
    pub smd_after: f64,
}

result_prelude! {
    pub struct PsmResult {
        pub treatment: String,
        pub covariates: Vec<String>,
        pub caliper: f64,
        pub ratio: usize,
        pub n_treated: usize,
        pub n_control: usize,
        pub n_matched_pairs: usize,
        pub n_unmatched_treated: usize,
        pub n_unmatched_control: usize,
        pub balance: Vec<PsmCovariateSmd>,
        pub matched_dataset_path: String,
    }
}
