//! `RunStore` trait definition — async interface for the Run-State Store.

use std::sync::Arc;

use async_trait::async_trait;

use crate::models::run::{AnalysisRun, RunLlmCall, RunStatus, RunWorkflowStep};

/// Errors that can occur in run-store operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunStoreError {
    /// A run with the given id already exists (Requirement 5.11).
    Conflict(String),
    /// No run exists for the given id.
    NotFound(String),
    /// An internal/unexpected error occurred.
    Internal(String),
}

impl std::fmt::Display for RunStoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Conflict(id) => write!(f, "run already exists: {id}"),
            Self::NotFound(id) => write!(f, "run not found: {id}"),
            Self::Internal(msg) => write!(f, "internal run-store error: {msg}"),
        }
    }
}

impl std::error::Error for RunStoreError {}

/// Async trait for recording and retrieving Analysis Runs.
///
/// Implementations handle the full lifecycle of an analysis run: creation,
/// status transitions, step/LLM-call recording, and retrieval.
#[async_trait]
pub trait RunStore: Send + Sync {
    /// Create a run record keyed by `run.run_id` with status `Running`
    /// (Requirement 5.1). Returns `Conflict` if a record already exists,
    /// retaining the existing record without overwriting (Requirement 5.11).
    async fn begin_run(&self, run: AnalysisRun) -> Result<(), RunStoreError>;

    /// Set the run's lifecycle status and update `updated_at`
    /// (Requirement 5.2, 5.3). Returns `NotFound` if `run_id` doesn't exist.
    async fn set_status(&self, run_id: &str, status: RunStatus) -> Result<(), RunStoreError>;

    /// Append a workflow step, keeping `steps` ordered by ascending
    /// `started_at_utc` (Requirement 5.5). Returns `NotFound` if `run_id`
    /// doesn't exist.
    async fn append_step(
        &self,
        run_id: &str,
        step: RunWorkflowStep,
    ) -> Result<(), RunStoreError>;

    /// Record an LLM call made during the run (Requirement 5.7).
    /// Returns `NotFound` if `run_id` doesn't exist.
    async fn record_llm_call(
        &self,
        run_id: &str,
        call: RunLlmCall,
    ) -> Result<(), RunStoreError>;

    /// Return the recorded run (Requirement 5.9), or `NotFound` if absent
    /// (Requirement 5.10).
    async fn get_run(&self, run_id: &str) -> Result<AnalysisRun, RunStoreError>;
}

/// Blanket implementation for `Arc<T>` so the trait can be used as
/// `Arc<dyn RunStore>`.
#[async_trait]
impl<T> RunStore for Arc<T>
where
    T: RunStore + ?Sized,
{
    async fn begin_run(&self, run: AnalysisRun) -> Result<(), RunStoreError> {
        self.as_ref().begin_run(run).await
    }

    async fn set_status(&self, run_id: &str, status: RunStatus) -> Result<(), RunStoreError> {
        self.as_ref().set_status(run_id, status).await
    }

    async fn append_step(
        &self,
        run_id: &str,
        step: RunWorkflowStep,
    ) -> Result<(), RunStoreError> {
        self.as_ref().append_step(run_id, step).await
    }

    async fn record_llm_call(
        &self,
        run_id: &str,
        call: RunLlmCall,
    ) -> Result<(), RunStoreError> {
        self.as_ref().record_llm_call(run_id, call).await
    }

    async fn get_run(&self, run_id: &str) -> Result<AnalysisRun, RunStoreError> {
        self.as_ref().get_run(run_id).await
    }
}
