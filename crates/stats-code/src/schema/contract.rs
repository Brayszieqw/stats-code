use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::types::{AnalysisKind, DataFormat, ModelKind, VariableKind, VariableRole};

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
    pub var: Option<String>,
    #[serde(default)]
    pub before: Option<String>,
    #[serde(default)]
    pub after: Option<String>,
    #[serde(default)]
    pub mu: Option<f64>,
    #[serde(default)]
    pub group: Option<String>,
    #[serde(default)]
    pub block: Option<String>,
    #[serde(default)]
    pub var1: Option<String>,
    #[serde(default)]
    pub var2: Option<String>,
    #[serde(default)]
    pub x: Option<String>,
    #[serde(default)]
    pub y: Option<String>,
    #[serde(default)]
    pub method: Option<String>,
    #[serde(default)]
    pub center: Option<String>,
    #[serde(default)]
    pub event: Option<String>,
    #[serde(default)]
    pub events: Option<String>,
    #[serde(default)]
    pub person_time: Option<String>,
    #[serde(default)]
    pub exposure: Option<String>,
    #[serde(default)]
    pub outcome: Option<String>,
    #[serde(default)]
    pub exposure_event: Option<String>,
    #[serde(default)]
    pub outcome_event: Option<String>,
    #[serde(default)]
    pub time: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub intervals: Option<String>,
    #[serde(default)]
    pub entering: Option<String>,
    #[serde(default)]
    pub withdrawals: Option<String>,
    #[serde(default)]
    pub input_format: Option<String>,
    #[serde(default)]
    pub age_group: Option<String>,
    #[serde(default)]
    pub standard_pop: Option<String>,
    #[serde(default)]
    pub exposure_prevalence: Option<f64>,
    #[serde(default)]
    pub predictors: Vec<String>,
    #[serde(default)]
    pub adjust: Vec<String>,
    #[serde(default)]
    pub strata: Vec<String>,
    #[serde(default)]
    pub scores: Vec<f64>,
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

/// Top-level specification for a statistical analysis workflow.
///
/// An `AnalysisSpec` is typically loaded from a YAML file that describes the
/// study design, data source, variables, and analysis steps.
///
/// # Examples
///
/// ```no_run
/// use stats_code::AnalysisSpec;
///
/// let yaml = r#"
/// study:
///   title: "Blood Pressure Trial"
///   design: "randomized_controlled_trial"
/// data:
///   path: "data/bp_trial.csv"
///   format: csv
/// variables:
///   - name: treatment
///     role: exposure
///     kind: categorical
///   - name: sbp_change
///     role: outcome
///     kind: continuous
/// analyses:
///   - kind: model
///     model: linear
/// "#;
///
/// let spec: AnalysisSpec = serde_yaml::from_str(yaml)
///     .expect("failed to parse analysis spec");
/// assert_eq!(spec.study.title, "Blood Pressure Trial");
/// ```
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
