use serde::{Deserialize, Serialize};

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
    #[serde(rename = "ttest.paired")]
    TtestPaired,
    #[serde(rename = "ttest.one_sample")]
    TtestOneSample,
    #[serde(rename = "anova.oneway")]
    AnovaOneway,
    #[serde(rename = "nonparam.cochran_armitage")]
    NonparamCochranArmitage,
    #[serde(rename = "nonparam.mcnemar")]
    NonparamMcnemar,
    #[serde(rename = "nonparam.wilcoxon")]
    NonparamWilcoxon,
    #[serde(rename = "nonparam.mannwhitney")]
    NonparamMannwhitney,
    Correlation,
    #[serde(rename = "epi.or_rr")]
    EpiOrRr,
    #[serde(rename = "epi.standardize")]
    EpiStandardize,
    #[serde(rename = "epi.attributable")]
    EpiAttributable,
    #[serde(rename = "diagnostic.normality")]
    DiagnosticNormality,
    #[serde(rename = "diagnostic.variance")]
    DiagnosticVariance,
    #[serde(rename = "survival.lifetable")]
    SurvivalLifetable,
    // --- Phase 2 (MEDIUM tier) ---
    #[serde(rename = "anova.posthoc")]
    AnovaPosthoc,
    #[serde(rename = "anova.repeated")]
    AnovaRepeated,
    #[serde(rename = "model.poisson")]
    ModelPoisson,
    #[serde(rename = "epi.dose_response")]
    EpiDoseResponse,
    Meta,
    #[serde(rename = "agreement.kappa")]
    AgreementKappa,
    #[serde(rename = "agreement.bland_altman")]
    AgreementBlandAltman,
    #[serde(rename = "multivariate.pca")]
    MultivariatePca,
    #[serde(rename = "sample_size.log_rank")]
    SampleSizeLogRank,
    // --- Phase 3 (LOW tier) ---
    #[serde(rename = "model.ordinal")]
    ModelOrdinal,
    #[serde(rename = "model.multinomial")]
    ModelMultinomial,
    #[serde(rename = "multivariate.lda")]
    MultivariateLda,
    #[serde(rename = "multivariate.cluster")]
    MultivariateCluster,
    Mixed,
    Psm,
    #[serde(rename = "survival.competing")]
    SurvivalCompeting,
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

// ---------------------------------------------------------------------------
// Validated study-context enums (R8 / task 10.1)
//
// These derive a typed view of two free-text wire fields ONLY at the
// validation boundary. They are intentionally NOT `Serialize`/`Deserialize`:
// the wire contract keeps `Option<String>` (see `schema::contract`), and these
// enums never reach a numeric kernel, a result payload, or the data matrix.
// `parse` is total (unknown input maps to a catch-all variant, never panics);
// `as_token` is its left inverse for recognized variants.
// ---------------------------------------------------------------------------

/// Typed view of `study_context.missing_data_strategy`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MissingDataStrategy {
    /// Complete-case analysis (listwise deletion).
    CompleteCase,
    /// Available-case analysis (pairwise deletion).
    AvailableCase,
    /// Imputation, carrying the raw method label (e.g. "mice", "mean").
    Imputation(String),
    /// Any other recognized-but-unmodeled or free-text strategy.
    Other(String),
}

impl MissingDataStrategy {
    /// Total parse from the raw wire string. Never panics; unrecognized input
    /// falls through to [`MissingDataStrategy::Other`].
    #[must_use]
    pub fn parse(raw: &str) -> Self {
        let norm = raw.trim().to_ascii_lowercase();
        match norm.as_str() {
            "complete_case" | "complete-case" | "completecase" | "complete case" => {
                Self::CompleteCase
            }
            "available_case" | "available-case" | "availablecase" | "available case" => {
                Self::AvailableCase
            }
            _ if norm.contains("imput") => Self::Imputation(raw.trim().to_string()),
            _ => Self::Other(raw.trim().to_string()),
        }
    }

    /// Canonical token for a recognized variant; round-trips through
    /// [`MissingDataStrategy::parse`]. For payload-carrying variants the
    /// stored raw label is returned verbatim.
    ///
    /// Part of the validation-boundary API; its `parse`/`as_token` round-trip
    /// is exercised by the `study_context_props` property tests (task 10.2).
    #[must_use]
    #[allow(dead_code)] // boundary API verified by property tests, not lib-internal callers
    pub fn as_token(&self) -> String {
        match self {
            Self::CompleteCase => "complete_case".to_string(),
            Self::AvailableCase => "available_case".to_string(),
            Self::Imputation(raw) | Self::Other(raw) => raw.clone(),
        }
    }
}

/// Typed view of `study_context.clustering`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClusteringUnit {
    /// No clustering declared.
    None,
    /// Individual-level (independent observations).
    Individual,
    /// A named clustering unit (e.g. `site`, `hospital`, `patient_id`).
    Named(String),
}

impl ClusteringUnit {
    /// Total parse from the raw wire string. Never panics; unrecognized
    /// non-empty input becomes a [`ClusteringUnit::Named`] unit.
    #[must_use]
    pub fn parse(raw: &str) -> Self {
        let trimmed = raw.trim();
        let norm = trimmed.to_ascii_lowercase();
        match norm.as_str() {
            "" | "none" | "no" | "false" => Self::None,
            "individual" | "individuals" | "independent" => Self::Individual,
            _ => Self::Named(trimmed.to_string()),
        }
    }

    /// Canonical token for a recognized variant; round-trips through
    /// [`ClusteringUnit::parse`].
    ///
    /// Part of the validation-boundary API; its `parse`/`as_token` round-trip
    /// is exercised by the `study_context_props` property tests (task 10.2).
    #[must_use]
    #[allow(dead_code)] // boundary API verified by property tests, not lib-internal callers
    pub fn as_token(&self) -> String {
        match self {
            Self::None => "none".to_string(),
            Self::Individual => "individual".to_string(),
            Self::Named(name) => name.clone(),
        }
    }
}
