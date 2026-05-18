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
