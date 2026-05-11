use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DataFormat {
    Csv,
    Excel,
    Parquet,
    Xpt,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VariableKind {
    Continuous,
    Categorical,
    Ordered,
    Binary,
    Time,
    Date,
    PersonTime,
    Event,
    Identifier,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VariableRole {
    Outcome,
    Exposure,
    Covariate,
    Strata,
    Time,
    Event,
    Id,
    Weight,
    Cluster,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnalysisKind {
    Inspect,
    TableOne,
    Rate,
    Model,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelKind {
    Logistic,
    Cox,
    Linear,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactRole {
    Declared,
    Exploratory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactStatus {
    Produced,
    Accepted,
    Rejected,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactMetadata {
    pub role: ArtifactRole,
    pub status: ArtifactStatus,
    #[serde(default)]
    pub formal_run_id: Option<String>,
    #[serde(default)]
    pub analysis_step_index: Option<usize>,
}

impl ArtifactMetadata {
    pub fn declared(formal_run_id: &str, analysis_step_index: usize) -> Self {
        Self {
            role: ArtifactRole::Declared,
            status: ArtifactStatus::Produced,
            formal_run_id: Some(formal_run_id.to_string()),
            analysis_step_index: Some(analysis_step_index),
        }
    }

    pub fn exploratory() -> Self {
        Self {
            role: ArtifactRole::Exploratory,
            status: ArtifactStatus::Produced,
            formal_run_id: None,
            analysis_step_index: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StudySpec {
    pub title: String,
    pub design: String,
    #[serde(default)]
    pub population: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StudyContextSpec {
    #[serde(default)]
    pub estimand: Option<String>,
    #[serde(default)]
    pub exposure: Option<String>,
    #[serde(default)]
    pub comparator: Option<String>,
    #[serde(default)]
    pub outcome: Option<String>,
    #[serde(default)]
    pub time_zero: Option<String>,
    #[serde(default)]
    pub follow_up: Option<String>,
    #[serde(default)]
    pub censoring: Option<String>,
    #[serde(default)]
    pub missing_data_strategy: Option<String>,
    #[serde(default)]
    pub clustering: Option<String>,
    #[serde(default)]
    pub sensitivity_analyses: Option<String>,
    #[serde(default)]
    pub reporting_guideline: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataSourceSpec {
    pub path: PathBuf,
    pub format: DataFormat,
    #[serde(default)]
    pub id_column: Option<String>,
    #[serde(default)]
    pub dictionary_path: Option<PathBuf>,
    #[serde(default)]
    pub encoding: Option<String>,
    #[serde(default)]
    pub sheet: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodingSpec {
    #[serde(default)]
    pub reference: Option<String>,
    #[serde(default)]
    pub levels: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MissingSpec {
    #[serde(default)]
    pub codes: Vec<String>,
    #[serde(default)]
    pub strategy: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VariableSpec {
    pub name: String,
    #[serde(default)]
    pub label: Option<String>,
    pub kind: VariableKind,
    #[serde(default)]
    pub roles: Vec<VariableRole>,
    #[serde(default)]
    pub coding: Option<CodingSpec>,
    #[serde(default)]
    pub missing: Option<MissingSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SurveyDesignSpec {
    #[serde(default)]
    pub weight: Option<String>,
    #[serde(default)]
    pub strata: Option<String>,
    #[serde(default)]
    pub cluster: Option<String>,
    #[serde(default)]
    pub replicate_weights: Vec<String>,
    #[serde(default)]
    pub variance_estimator: Option<String>,
    #[serde(default)]
    pub combined_cycles: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrivacySpec {
    #[serde(default)]
    pub deidentify: bool,
    #[serde(default)]
    pub direct_identifiers: Vec<String>,
    #[serde(default)]
    pub quasi_identifiers: Vec<String>,
    #[serde(default)]
    pub small_cell_threshold: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisStepSpec {
    #[serde(default)]
    pub id: Option<String>,
    pub kind: AnalysisKind,
    #[serde(default)]
    pub model: Option<ModelKind>,
    #[serde(default)]
    pub by: Option<String>,
    #[serde(default)]
    pub event: Option<String>,
    #[serde(default)]
    pub person_time: Option<String>,
    #[serde(default)]
    pub outcome: Option<String>,
    #[serde(default)]
    pub time: Option<String>,
    #[serde(default)]
    pub predictors: Vec<String>,
    #[serde(default)]
    pub adjust: Vec<String>,
    #[serde(default)]
    pub strata: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportSpec {
    pub out_dir: PathBuf,
    #[serde(default)]
    pub include_methods: bool,
    #[serde(default)]
    pub include_tables: bool,
    #[serde(default)]
    pub include_assumptions: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditSpec {
    pub log_dir: PathBuf,
    #[serde(default)]
    pub save_commands: bool,
    #[serde(default)]
    pub save_inputs: bool,
    #[serde(default)]
    pub save_outputs: bool,
    #[serde(default)]
    pub save_environment: bool,
    #[serde(default)]
    pub save_decisions: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisSpec {
    #[serde(default)]
    pub schema_version: Option<String>,
    pub study: StudySpec,
    #[serde(default)]
    pub study_context: StudyContextSpec,
    pub data: DataSourceSpec,
    #[serde(default)]
    pub variables: Vec<VariableSpec>,
    #[serde(default)]
    pub survey: Option<SurveyDesignSpec>,
    #[serde(default)]
    pub privacy: Option<PrivacySpec>,
    #[serde(default)]
    pub analyses: Vec<AnalysisStepSpec>,
    #[serde(default)]
    pub report: Option<ReportSpec>,
    #[serde(default)]
    pub audit: Option<AuditSpec>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnalysisCheckLevel {
    Ok,
    Warning,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisCheckItem {
    pub level: AnalysisCheckLevel,
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisCheckResult {
    pub status: String,
    pub analysis_path: String,
    pub data_path: String,
    pub error_count: usize,
    pub warning_count: usize,
    pub items: Vec<AnalysisCheckItem>,
    pub notes: Vec<String>,
}

impl AnalysisCheckResult {
    #[must_use]
    pub fn has_errors(&self) -> bool {
        self.error_count > 0
    }
}

pub fn validate_study_context(spec: &AnalysisSpec) -> Vec<String> {
    let mut issues = Vec::new();
    let has_declared_analyses = !spec.analyses.is_empty();
    let needs_estimand = spec
        .analyses
        .iter()
        .any(|step| !matches!(step.kind, AnalysisKind::Inspect));
    let needs_outcome = spec.variables.iter().any(|variable| {
        variable.roles.contains(&VariableRole::Outcome)
            || variable.roles.contains(&VariableRole::Event)
    }) || spec
        .analyses
        .iter()
        .any(|step| step.outcome.is_some() || step.event.is_some());
    let needs_exposure = spec
        .variables
        .iter()
        .any(|variable| variable.roles.contains(&VariableRole::Exposure))
        || spec.study.design.to_ascii_lowercase().contains("trial");
    let needs_comparator = needs_exposure;
    let needs_time_anchor = spec.analyses.iter().any(|step| {
        matches!(step.model, Some(ModelKind::Cox))
            || matches!(step.kind, AnalysisKind::Rate)
            || step.time.is_some()
            || step.event.is_some()
            || step.person_time.is_some()
    });
    let needs_clustering = spec
        .survey
        .as_ref()
        .and_then(|survey| survey.cluster.as_ref())
        .is_some()
        || spec
            .variables
            .iter()
            .any(|variable| variable.roles.contains(&VariableRole::Cluster));

    if needs_estimand && is_blank_option(spec.study_context.estimand.as_deref()) {
        issues.push(
            "study_context.estimand is required for declared analyses beyond inspect".to_string(),
        );
    }
    if needs_outcome && is_blank_option(spec.study_context.outcome.as_deref()) {
        issues.push(
            "study_context.outcome is required because outcomes/events are declared".to_string(),
        );
    }
    if needs_exposure && is_blank_option(spec.study_context.exposure.as_deref()) {
        issues.push(
            "study_context.exposure is required because an exposure or intervention is declared"
                .to_string(),
        );
    }
    if needs_comparator && is_blank_option(spec.study_context.comparator.as_deref()) {
        issues.push(
            "study_context.comparator is required because a comparison strategy is declared"
                .to_string(),
        );
    }
    if needs_time_anchor && is_blank_option(spec.study_context.time_zero.as_deref()) {
        issues.push(
            "study_context.time_zero is required for rate or time-to-event analyses".to_string(),
        );
    }
    if needs_time_anchor && is_blank_option(spec.study_context.follow_up.as_deref()) {
        issues.push(
            "study_context.follow_up is required for rate or time-to-event analyses".to_string(),
        );
    }
    if needs_time_anchor && is_blank_option(spec.study_context.censoring.as_deref()) {
        issues.push(
            "study_context.censoring is required for rate or time-to-event analyses".to_string(),
        );
    }
    if has_declared_analyses && is_blank_option(spec.study_context.missing_data_strategy.as_deref())
    {
        issues
            .push("study_context.missing_data_strategy is required for analysis runs".to_string());
    }
    if needs_clustering && is_blank_option(spec.study_context.clustering.as_deref()) {
        issues.push("study_context.clustering is required because clustered or survey structure is declared".to_string());
    }
    if has_declared_analyses && is_blank_option(spec.study_context.reporting_guideline.as_deref()) {
        issues.push(format!(
            "study_context.reporting_guideline is required (recommended: {})",
            recommended_reporting_guideline(&spec.study.design)
        ));
    }

    issues
}

pub fn recommended_reporting_guideline(design: &str) -> &'static str {
    let normalized = design.to_ascii_lowercase();
    if normalized.contains("trial") || normalized.contains("random") {
        "CONSORT"
    } else if normalized.contains("prediction")
        || normalized.contains("prognostic")
        || normalized.contains("diagnostic")
    {
        "TRIPOD"
    } else {
        "STROBE"
    }
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

#[derive(Debug, Clone)]
pub struct RunningColumnStats {
    name: String,
    missing_count: usize,
    non_missing_count: usize,
    numeric_non_missing_count: usize,
    numeric_sum: f64,
    numeric_min: Option<f64>,
    numeric_max: Option<f64>,
    zero_count: usize,
    distinct_values: BTreeSet<String>,
    sample_values: Vec<String>,
}

impl RunningColumnStats {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            missing_count: 0,
            non_missing_count: 0,
            numeric_non_missing_count: 0,
            numeric_sum: 0.0,
            numeric_min: None,
            numeric_max: None,
            zero_count: 0,
            distinct_values: BTreeSet::new(),
            sample_values: Vec::new(),
        }
    }

    pub fn observe(&mut self, raw: &str) {
        let trimmed = raw.trim();
        if is_missing_value_for_column(&self.name, trimmed) {
            self.missing_count += 1;
            return;
        }
        self.non_missing_count += 1;
        if let Ok(value) = trimmed.parse::<f64>() {
            self.numeric_non_missing_count += 1;
            self.numeric_sum += value;
            self.numeric_min = Some(self.numeric_min.map_or(value, |current| current.min(value)));
            self.numeric_max = Some(self.numeric_max.map_or(value, |current| current.max(value)));
            if value == 0.0 {
                self.zero_count += 1;
            }
        }
        if self.distinct_values.len() < 128 {
            self.distinct_values.insert(trimmed.to_string());
        }
        if self.sample_values.len() < 5 && !self.sample_values.iter().any(|value| value == trimmed)
        {
            self.sample_values.push(trimmed.to_string());
        }
    }

    pub fn finish(self) -> ColumnInspection {
        let inferred_kind = infer_variable_kind(
            &self.name,
            self.non_missing_count,
            self.numeric_non_missing_count,
            &self.distinct_values,
        );
        let total_count = self.non_missing_count + self.missing_count;
        let missing_fraction = if total_count == 0 {
            0.0
        } else {
            self.missing_count as f64 / total_count as f64
        };
        let mut warnings = Vec::new();
        if missing_fraction >= 0.2 {
            warnings.push(format!("high_missingness={:.1}%", missing_fraction * 100.0));
        }
        if matches!(inferred_kind, VariableKind::Identifier)
            && self.non_missing_count > 0
            && self.distinct_values.len() == self.non_missing_count
        {
            warnings.push("possible_direct_identifier".to_string());
        }
        if matches!(inferred_kind, VariableKind::Continuous)
            && self.non_missing_count > 0
            && self.numeric_min == self.numeric_max
        {
            warnings.push("single_observed_value".to_string());
        }

        ColumnInspection {
            name: self.name.clone(),
            inferred_kind,
            missing_count: self.missing_count,
            non_missing_count: self.non_missing_count,
            distinct_count: self.distinct_values.len(),
            sample_values: self.sample_values,
            numeric_summary: if self.numeric_non_missing_count > 0 {
                Some(NumericSummary {
                    min: self.numeric_min.unwrap_or(0.0),
                    max: self.numeric_max.unwrap_or(0.0),
                    mean: self.numeric_sum / self.numeric_non_missing_count as f64,
                    zero_count: self.zero_count,
                })
            } else {
                None
            },
            warnings,
        }
    }
}

pub fn load_analysis_spec(path: &Path) -> Result<AnalysisSpec, String> {
    let contents = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
    let contents = contents.strip_prefix('\u{feff}').unwrap_or(&contents);
    serde_yaml::from_str(contents).map_err(|error| error.to_string())
}

pub fn detect_data_format(path: &Path) -> DataFormat {
    match path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("csv") => DataFormat::Csv,
        Some("xls" | "xlsx") => DataFormat::Excel,
        Some("parquet") => DataFormat::Parquet,
        Some("xpt") => DataFormat::Xpt,
        _ => DataFormat::Unknown,
    }
}

fn is_blank_option(value: Option<&str>) -> bool {
    value.map(str::trim).is_none_or(str::is_empty)
}

pub fn format_variable_kind(kind: VariableKind) -> &'static str {
    match kind {
        VariableKind::Continuous => "continuous",
        VariableKind::Categorical => "categorical",
        VariableKind::Ordered => "ordered",
        VariableKind::Binary => "binary",
        VariableKind::Time => "time",
        VariableKind::Date => "date",
        VariableKind::PersonTime => "person_time",
        VariableKind::Event => "event",
        VariableKind::Identifier => "identifier",
    }
}

pub fn infer_variable_kind(
    name: &str,
    non_missing_count: usize,
    numeric_non_missing_count: usize,
    distinct_values: &BTreeSet<String>,
) -> VariableKind {
    let lower = name.to_ascii_lowercase();
    // Person-time: explicit marker columns
    if lower.contains("person_time") || lower.ends_with("_pt") || lower.contains("fu_pt") {
        return VariableKind::PersonTime;
    }
    // Event/outcome: use precise patterns to avoid false positives like case_id, test_case
    if lower == "event"
        || lower == "death"
        || lower == "died"
        || lower == "outcome"
        || lower.ends_with("_event")
        || lower.ends_with("_death")
        || lower.ends_with("_died")
        || lower.starts_with("ev_")
        || lower.starts_with("event_")
    {
        return VariableKind::Event;
    }
    if lower.contains("date") || lower.ends_with("_dt") || lower.starts_with("dt_") {
        return VariableKind::Date;
    }
    if lower.contains("time") || lower.starts_with("fu_") || lower.ends_with("_time") {
        return VariableKind::Time;
    }
    if lower == "id" || lower.ends_with("_id") || lower.starts_with("id_") {
        return VariableKind::Identifier;
    }
    if non_missing_count > 0 && numeric_non_missing_count == non_missing_count {
        if distinct_values.len() <= 2 {
            return VariableKind::Binary;
        }
        return VariableKind::Continuous;
    }
    if distinct_values.len() <= 2 {
        return VariableKind::Binary;
    }
    if distinct_values.len() <= 8 {
        return VariableKind::Categorical;
    }
    VariableKind::Ordered
}

pub fn is_missing_value(value: &str) -> bool {
    if value.is_empty() {
        return true;
    }
    let lower = value.to_ascii_lowercase();
    // Common text missing codes
    if matches!(
        lower.as_str(),
        "na" | "n/a"
            | "null"
            | "missing"
            | "none"
            | "unknown"
            | "."
            | "-"
            | "nd"
            | "nm"
            | "not applicable"
            | "not available"
            | "nan"
            | "inf"
            | "-inf"
    ) {
        return true;
    }
    // SAS-style sentinel values (common in epidemiology/clinical data)
    if matches!(value, "9" | "99" | "999" | "9999" | "99999" | "999999") {
        return true;
    }
    false
}

pub fn is_missing_value_for_column(column_name: &str, value: &str) -> bool {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return true;
    }
    let lower = trimmed.to_ascii_lowercase();
    if matches!(lower.as_str(), "nd" | "nm") && is_code_like_column(column_name) {
        return false;
    }
    is_missing_value(trimmed)
}

fn is_code_like_column(column_name: &str) -> bool {
    let compact = column_name
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .collect::<String>()
        .to_ascii_lowercase();

    matches!(
        compact.as_str(),
        "stateabbr" | "stateabbrev" | "statecode" | "geoid" | "locationid" | "countyfips"
    ) || compact.ends_with("abbr")
        || compact.ends_with("code")
        || compact.ends_with("fips")
        || compact.ends_with("id")
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{
        is_missing_value, is_missing_value_for_column, load_analysis_spec, RunningColumnStats,
    };

    #[test]
    fn contextual_missing_value_preserves_state_codes() {
        assert!(is_missing_value("ND"));
        assert!(is_missing_value("NM"));
        assert!(!is_missing_value_for_column("stateabbr", "ND"));
        assert!(!is_missing_value_for_column("stateabbr", "NM"));
        assert!(!is_missing_value_for_column("measureid", "ND"));
        assert!(is_missing_value_for_column("lab_result", "ND"));
    }

    #[test]
    fn running_column_stats_uses_column_context_for_missing_codes() {
        let mut state_stats = RunningColumnStats::new("stateabbr");
        state_stats.observe("ND");
        state_stats.observe("NM");
        let state_column = state_stats.finish();
        assert_eq!(state_column.missing_count, 0);
        assert_eq!(state_column.non_missing_count, 2);
        assert_eq!(state_column.distinct_count, 2);

        let mut result_stats = RunningColumnStats::new("result");
        result_stats.observe("ND");
        let result_column = result_stats.finish();
        assert_eq!(result_column.missing_count, 1);
        assert_eq!(result_column.non_missing_count, 0);
    }

    #[test]
    fn analysis_yaml_loader_accepts_utf8_bom() {
        let root = std::env::temp_dir().join(format!(
            "stats-code-bom-analysis-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time after epoch")
                .as_nanos()
        ));
        fs::create_dir_all(&root).expect("create temp dir");
        let path = root.join("analysis.yaml");
        let yaml = r"
study:
  title: BOM demo
  design: cohort
data:
  path: demo.csv
  format: csv
variables: []
analyses: []
";
        let mut bytes = vec![0xef, 0xbb, 0xbf];
        bytes.extend_from_slice(yaml.trim_start().as_bytes());
        fs::write(&path, bytes).expect("write bom yaml");

        let spec = load_analysis_spec(&path).expect("load bom yaml");
        assert_eq!(spec.study.title, "BOM demo");

        fs::remove_dir_all(root).expect("cleanup");
    }
}
