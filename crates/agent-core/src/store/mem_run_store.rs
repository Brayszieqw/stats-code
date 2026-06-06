//! In-memory implementation of `RunStore` for testing and development.

use std::collections::HashMap;

use async_trait::async_trait;
use chrono::Utc;
use tokio::sync::RwLock;

use crate::models::run::{AnalysisRun, RunLlmCall, RunStatus, RunWorkflowStep};
use crate::traits::run_store::{RunStore, RunStoreError};

/// In-memory run store backed by a `tokio::sync::RwLock<HashMap>`.
///
/// Suitable for tests and single-process development; not durable across restarts.
pub struct MemRunStore {
    runs: RwLock<HashMap<String, AnalysisRun>>,
}

impl MemRunStore {
    /// Create a new empty store.
    #[must_use]
    pub fn new() -> Self {
        Self {
            runs: RwLock::new(HashMap::new()),
        }
    }
}

impl Default for MemRunStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl RunStore for MemRunStore {
    /// Create a run record keyed by `run.run_id` with status `Running`.
    /// Returns `Conflict` if a record already exists, retaining the existing
    /// record without overwriting (Requirement 5.11).
    async fn begin_run(&self, run: AnalysisRun) -> Result<(), RunStoreError> {
        let mut map = self.runs.write().await;
        if map.contains_key(&run.run_id) {
            return Err(RunStoreError::Conflict(run.run_id));
        }
        map.insert(run.run_id.clone(), run);
        Ok(())
    }

    /// Set the run's lifecycle status and update `updated_at` to now.
    /// Returns `NotFound` if `run_id` doesn't exist.
    async fn set_status(&self, run_id: &str, status: RunStatus) -> Result<(), RunStoreError> {
        let mut map = self.runs.write().await;
        let run = map
            .get_mut(run_id)
            .ok_or_else(|| RunStoreError::NotFound(run_id.to_string()))?;
        run.status = status;
        run.updated_at = Utc::now();
        Ok(())
    }

    /// Append a workflow step, keeping `steps` ordered by ascending
    /// `started_at_utc` (Requirement 5.5). Records outputs under
    /// `artifacts/<step_id>/`. Returns `NotFound` if `run_id` doesn't exist.
    async fn append_step(
        &self,
        run_id: &str,
        mut step: RunWorkflowStep,
    ) -> Result<(), RunStoreError> {
        let mut map = self.runs.write().await;
        let run = map
            .get_mut(run_id)
            .ok_or_else(|| RunStoreError::NotFound(run_id.to_string()))?;

        // Ensure output artifact paths are under `artifacts/<step_id>/`
        for artifact in &mut step.outputs {
            let prefix = format!("artifacts/{}/", step.step_id);
            if !artifact.path.starts_with(&prefix) {
                artifact.path = format!("{}{}", prefix, artifact.path);
            }
        }

        run.steps.push(step);
        // Maintain ascending `started_at_utc` ordering (Requirement 5.5)
        run.steps.sort_by_key(|s| s.started_at_utc);
        run.updated_at = Utc::now();
        Ok(())
    }

    /// Record an LLM call made during the run (Requirement 5.7).
    /// Returns `NotFound` if `run_id` doesn't exist.
    async fn record_llm_call(
        &self,
        run_id: &str,
        call: RunLlmCall,
    ) -> Result<(), RunStoreError> {
        let mut map = self.runs.write().await;
        let run = map
            .get_mut(run_id)
            .ok_or_else(|| RunStoreError::NotFound(run_id.to_string()))?;
        run.llm_calls.push(call);
        run.updated_at = Utc::now();
        Ok(())
    }

    /// Return the recorded run (Requirement 5.9), or `NotFound` if absent
    /// (Requirement 5.10).
    async fn get_run(&self, run_id: &str) -> Result<AnalysisRun, RunStoreError> {
        let map = self.runs.read().await;
        map.get(run_id)
            .cloned()
            .ok_or_else(|| RunStoreError::NotFound(run_id.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::run::{RunArtifact, RunEnvironment};
    use chrono::Duration;

    /// Helper to create a minimal `AnalysisRun` for testing.
    fn make_run(run_id: &str) -> AnalysisRun {
        let now = Utc::now();
        AnalysisRun {
            run_id: run_id.to_string(),
            algorithm_id: "linear".to_string(),
            dataset_id: "ds-001".to_string(),
            dataset_sha256: "a".repeat(64),
            status: RunStatus::Running,
            steps: Vec::new(),
            llm_calls: Vec::new(),
            environment: RunEnvironment {
                os_family: "Windows".to_string(),
                os_version: Some("10.0".to_string()),
                release_version: "1.0.0".to_string(),
                commit_sha: "b".repeat(40),
                reference_software: Vec::new(),
            },
            created_at: now,
            updated_at: now,
        }
    }

    /// Helper to create a workflow step at a given time offset.
    fn make_step(step_id: &str, offset_secs: i64) -> RunWorkflowStep {
        let base = Utc::now();
        RunWorkflowStep {
            step_id: step_id.to_string(),
            name: format!("Step {step_id}"),
            started_at_utc: base + Duration::seconds(offset_secs),
            ended_at_utc: base + Duration::seconds(offset_secs + 1),
            outputs: vec![RunArtifact {
                artifact_id: format!("art-{step_id}"),
                path: "result.json".to_string(),
                content_type: "application/json".to_string(),
                size_bytes: 42,
            }],
            narrative: None,
        }
    }

    #[tokio::test]
    async fn begin_run_and_get_run() {
        let store = MemRunStore::new();
        let run = make_run("run-1");
        store.begin_run(run.clone()).await.unwrap();

        let fetched = store.get_run("run-1").await.unwrap();
        assert_eq!(fetched.run_id, "run-1");
        assert_eq!(fetched.status, RunStatus::Running);
        assert_eq!(fetched.dataset_sha256, "a".repeat(64));
    }

    #[tokio::test]
    async fn begin_run_conflict_does_not_overwrite() {
        let store = MemRunStore::new();
        let run1 = make_run("run-1");
        store.begin_run(run1).await.unwrap();

        // Attempt to insert a second run with the same id
        let mut run2 = make_run("run-1");
        run2.algorithm_id = "logistic".to_string();
        let result = store.begin_run(run2).await;
        assert!(matches!(result, Err(RunStoreError::Conflict(ref id)) if id == "run-1"));

        // Original run is preserved
        let fetched = store.get_run("run-1").await.unwrap();
        assert_eq!(fetched.algorithm_id, "linear");
    }

    #[tokio::test]
    async fn get_run_not_found() {
        let store = MemRunStore::new();
        let result = store.get_run("nonexistent").await;
        assert!(matches!(result, Err(RunStoreError::NotFound(ref id)) if id == "nonexistent"));
    }

    #[tokio::test]
    async fn set_status_updates_status_and_timestamp() {
        let store = MemRunStore::new();
        let run = make_run("run-1");
        let original_updated = run.updated_at;
        store.begin_run(run).await.unwrap();

        store
            .set_status("run-1", RunStatus::Completed)
            .await
            .unwrap();

        let fetched = store.get_run("run-1").await.unwrap();
        assert_eq!(fetched.status, RunStatus::Completed);
        assert!(fetched.updated_at >= original_updated);
    }

    #[tokio::test]
    async fn set_status_not_found() {
        let store = MemRunStore::new();
        let result = store.set_status("nope", RunStatus::Failed).await;
        assert!(matches!(result, Err(RunStoreError::NotFound(_))));
    }

    #[tokio::test]
    async fn append_step_maintains_order() {
        let store = MemRunStore::new();
        store.begin_run(make_run("run-1")).await.unwrap();

        // Insert steps out of order
        let step_late = make_step("step-2", 10);
        let step_early = make_step("step-1", 1);

        store.append_step("run-1", step_late).await.unwrap();
        store.append_step("run-1", step_early).await.unwrap();

        let fetched = store.get_run("run-1").await.unwrap();
        assert_eq!(fetched.steps.len(), 2);
        // Steps should be ordered by started_at_utc ascending
        assert!(fetched.steps[0].started_at_utc <= fetched.steps[1].started_at_utc);
        assert_eq!(fetched.steps[0].step_id, "step-1");
        assert_eq!(fetched.steps[1].step_id, "step-2");
    }

    #[tokio::test]
    async fn append_step_records_outputs_under_artifacts_prefix() {
        let store = MemRunStore::new();
        store.begin_run(make_run("run-1")).await.unwrap();

        let step = make_step("step-1", 0);
        store.append_step("run-1", step).await.unwrap();

        let fetched = store.get_run("run-1").await.unwrap();
        let output = &fetched.steps[0].outputs[0];
        assert!(
            output.path.starts_with("artifacts/step-1/"),
            "artifact path should be under artifacts/<step_id>/, got: {}",
            output.path
        );
    }

    #[tokio::test]
    async fn append_step_not_found() {
        let store = MemRunStore::new();
        let result = store.append_step("nope", make_step("s", 0)).await;
        assert!(matches!(result, Err(RunStoreError::NotFound(_))));
    }

    #[tokio::test]
    async fn record_llm_call_and_retrieve() {
        let store = MemRunStore::new();
        store.begin_run(make_run("run-1")).await.unwrap();

        let call = RunLlmCall {
            call_id: "call-1".to_string(),
            step_id: "step-1".to_string(),
            model: "gpt-4o".to_string(),
            prompt_tokens: 100,
            completion_tokens: 50,
            started_at_utc: Utc::now(),
            ended_at_utc: Utc::now(),
            references: Vec::new(),
        };
        store.record_llm_call("run-1", call).await.unwrap();

        let fetched = store.get_run("run-1").await.unwrap();
        assert_eq!(fetched.llm_calls.len(), 1);
        assert_eq!(fetched.llm_calls[0].call_id, "call-1");
    }

    #[tokio::test]
    async fn record_llm_call_not_found() {
        let store = MemRunStore::new();
        let call = RunLlmCall {
            call_id: "call-1".to_string(),
            step_id: "step-1".to_string(),
            model: "gpt-4o".to_string(),
            prompt_tokens: 100,
            completion_tokens: 50,
            started_at_utc: Utc::now(),
            ended_at_utc: Utc::now(),
            references: Vec::new(),
        };
        let result = store.record_llm_call("nope", call).await;
        assert!(matches!(result, Err(RunStoreError::NotFound(_))));
    }
}
