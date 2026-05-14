use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::contract::AnalysisCheckItem;
use super::types::{DataFormat, VariableKind};

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
