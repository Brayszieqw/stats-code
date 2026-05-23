//! Skill domain model.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Type alias for skill run identifiers.
pub type SkillRunId = Uuid;

/// Record of a single skill execution within a session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillRun {
    pub run_id: SkillRunId,
    pub skill_id: String,
    pub args: serde_json::Value,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub outcome: SkillOutcome,
}

/// Outcome of a skill execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SkillOutcome {
    /// Skill is still running.
    Pending,
    /// Skill completed successfully.
    Ok(SkillResult),
    /// Skill failed with an error.
    Failed(SkillError),
}

/// Structured result of a successful skill execution (R7.5).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillResult {
    pub schema_version: String,
    /// Structured result payload (schema matches stats-code `--json` output).
    pub payload: serde_json::Value,
    /// Risk signals detected in the result (R7.3).
    pub risk_signals: Vec<RiskSignal>,
}

/// Known risk signals that can be detected in skill results (R7.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RiskSignal {
    /// p-value > 0.05
    PValueAboveAlpha,
    /// VIF > 10 (multicollinearity)
    VifTooHigh,
    /// Statistical power < 0.8
    LowPower,
    /// Cox proportional hazards assumption violated
    CoxPhAssumptionViolated,
}

/// Error from a failed skill execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillError {
    pub message: String,
    pub stderr_excerpt: Option<String>,
}
