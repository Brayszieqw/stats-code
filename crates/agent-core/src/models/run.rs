//! Run status, analysis result metadata, and run-state store models.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::dataset::ColumnSummary;

/// Lifecycle state of an Analysis Run.
///
/// Serializes to lowercase tokens: `"running"`, `"completed"`, `"failed"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RunStatus {
    Running,
    Completed,
    Failed,
}

impl RunStatus {
    /// Returns the lowercase string token for this status.
    #[must_use]
    pub fn as_token(&self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }
}

/// Metadata associating a `SkillResult` with its resolved algorithm, dataset,
/// columns, parameters, and run lifecycle (Requirement 2.1, 2.4).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisResultMeta {
    /// Exact-match Output-Level Algorithm id from the coverage matrix.
    pub algorithm_id: String,
    /// The `dataset_id` of the attributed input dataset (UUID as string).
    pub dataset_id: String,
    /// 64 lowercase hex SHA256 of the input dataset bytes, or `None` for
    /// legacy datasets persisted before the SHA256 field existed.
    pub dataset_sha256: Option<String>,
    /// Column metadata of the input dataset (name + inferred type).
    pub columns: Vec<ColumnSummary>,
    /// Resolved algorithm parameter name/value bindings.
    pub params: serde_json::Value,
    /// String form of the originating Skill Run's `run_id`.
    pub run_id: String,
    /// Current lifecycle status of the analysis run.
    pub run_status: RunStatus,
}

// ---------------------------------------------------------------------------
// Run-State Store models (agent-core-local mirrors of the exporter types)
// Requirements: 5.4, 5.5, 5.6, 5.7, 5.8
// ---------------------------------------------------------------------------

/// Top-level record representing the full lifecycle of an analysis run.
///
/// This is the unit an Audit Snapshot is exported for. Keyed by `run_id` in the
/// Run-State Store (Requirement 5.1, 5.9).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisRun {
    /// Unique identifier — the string form of the originating Skill Run's `run_id`.
    pub run_id: String,
    /// Output-Level Algorithm id from the coverage matrix.
    pub algorithm_id: String,
    /// The `dataset_id` of the input dataset.
    pub dataset_id: String,
    /// 64 lowercase hex SHA256 of the input dataset bytes (Requirement 5.4).
    pub dataset_sha256: String,
    /// Current lifecycle status.
    pub status: RunStatus,
    /// Ordered workflow steps (ascending `started_at_utc`, Requirement 5.5).
    pub steps: Vec<RunWorkflowStep>,
    /// LLM calls made during the run (Requirement 5.7).
    pub llm_calls: Vec<RunLlmCall>,
    /// Host and build environment captured at run start (Requirement 5.8).
    pub environment: RunEnvironment,
    /// Timestamp when the run was created (ISO-8601 UTC).
    pub created_at: DateTime<Utc>,
    /// Timestamp of the last status or content update (ISO-8601 UTC).
    pub updated_at: DateTime<Utc>,
}

/// A single step in the analysis workflow (Requirement 5.5, 5.6).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunWorkflowStep {
    /// Step identifier (e.g. "step-1", "step-2").
    pub step_id: String,
    /// Human-readable name of the step.
    pub name: String,
    /// When the step started executing (ISO-8601 UTC, ordering key).
    pub started_at_utc: DateTime<Utc>,
    /// When the step finished executing (ISO-8601 UTC).
    pub ended_at_utc: DateTime<Utc>,
    /// Output artifacts produced by this step (Requirement 5.6).
    pub outputs: Vec<RunArtifact>,
    /// Optional narrative annotation for this step.
    pub narrative: Option<RunNarrativeStep>,
}

/// A per-step output artifact recorded under `artifacts/<step_id>/` (Requirement 5.6).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunArtifact {
    /// Unique artifact identifier.
    pub artifact_id: String,
    /// Relative path within the run archive (e.g. "artifacts/step-1/result.json").
    pub path: String,
    /// MIME content type of the artifact.
    pub content_type: String,
    /// Size of the artifact in bytes.
    pub size_bytes: u64,
}

/// An LLM call made during an analysis run (Requirement 5.7).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunLlmCall {
    /// Unique call identifier.
    pub call_id: String,
    /// The step during which this call was made.
    pub step_id: String,
    /// Model identifier (e.g. "gpt-4o").
    pub model: String,
    /// Number of prompt tokens consumed.
    pub prompt_tokens: u64,
    /// Number of completion tokens generated.
    pub completion_tokens: u64,
    /// When the call started (ISO-8601 UTC).
    pub started_at_utc: DateTime<Utc>,
    /// When the call completed (ISO-8601 UTC).
    pub ended_at_utc: DateTime<Utc>,
    /// Provider/model references for provenance.
    pub references: Vec<RunLlmRef>,
}

/// A provider/model reference for an LLM call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunLlmRef {
    /// Provider source (e.g. "openai", "anthropic").
    pub source: String,
    /// Model identifier.
    pub model: String,
    /// Model version string.
    pub version: String,
}

/// A reference-software entry recording a tool/library used during the run
/// (Requirement 5.8).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunReferenceSoftware {
    /// Software name (e.g. "R", "numpy").
    pub name: String,
    /// Version string.
    pub version: String,
    /// Role of this software (e.g. "runtime", "library").
    pub role: String,
}

/// A narrative annotation for a workflow step.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunNarrativeStep {
    /// Title of the narrative section.
    pub title: String,
    /// Markdown body of the narrative.
    pub body_markdown: String,
}

/// Host and build environment captured at run start (Requirement 5.8).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunEnvironment {
    /// OS family token: "Windows", "Linux", or "macOS".
    pub os_family: String,
    /// Best-effort OS version string; `None` if unavailable.
    pub os_version: Option<String>,
    /// Stats Code release version (e.g. from `RELEASE_VERSION`).
    pub release_version: String,
    /// Stats Code commit SHA (from build-time `git rev-parse HEAD`).
    pub commit_sha: String,
    /// Reference software versions invoked during the run.
    pub reference_software: Vec<RunReferenceSoftware>,
}
