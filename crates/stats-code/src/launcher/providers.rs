//! Concrete `agent-server` provider implementations wired by the launcher.
//!
//! The `agent-server` crate declares three provider traits
//! ([`CoverageMatrixProvider`], [`SidecarProvider`], [`SnapshotProvider`])
//! but cannot implement them itself: the dependency arrow runs
//! `api → agent-core → agent-server → stats-code`, so only `stats-code`
//! (this crate) can bridge the embedded [`CoverageMatrix`] and the
//! [`sidecar`]/[`snapshot`] subsystems into the HTTP layer.
//!
//! The launcher constructs these providers once and injects them into the
//! shared `AppState` (see `launcher::mod`). Before this wiring existed the
//! three endpoints (`GET /api/coverage-matrix`, `GET /api/sidecar/{id}`,
//! `POST /api/snapshot/export`) returned `503 Service Unavailable` because
//! the corresponding `Option<Arc<dyn …>>` fields defaulted to `None`.
//!
//! ## Coverage matrix
//!
//! [`EmbeddedCoverageMatrixProvider`] is fully functional: it converts the
//! process-global, compile-time-embedded [`CoverageMatrix`] into the
//! wire DTO. No per-run state is required, so the endpoint is complete.
//!
//! ## Sidecar
//!
//! [`LiveSidecarProvider`] is fully functional and stateless. The
//! Equivalent Code Sidecar is a pure function of
//! `(algorithm_id, software, columns, dataset_sha256, params)`, all of
//! which the SPA already holds and posts directly in the request body, so
//! the provider renders real snippets via
//! [`sidecar::generate_snippet`](crate::sidecar::generate_snippet) with no
//! run-state store.
//!
//! ## Snapshot
//!
//! [`UnavailableSnapshotProvider`] returns a structured error. The
//! deterministic exporter is implemented and unit-tested, but it needs a
//! materialized `RunSnapshot` (workflow steps, per-step artifacts, dataset
//! bytes) that no run-state store currently persists. Wiring it is a
//! separate run-store feature; until then the endpoint reports the gap
//! honestly instead of fabricating an empty run.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use agent_core::models::run::{AnalysisRun, RunStatus as CoreRunStatus};
use agent_core::traits::dataset_store::DatasetStore;
use agent_core::traits::run_store::{RunStore, RunStoreError};
use agent_server::state::{
    CoverageMatrixProvider, SidecarProvider, SidecarProviderError, SnapshotProvider,
    SnapshotProviderError,
};
use api::sidecar::{
    AlgorithmEntryDto, CoverageMatrixDto, CoverageValueDto, ReferenceImplDto,
    ReferenceSoftware as DtoSoftware, SidecarRenderRequest, SidecarSnippetDto,
    SnapshotExportResponse,
};

use crate::coverage_matrix::{CoverageMatrix, CoverageState, ReferenceImpl, ReferenceSoftware};
use crate::sidecar::{
    generate_snippet, Column, ColumnDtype, GenerateError, RenderParams, SidecarSnippet,
};
use crate::snapshot::{
    export_snapshot, LlmCall as SnapshotLlmCall, NarrativeStep as SnapshotNarrativeStep,
    ReferenceSoftwareVersion, RunSnapshot, RunStatus as SnapshotRunStatus, SnapshotArtifact,
    SnapshotError, SnapshotResult as ExportResult, Workflow,
};
use crate::snapshot::workflow_yaml::{ArtifactRef, InputDataset, WorkflowStep};

// ---------------------------------------------------------------------------
// DTO conversion (CoverageMatrix → CoverageMatrixDto)
// ---------------------------------------------------------------------------

/// Map the in-crate [`ReferenceSoftware`] onto the wire [`DtoSoftware`].
fn software_to_dto(sw: ReferenceSoftware) -> DtoSoftware {
    match sw {
        ReferenceSoftware::R => DtoSoftware::R,
        ReferenceSoftware::SAS => DtoSoftware::SAS,
        ReferenceSoftware::Python => DtoSoftware::Python,
        ReferenceSoftware::SPSS => DtoSoftware::SPSS,
    }
}

/// Map the in-crate [`CoverageState`] onto the wire [`CoverageValueDto`].
fn coverage_to_dto(state: CoverageState) -> CoverageValueDto {
    match state {
        CoverageState::Live => CoverageValueDto::Live,
        CoverageState::Recorded => CoverageValueDto::Recorded,
        CoverageState::SidecarOnly => CoverageValueDto::SidecarOnly,
        CoverageState::None_ => CoverageValueDto::None_,
    }
}

/// Map a [`ReferenceImpl`] onto the wire [`ReferenceImplDto`].
///
/// The DTO carries a single required `callable` string; the in-crate model
/// splits R/Python function names (`callable`) from SAS/SPSS procedure
/// names (`proc`). We coalesce them so the SPA always has a non-empty
/// identifier to show, preferring `callable` and falling back to `proc`.
fn reference_to_dto(reference: &ReferenceImpl) -> ReferenceImplDto {
    let callable = reference
        .callable
        .clone()
        .or_else(|| reference.proc.clone())
        .unwrap_or_default();
    ReferenceImplDto {
        callable,
        package: reference.package.clone(),
        version: reference.version.clone(),
    }
}

/// Convert the entire embedded [`CoverageMatrix`] into the wire DTO.
///
/// Iteration order follows the matrix's declared algorithm order and the
/// canonical `BTreeMap` software order, so the emitted JSON is
/// byte-deterministic across hosts (consistent with Requirement 2.1's
/// determinism intent).
#[must_use]
pub fn coverage_matrix_to_dto(matrix: &CoverageMatrix) -> CoverageMatrixDto {
    let algorithms = matrix
        .algorithms
        .iter()
        .map(|entry| AlgorithmEntryDto {
            id: entry.id.clone(),
            display_name: entry.display_name.clone(),
            iterative: entry.iterative,
            coverage: entry
                .coverage
                .iter()
                .map(|(sw, state)| (software_to_dto(*sw), coverage_to_dto(*state)))
                .collect(),
            reference: entry
                .reference
                .iter()
                .map(|(sw, r)| (software_to_dto(*sw), reference_to_dto(r)))
                .collect(),
        })
        .collect();

    CoverageMatrixDto {
        schema_version: matrix.schema_version,
        release_version: matrix.release_version.clone(),
        algorithms,
    }
}

// ---------------------------------------------------------------------------
// CoverageMatrixProvider — fully functional
// ---------------------------------------------------------------------------

/// Serves the process-global, compile-time-embedded Algorithm Coverage
/// Matrix as the wire DTO.
#[derive(Debug, Default, Clone, Copy)]
pub struct EmbeddedCoverageMatrixProvider;

impl CoverageMatrixProvider for EmbeddedCoverageMatrixProvider {
    fn get(&self) -> CoverageMatrixDto {
        coverage_matrix_to_dto(CoverageMatrix::get_loaded())
    }
}

// ---------------------------------------------------------------------------
// SidecarProvider — fully functional, stateless
// ---------------------------------------------------------------------------

/// Map a column dtype token from the wire request onto the in-crate
/// [`ColumnDtype`]. Returns `None` for any token outside the closed set
/// `{numeric, categorical, date, string}` so a malformed request is
/// rejected rather than silently rendering the wrong dtype.
fn parse_dtype(token: &str) -> Option<ColumnDtype> {
    match token {
        "numeric" => Some(ColumnDtype::Numeric),
        "categorical" => Some(ColumnDtype::Categorical),
        "date" => Some(ColumnDtype::Date),
        "string" => Some(ColumnDtype::String),
        _ => None,
    }
}

/// Map the wire [`DtoSoftware`] onto the in-crate [`ReferenceSoftware`].
fn software_from_dto(sw: DtoSoftware) -> ReferenceSoftware {
    match sw {
        DtoSoftware::R => ReferenceSoftware::R,
        DtoSoftware::SAS => ReferenceSoftware::SAS,
        DtoSoftware::Python => ReferenceSoftware::Python,
        DtoSoftware::SPSS => ReferenceSoftware::SPSS,
    }
}

/// Map an in-crate [`CoverageState`] onto the wire [`CoverageValueDto`]
/// for the snippet response.
fn coverage_value_dto(state: CoverageState) -> CoverageValueDto {
    coverage_to_dto(state)
}

/// Concrete Equivalent Code Sidecar provider.
///
/// Stateless and fully functional: it renders the snippet from the data
/// carried in the [`SidecarRenderRequest`] (algorithm id, software,
/// columns, dataset SHA256, params) by calling the pure
/// [`sidecar::generate_snippet`](crate::sidecar::generate_snippet). No
/// run-state store is consulted, so the endpoint produces real snippets
/// today.
#[derive(Debug, Default, Clone, Copy)]
pub struct LiveSidecarProvider;

impl SidecarProvider for LiveSidecarProvider {
    fn generate(
        &self,
        algorithm_id: &str,
        request: &SidecarRenderRequest,
    ) -> Result<SidecarSnippetDto, SidecarProviderError> {
        // Validate the dataset SHA256 shape up front: `format_header`
        // debug-asserts a 64-char lowercase hex string, and a malformed
        // value is a caller error (400) rather than an internal fault.
        let sha = &request.dataset_sha256;
        if sha.len() != 64 || !sha.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(SidecarProviderError::InvalidRequest(format!(
                "dataset_sha256 must be 64 lowercase hex chars, got {} chars",
                sha.len()
            )));
        }

        // Parse columns, rejecting any unknown dtype token.
        let mut columns = Vec::with_capacity(request.columns.len());
        for col in &request.columns {
            let dtype = parse_dtype(&col.dtype).ok_or_else(|| {
                SidecarProviderError::InvalidRequest(format!(
                    "unknown column dtype {:?} for column {:?}; \
                     expected one of numeric|categorical|date|string",
                    col.dtype, col.name
                ))
            })?;
            columns.push(Column {
                name: col.name.clone(),
                dtype,
            });
        }

        let mut params = RenderParams::new();
        for (k, v) in &request.params {
            params.insert(k.clone(), v.clone());
        }

        let software = software_from_dto(request.software);

        // The Sidecar Code Generator is the authoritative source of the
        // coverage value for the response, so resolve it once here for the
        // snippet DTO regardless of which variant `generate_snippet`
        // returns.
        let matrix = CoverageMatrix::get_loaded();
        let coverage = matrix.coverage(algorithm_id, software);

        match generate_snippet(algorithm_id, &params, &columns, sha, software, &[], None) {
            Ok(SidecarSnippet::Snippet {
                text,
                sha256_of_dataset,
                release_version,
                ..
            }) => Ok(SidecarSnippetDto {
                algorithm_id: algorithm_id.to_string(),
                software: request.software,
                coverage_value: coverage
                    .map_or(CoverageValueDto::None_, coverage_value_dto),
                text: Some(text),
                sha256_of_dataset,
                release_version,
            }),
            Ok(SidecarSnippet::Uncovered { .. }) => Ok(SidecarSnippetDto {
                algorithm_id: algorithm_id.to_string(),
                software: request.software,
                coverage_value: CoverageValueDto::None_,
                text: None,
                sha256_of_dataset: sha.clone(),
                release_version: matrix.release_version().to_string(),
            }),
            Err(GenerateError::UnknownAlgorithm { algorithm_id }) => {
                Err(SidecarProviderError::UnknownAlgorithm(algorithm_id))
            }
            Err(GenerateError::MissingTemplate { algorithm_id, .. }) => {
                Err(SidecarProviderError::MissingTemplate {
                    algorithm_id,
                    software: request.software,
                })
            }
            Err(GenerateError::Render(e)) => Err(SidecarProviderError::InvalidRequest(
                format!("template render failed: {e}"),
            )),
            Err(GenerateError::ForbiddenSpawn(e)) => {
                Err(SidecarProviderError::ForbiddenSpawn(e.to_string()))
            }
        }
    }
}

// ---------------------------------------------------------------------------
// SnapshotProvider — gated on a run-state store that does not yet exist
// ---------------------------------------------------------------------------

const SNAPSHOT_UNAVAILABLE_MSG: &str =
    "audit snapshot export requires a per-run state store (workflow steps, \
     per-step artifacts, dataset bytes) that this build does not yet \
     persist; the exporter is implemented and unit-tested but no run store \
     currently feeds it";

/// Audit Snapshot provider.
///
/// The deterministic exporter ([`snapshot::export_snapshot`]) is fully
/// implemented and unit-tested, but it requires a materialized
/// `RunSnapshot` (workflow, per-step artifacts, dataset bytes) that no
/// run-state store currently persists. Until that store lands, this
/// provider returns a structured [`SnapshotProviderError::Internal`]
/// rather than fabricating an empty run, which keeps the endpoint honest.
#[derive(Debug, Default, Clone, Copy)]
pub struct UnavailableSnapshotProvider;

impl SnapshotProvider for UnavailableSnapshotProvider {
    fn export(
        &self,
        _run_id: &str,
        _destination: &str,
    ) -> Result<SnapshotExportResponse, SnapshotProviderError> {
        Err(SnapshotProviderError::Internal(
            SNAPSHOT_UNAVAILABLE_MSG.to_string(),
        ))
    }
}

// ---------------------------------------------------------------------------
// RunBackedSnapshotProvider — builds a RunSnapshot from the Run-State Store
// and delegates to export_snapshot (Requirements: 6.2, 6.6, 6.7, 8.1–8.3, 8.7)
// ---------------------------------------------------------------------------

/// Concrete Audit Snapshot provider backed by the Run-State Store.
///
/// Replaces [`UnavailableSnapshotProvider`] once the run-state store is
/// wired. Implements the refusal ladder (Requirements 8.1–8.3, 8.7) and
/// delegates to the deterministic [`export_snapshot`] exporter.
pub struct RunBackedSnapshotProvider {
    /// The run-state store to look up analysis runs.
    runs: Arc<dyn RunStore>,
    /// The dataset store, used to read raw dataset bytes by `dataset_id`.
    /// The store owns its on-disk layout; this provider no longer
    /// reconstructs file paths itself.
    datasets: Arc<dyn DatasetStore>,
    /// Secret values to redact (the active LLM API key from config.toml).
    api_keys: Vec<String>,
    /// Analysis working directory (so the exporter rewrites in-dir paths
    /// relative and out-of-dir paths to `<external>`).
    working_directory: Option<PathBuf>,
}

impl RunBackedSnapshotProvider {
    /// Create a new provider with the given run store, dataset store, and
    /// configuration.
    pub fn new(
        runs: Arc<dyn RunStore>,
        datasets: Arc<dyn DatasetStore>,
        api_keys: Vec<String>,
        working_directory: Option<PathBuf>,
    ) -> Self {
        Self {
            runs,
            datasets,
            api_keys,
            working_directory,
        }
    }
}

/// Decode the `dataset_id` string recorded on an `AnalysisRun` into a
/// `DatasetId` for the `DatasetStore::read_raw_by_id` lookup. A malformed id is
/// a `DatasetUnresolved` refusal rather than an internal error.
fn parse_dataset_id(
    dataset_id: &str,
) -> Result<agent_core::models::DatasetId, SnapshotProviderError> {
    agent_core::models::DatasetId::parse_str(dataset_id).map_err(|e| {
        SnapshotProviderError::DatasetUnresolved {
            reason: format!("recorded dataset_id {dataset_id:?} is not a valid UUID: {e}"),
        }
    })
}

impl SnapshotProvider for RunBackedSnapshotProvider {
    fn export(
        &self,
        run_id: &str,
        destination: &str,
    ) -> Result<SnapshotExportResponse, SnapshotProviderError> {
        // 1. Look up the run (Requirement 6.6 → UnknownRun if absent).
        //
        // `export` is a *synchronous* trait method, but `RunStore::get_run` is
        // async. The HTTP server drives this handler on a multi-thread tokio
        // worker (`#[tokio::main]` + `axum::serve`), so the worker thread is
        // already inside a runtime. A bare `Handle::current().block_on(..)`
        // panics in that situation ("Cannot start a runtime from within a
        // runtime"). `task::block_in_place` tells the multi-thread scheduler to
        // hand this worker's other tasks to a sibling thread, making it legal
        // to block here while we await the store. (Verified: bare block_on
        // panics on both current_thread and multi_thread flavors; the
        // block_in_place form does not panic on the multi_thread runtime the
        // server actually uses.)
        let run = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(self.runs.get_run(run_id))
        })
        .map_err(|e| match e {
            RunStoreError::NotFound(_) => SnapshotProviderError::UnknownRun(run_id.to_string()),
            other => SnapshotProviderError::Internal(other.to_string()),
        })?;

        // 2. Refusal ladder — fixed order 8.1 → 8.2 → 8.3 (Requirement 8.7).

        // Gate 8.1: Run must be completed.
        if run.status != CoreRunStatus::Completed {
            return Err(SnapshotProviderError::RunNotCompleted {
                actual_status: run.status.as_token().to_string(),
            });
        }

        // Gate 8.2: Run must have at least one workflow step.
        if run.steps.is_empty() {
            return Err(SnapshotProviderError::NoExportableStep {
                run_id: run_id.to_string(),
            });
        }

        // Gate 8.3: Dataset bytes must be resolvable. We ask the dataset store
        // for the bytes by id — the store owns its on-disk layout, so this
        // provider no longer reconstructs file paths itself. Same sync/async
        // bridge as the run lookup above.
        let dataset_uuid = parse_dataset_id(&run.dataset_id)?;
        let dataset_bytes = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(self.datasets.read_raw_by_id(dataset_uuid))
        })
        .map_err(|e| SnapshotProviderError::DatasetUnresolved {
            reason: e.to_string(),
        })?;

        // Hazard-B guard: a present-but-malformed recorded SHA256 must be
        // refused, not silently decoded into a fabricated all-zero fingerprint
        // (which would forge the audit chain). An empty string is the legacy
        // "no hash recorded" case and is allowed to flow through to the
        // exporter, which records it faithfully as absent.
        if !run.dataset_sha256.is_empty() && !is_valid_dataset_sha256(&run.dataset_sha256) {
            return Err(SnapshotProviderError::Internal(format!(
                "run {run_id} has a malformed dataset SHA256 \
                 (expected 64 lowercase hex chars, got {} chars); \
                 refusing to export a fabricated fingerprint",
                run.dataset_sha256.len()
            )));
        }

        // 3. Build the RunSnapshot from the recorded run (Requirement 6.3).
        let snapshot = build_run_snapshot(
            &run,
            dataset_bytes,
            &self.api_keys,
            self.working_directory.clone(),
        );

        // 4. Delegate to the deterministic exporter (Requirement 6.2).
        let dest_path = Path::new(destination);
        match export_snapshot(&snapshot, dest_path) {
            Ok(ExportResult {
                snapshot_path,
                sha256,
            }) => Ok(SnapshotExportResponse {
                snapshot_path: snapshot_path.display().to_string(),
                sha256: sha256.iter().fold(String::with_capacity(64), |mut acc, b| {
                    use std::fmt::Write as _;
                    let _ = write!(acc, "{b:02x}");
                    acc
                }),
            }),
            Err(SnapshotError::PayloadTooLarge {
                measured_bytes,
                ceiling,
            }) => Err(SnapshotProviderError::PayloadTooLarge {
                measured_bytes,
                ceiling_bytes: ceiling,
            }),
            Err(SnapshotError::ForbiddenSpawn(e)) => {
                Err(SnapshotProviderError::ForbiddenSpawn(e.to_string()))
            }
            Err(other) => Err(SnapshotProviderError::Internal(other.to_string())),
        }
    }
}

// ---------------------------------------------------------------------------
// build_run_snapshot — pure projection (AnalysisRun → snapshot::RunSnapshot)
// Requirements: 6.3, 6.4, 6.5, 6.8, 6.9
// ---------------------------------------------------------------------------

/// Map the agent-core `RunStatus` onto the exporter's `snapshot::RunStatus`.
fn core_status_to_snapshot(status: CoreRunStatus) -> SnapshotRunStatus {
    match status {
        CoreRunStatus::Running => SnapshotRunStatus::Running,
        CoreRunStatus::Completed => SnapshotRunStatus::Completed,
        CoreRunStatus::Failed => SnapshotRunStatus::Failed,
    }
}

/// Returns `true` iff `s` is exactly 64 lowercase-hex characters — the shape a
/// dataset SHA256 must have to produce a faithful 32-byte fingerprint.
///
/// Used as an export-time guard (hazard B): a *present-but-malformed* recorded
/// SHA256 must be refused rather than silently decoded to a fabricated all-zero
/// fingerprint, which would corrupt the audit chain. An empty string is the
/// legacy "no hash recorded" case and is handled separately by the caller.
fn is_valid_dataset_sha256(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Decode a 64-character lowercase hex string into a `[u8; 32]` array.
///
/// Returns `[0u8; 32]` if the input is malformed. Callers that need to
/// distinguish "legacy/absent hash" from "corrupt hash" MUST pre-validate with
/// [`is_valid_dataset_sha256`] — this decoder is intentionally total so the
/// pure projection never panics, but it does not by itself signal corruption.
fn hex_to_bytes32(hex: &str) -> [u8; 32] {
    let mut out = [0u8; 32];
    if hex.len() != 64 {
        return out;
    }
    for (i, chunk) in hex.as_bytes().chunks(2).enumerate() {
        if i >= 32 {
            break;
        }
        let hi = hex_nibble(chunk[0]);
        let lo = hex_nibble(chunk[1]);
        out[i] = (hi << 4) | lo;
    }
    out
}

fn hex_nibble(b: u8) -> u8 {
    match b {
        b'0'..=b'9' => b - b'0',
        b'a'..=b'f' => b - b'a' + 10,
        _ => 0,
    }
}

/// Project an [`AnalysisRun`] (from agent-core) into a [`RunSnapshot`]
/// suitable for the audit snapshot exporter.
///
/// ## Field mapping
///
/// | `RunSnapshot` field     | Source                                      |
/// |-------------------------|---------------------------------------------|
/// | `run_id`                | `run.run_id`                                |
/// | `status`                | `run.status` (mapped to snapshot enum)      |
/// | `dataset_sha256`        | `run.dataset_sha256` (hex → `[u8;32]`)      |
/// | `dataset_csv_bytes`     | `dataset_csv_bytes` parameter               |
/// | `workflow`              | built from `run.steps`                      |
/// | `artifacts`             | collected from `run.steps[].outputs`        |
/// | `llm_calls`             | mapped from `run.llm_calls`                 |
/// | `reference_software`    | from `run.environment.reference_software`   |
/// | `os_family`             | `run.environment.os_family`                 |
/// | `os_version`            | `run.environment.os_version`                |
/// | `release_version`       | `run.environment.release_version`           |
/// | `commit_sha`            | `run.environment.commit_sha`                |
/// | `created_at_utc`        | `run.created_at` (ISO-8601)                 |
/// | `api_keys`              | from active config (parameter)              |
/// | `working_directory`     | from active config (parameter)              |
/// | `narrative_steps`       | mapped from `run.steps[].narrative`         |
///
/// ## Privacy constraint (Requirement 6.9)
///
/// `api_keys` is accepted as a parameter so the exporter can use it for
/// redaction, but **no value drawn from `api_keys` is placed into any
/// analysis content field** of the returned `RunSnapshot`. The keys flow
/// only into `RunSnapshot.api_keys` which the exporter uses exclusively
/// as a redaction list.
#[must_use]
pub fn build_run_snapshot(
    run: &AnalysisRun,
    dataset_csv_bytes: Vec<u8>,
    api_keys: &[String],
    working_directory: Option<PathBuf>,
) -> RunSnapshot {
    // --- status (Requirement 6.4) ---
    let status = core_status_to_snapshot(run.status);

    // --- dataset_sha256 (Requirement 6.5) ---
    let dataset_sha256 = hex_to_bytes32(&run.dataset_sha256);

    // --- workflow + artifacts (Requirement 6.3) ---
    let mut workflow_steps = Vec::with_capacity(run.steps.len());
    let mut artifacts = Vec::new();
    let mut narrative_steps = Vec::new();

    for step in &run.steps {
        // Map step outputs to WorkflowStep ArtifactRef entries
        let outputs: Vec<ArtifactRef> = step
            .outputs
            .iter()
            .map(|a| ArtifactRef {
                path: a.path.clone(),
                sha256: "0".repeat(64), // artifact SHA256 not stored in RunArtifact
            })
            .collect();

        // Collect SnapshotArtifacts (the exporter needs the bytes; we record
        // empty bytes here since the actual artifact bytes are read at export
        // time by the provider, not by this projection function).
        for a in &step.outputs {
            artifacts.push(SnapshotArtifact {
                path: a.path.clone(),
                bytes: Vec::new(),
            });
        }

        workflow_steps.push(WorkflowStep {
            id: step.step_id.clone(),
            algorithm: run.algorithm_id.clone(),
            params: serde_json::Value::Object(serde_json::Map::new()),
            inputs: vec![ArtifactRef {
                path: "data.csv".to_string(),
                sha256: run.dataset_sha256.clone(),
            }],
            outputs,
            reference_software: None,
            llm: None,
            started_at_utc: step.started_at_utc.to_rfc3339(),
            ended_at_utc: step.ended_at_utc.to_rfc3339(),
        });

        // Collect narrative steps from step annotations
        if let Some(narr) = &step.narrative {
            narrative_steps.push(SnapshotNarrativeStep {
                id: step.step_id.clone(),
                algorithm: run.algorithm_id.clone(),
                display_name: narr.title.clone(),
                params_summary: String::new(),
                key_metrics: Vec::new(),
            });
        }
    }

    let workflow = Workflow {
        schema_version: 1,
        input_dataset: InputDataset {
            path: "data.csv".to_string(),
            sha256: run.dataset_sha256.clone(),
        },
        steps: workflow_steps,
    };

    // --- llm_calls (Requirement 6.3) ---
    let llm_calls: Vec<SnapshotLlmCall> = run
        .llm_calls
        .iter()
        .map(|call| SnapshotLlmCall {
            provider: call
                .references
                .first()
                .map(|r| r.source.clone())
                .unwrap_or_default(),
            model: call.model.clone(),
            request_at_utc: call.started_at_utc.to_rfc3339(),
            prompt_sha256: "0".repeat(64),
            response_sha256: "0".repeat(64),
        })
        .collect();

    // --- reference_software (Requirement 6.3) ---
    let reference_software: Vec<ReferenceSoftwareVersion> = run
        .environment
        .reference_software
        .iter()
        .map(|rs| ReferenceSoftwareVersion {
            name: rs.name.clone(),
            version: rs.version.clone(),
        })
        .collect();

    // --- environment fields (Requirement 6.3) ---
    let os_family = run.environment.os_family.clone();
    let os_version = run.environment.os_version.clone().unwrap_or_default();
    let release_version = run.environment.release_version.clone();
    let commit_sha = run.environment.commit_sha.clone();

    // --- created_at_utc (Requirement 6.3) ---
    let created_at_utc = run.created_at.to_rfc3339();

    // --- api_keys (Requirement 6.9): passed through for redaction only ---
    // NEVER placed into analysis content fields.
    let api_keys = api_keys.to_vec();

    RunSnapshot {
        run_id: run.run_id.clone(),
        status,
        dataset_sha256,
        dataset_csv_bytes,
        workflow,
        artifacts,
        llm_calls,
        reference_software,
        os_family,
        os_version,
        release_version,
        commit_sha,
        created_at_utc,
        api_keys,
        working_directory,
        narrative_steps,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coverage_dto_round_trips_every_embedded_cell() {
        let matrix = CoverageMatrix::get_loaded();
        let dto = coverage_matrix_to_dto(matrix);

        // Same schema + release version.
        assert_eq!(dto.schema_version, matrix.schema_version);
        assert_eq!(dto.release_version, matrix.release_version);

        // Same algorithm count and per-cell coverage values.
        assert_eq!(dto.algorithms.len(), matrix.algorithms.len());
        for (entry, dto_entry) in matrix.algorithms.iter().zip(&dto.algorithms) {
            assert_eq!(entry.id, dto_entry.id);
            assert_eq!(entry.display_name, dto_entry.display_name);
            assert_eq!(entry.iterative, dto_entry.iterative);
            assert_eq!(entry.coverage.len(), dto_entry.coverage.len());
            for (sw, state) in &entry.coverage {
                let got = dto_entry
                    .coverage
                    .get(&software_to_dto(*sw))
                    .expect("every coverage cell maps to a DTO cell");
                assert_eq!(*got, coverage_to_dto(*state));
            }
        }
    }

    #[test]
    fn embedded_provider_serializes_to_json() {
        let provider = EmbeddedCoverageMatrixProvider;
        let dto = provider.get();
        let json = serde_json::to_string(&dto).expect("dto serializes");
        assert!(json.contains("\"schema_version\""));
        assert!(json.contains("\"algorithms\""));
    }

    #[test]
    fn reference_dto_falls_back_to_proc_when_callable_absent() {
        let reference = ReferenceImpl {
            callable: None,
            proc: Some("PROC FREQ".to_string()),
            package: None,
            version: "9.4M8".to_string(),
        };
        let dto = reference_to_dto(&reference);
        assert_eq!(dto.callable, "PROC FREQ");
        assert_eq!(dto.version, "9.4M8");
        assert!(dto.package.is_none());
    }

    #[test]
    fn reference_dto_prefers_callable_over_proc() {
        let reference = ReferenceImpl {
            callable: Some("survival::coxph".to_string()),
            proc: Some("PROC PHREG".to_string()),
            package: Some("survival".to_string()),
            version: "3.7-0".to_string(),
        };
        let dto = reference_to_dto(&reference);
        assert_eq!(dto.callable, "survival::coxph");
        assert_eq!(dto.package.as_deref(), Some("survival"));
    }

    #[test]
    fn sidecar_unknown_algorithm_is_404() {
        let provider = LiveSidecarProvider;
        let req = SidecarRenderRequest {
            software: DtoSoftware::R,
            dataset_sha256: "0".repeat(64),
            columns: vec![],
            params: std::collections::BTreeMap::new(),
        };
        let err = provider
            .generate("no_such_algorithm", &req)
            .expect_err("unknown algorithm must error");
        assert_eq!(
            err,
            SidecarProviderError::UnknownAlgorithm("no_such_algorithm".to_string())
        );
    }

    #[test]
    fn sidecar_renders_real_snippet_for_covered_cell() {
        let provider = LiveSidecarProvider;
        let sha = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let req = SidecarRenderRequest {
            software: DtoSoftware::R,
            dataset_sha256: sha.to_string(),
            // Wave-1 templates reference up to `{{column.1.…}}`, so supply
            // two columns (the realistic tableone shape: a grouping column
            // plus an analysis variable).
            columns: vec![
                api::sidecar::SidecarColumnDto {
                    name: "group".to_string(),
                    dtype: "categorical".to_string(),
                },
                api::sidecar::SidecarColumnDto {
                    name: "age".to_string(),
                    dtype: "numeric".to_string(),
                },
            ],
            params: std::collections::BTreeMap::new(),
        };
        // `tableone` is `live` for R in the embedded matrix.
        let dto = provider
            .generate("tableone", &req)
            .expect("covered cell must render");
        assert_eq!(dto.algorithm_id, "tableone");
        assert_eq!(dto.software, DtoSoftware::R);
        assert_eq!(dto.coverage_value, CoverageValueDto::Live);
        let text = dto.text.expect("covered cell carries snippet text");
        // Real, non-empty snippet that embeds the contractual tokens.
        assert!(text.contains("data.csv"), "snippet must reference data.csv");
        assert!(text.contains(sha), "snippet must embed the dataset sha256");
        assert!(text.contains("age"), "snippet must reference the column");
        assert!(!text.contains('\r'), "snippet must be LF-only");
        assert_eq!(dto.sha256_of_dataset, sha);
    }

    #[test]
    fn sidecar_none_cell_returns_placeholder_dto() {
        let provider = LiveSidecarProvider;
        let req = SidecarRenderRequest {
            software: DtoSoftware::SPSS,
            dataset_sha256: "a".repeat(64),
            columns: vec![],
            params: std::collections::BTreeMap::new(),
        };
        // `standardization` × SPSS is `none` in the embedded matrix.
        let dto = provider
            .generate("standardization", &req)
            .expect("none cell returns a DTO, not an error");
        assert_eq!(dto.coverage_value, CoverageValueDto::None_);
        assert!(dto.text.is_none(), "none cell carries no snippet text");
    }

    #[test]
    fn sidecar_rejects_unknown_dtype_as_invalid_request() {
        let provider = LiveSidecarProvider;
        let req = SidecarRenderRequest {
            software: DtoSoftware::R,
            dataset_sha256: "0".repeat(64),
            columns: vec![api::sidecar::SidecarColumnDto {
                name: "x".to_string(),
                dtype: "blob".to_string(),
            }],
            params: std::collections::BTreeMap::new(),
        };
        let err = provider
            .generate("tableone", &req)
            .expect_err("unknown dtype must be rejected");
        match err {
            SidecarProviderError::InvalidRequest(msg) => {
                assert!(msg.contains("dtype"), "got: {msg}");
            }
            other => panic!("expected InvalidRequest, got {other:?}"),
        }
    }

    #[test]
    fn sidecar_rejects_malformed_sha256() {
        let provider = LiveSidecarProvider;
        let req = SidecarRenderRequest {
            software: DtoSoftware::R,
            dataset_sha256: "tooshort".to_string(),
            columns: vec![],
            params: std::collections::BTreeMap::new(),
        };
        let err = provider
            .generate("tableone", &req)
            .expect_err("malformed sha must be rejected");
        assert!(matches!(err, SidecarProviderError::InvalidRequest(_)));
    }

    #[test]
    fn snapshot_reports_unavailable_without_run_store() {
        let provider = UnavailableSnapshotProvider;
        let err = provider
            .export("run-1", "C:/tmp/out.zip")
            .expect_err("run-state-less build must report unavailable");
        match err {
            SnapshotProviderError::Internal(msg) => {
                assert!(msg.contains("per-run state store"), "got: {msg}");
            }
            other => panic!("expected Internal, got {other:?}"),
        }
    }

    #[test]
    fn build_run_snapshot_projects_all_fields() {
        use agent_core::models::run::AnalysisRun;

        let run: AnalysisRun = serde_json::from_str(r#"{
            "run_id": "run-abc",
            "algorithm_id": "linear",
            "dataset_id": "ds-1",
            "dataset_sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "status": "completed",
            "steps": [{
                "step_id": "step-1",
                "name": "Linear regression",
                "started_at_utc": "2024-06-01T12:00:00Z",
                "ended_at_utc": "2024-06-01T12:01:00Z",
                "outputs": [],
                "narrative": null
            }],
            "llm_calls": [{
                "call_id": "call-1",
                "step_id": "step-1",
                "model": "gpt-4o",
                "prompt_tokens": 100,
                "completion_tokens": 50,
                "started_at_utc": "2024-06-01T12:00:30Z",
                "ended_at_utc": "2024-06-01T12:00:35Z",
                "references": [{"source": "openai", "model": "gpt-4o", "version": "2024-05-13"}]
            }],
            "environment": {
                "os_family": "Windows",
                "os_version": "10.0.22631",
                "release_version": "0.5.0",
                "commit_sha": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                "reference_software": []
            },
            "created_at": "2024-06-01T12:00:00Z",
            "updated_at": "2024-06-01T12:01:00Z"
        }"#).expect("valid AnalysisRun JSON");

        let api_keys = vec!["sk-secret-key-123".to_string()];
        let csv_bytes = b"col1,col2\n1,2\n".to_vec();

        let snapshot = build_run_snapshot(
            &run,
            csv_bytes.clone(),
            &api_keys,
            Some(PathBuf::from("C:/work")),
        );

        // Core fields from the run
        assert_eq!(snapshot.run_id, "run-abc");
        assert_eq!(snapshot.status, crate::snapshot::RunStatus::Completed);
        assert_eq!(snapshot.dataset_sha256, [0xaa; 32]); // "a" repeated = 0xaa bytes
        assert_eq!(snapshot.dataset_csv_bytes, csv_bytes);
        assert_eq!(snapshot.os_family, "Windows");
        assert_eq!(snapshot.os_version, "10.0.22631");
        assert_eq!(snapshot.release_version, "0.5.0");
        assert_eq!(snapshot.commit_sha, "b".repeat(40));

        // Workflow
        assert_eq!(snapshot.workflow.schema_version, 1);
        assert_eq!(snapshot.workflow.input_dataset.sha256, "a".repeat(64));
        assert_eq!(snapshot.workflow.steps.len(), 1);
        assert_eq!(snapshot.workflow.steps[0].id, "step-1");
        assert_eq!(snapshot.workflow.steps[0].algorithm, "linear");

        // LLM calls
        assert_eq!(snapshot.llm_calls.len(), 1);
        assert_eq!(snapshot.llm_calls[0].provider, "openai");
        assert_eq!(snapshot.llm_calls[0].model, "gpt-4o");

        // api_keys passed through for redaction (Requirement 6.9)
        assert_eq!(snapshot.api_keys, vec!["sk-secret-key-123".to_string()]);

        // working_directory from config
        assert_eq!(snapshot.working_directory, Some(PathBuf::from("C:/work")));

        // CRITICAL: api_keys must NOT appear in analysis content fields
        // (Requirement 6.9 — redaction soundness). Verify no analysis
        // content field contains the secret.
        let secret = "sk-secret-key-123";
        assert!(!snapshot.run_id.contains(secret));
        assert!(!snapshot.os_family.contains(secret));
        assert!(!snapshot.os_version.contains(secret));
        assert!(!snapshot.release_version.contains(secret));
        assert!(!snapshot.commit_sha.contains(secret));
        assert!(!snapshot.created_at_utc.contains(secret));
        for step in &snapshot.workflow.steps {
            assert!(!step.id.contains(secret));
            assert!(!step.algorithm.contains(secret));
        }
        for call in &snapshot.llm_calls {
            assert!(!call.provider.contains(secret));
            assert!(!call.model.contains(secret));
        }
    }

    #[test]
    fn build_run_snapshot_empty_run_produces_valid_snapshot() {
        use agent_core::models::run::AnalysisRun;

        let run: AnalysisRun = serde_json::from_str(r#"{
            "run_id": "run-empty",
            "algorithm_id": "cox",
            "dataset_id": "ds-2",
            "dataset_sha256": "0000000000000000000000000000000000000000000000000000000000000000",
            "status": "running",
            "steps": [],
            "llm_calls": [],
            "environment": {
                "os_family": "Linux",
                "os_version": null,
                "release_version": "0.4.0",
                "commit_sha": "cccccccccccccccccccccccccccccccccccccccc",
                "reference_software": []
            },
            "created_at": "2024-06-01T12:00:00Z",
            "updated_at": "2024-06-01T12:00:00Z"
        }"#).expect("valid AnalysisRun JSON");

        let snapshot = build_run_snapshot(&run, vec![], &[], None);

        assert_eq!(snapshot.run_id, "run-empty");
        assert_eq!(snapshot.status, crate::snapshot::RunStatus::Running);
        assert_eq!(snapshot.dataset_sha256, [0u8; 32]);
        assert!(snapshot.dataset_csv_bytes.is_empty());
        assert!(snapshot.workflow.steps.is_empty());
        assert!(snapshot.llm_calls.is_empty());
        assert!(snapshot.artifacts.is_empty());
        assert!(snapshot.narrative_steps.is_empty());
        assert!(snapshot.api_keys.is_empty());
        assert!(snapshot.working_directory.is_none());
        assert_eq!(snapshot.os_version, ""); // None → empty string
    }

    // -----------------------------------------------------------------------
    // Regression tests for the two latent hazards found during the
    // architecture/diagnose review of sidecar-snapshot-integration.
    // -----------------------------------------------------------------------

    /// Build a fully-formed, completed `AnalysisRun` (one step) with the given
    /// `dataset_id` and `dataset_sha256`, deserialized from JSON to avoid
    /// importing the whole model tree.
    fn completed_run_json(dataset_id: &str, dataset_sha256: &str) -> agent_core::models::run::AnalysisRun {
        let json = format!(
            r#"{{
                "run_id": "run-regression",
                "algorithm_id": "linear",
                "dataset_id": "{dataset_id}",
                "dataset_sha256": "{dataset_sha256}",
                "status": "completed",
                "steps": [{{
                    "step_id": "step-1",
                    "name": "Linear regression",
                    "started_at_utc": "2024-06-01T12:00:00Z",
                    "ended_at_utc": "2024-06-01T12:01:00Z",
                    "outputs": [],
                    "narrative": null
                }}],
                "llm_calls": [],
                "environment": {{
                    "os_family": "Windows",
                    "os_version": null,
                    "release_version": "0.5.0",
                    "commit_sha": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                    "reference_software": []
                }},
                "created_at": "2024-06-01T12:00:00Z",
                "updated_at": "2024-06-01T12:01:00Z"
            }}"#
        );
        serde_json::from_str(&json).expect("valid AnalysisRun JSON")
    }

    /// Hazard A regression: `RunBackedSnapshotProvider::export` is a SYNC trait
    /// method that internally awaits an async `RunStore`. The production server
    /// drives the handler on a multi-thread tokio worker, so the export runs
    /// *inside* a runtime. The original `Handle::current().block_on(..)` panics
    /// in that situation; the `block_in_place` form must not.
    ///
    /// This test reproduces the exact production execution context
    /// (`#[tokio::test(flavor = "multi_thread")]`) and asserts the call returns
    /// a value instead of panicking. Before the fix it aborts with
    /// "Cannot start a runtime from within a runtime".
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn export_does_not_panic_inside_multi_thread_runtime() {
        use agent_core::store::{FsDatasetStore, MemRunStore};
        use agent_core::traits::run_store::RunStore;

        // Save a dataset through the real FsDatasetStore (using its own
        // `get_path` so the test carries no layout knowledge) so the provider
        // can resolve it by id through the DatasetStore trait.
        let root = tempfile::tempdir().expect("tempdir");
        let ds_store = Arc::new(
            FsDatasetStore::new(root.path().to_path_buf())
                .await
                .expect("dataset store"),
        );
        let sid = agent_core::models::SessionId::new();
        let dataset_id = agent_core::models::DatasetId::new_v4();
        let path = ds_store.get_path(sid, dataset_id, "data.csv");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"col1,col2\n1,2\n").unwrap();

        let store = Arc::new(MemRunStore::new());
        store
            .begin_run(completed_run_json(&dataset_id.to_string(), &"a".repeat(64)))
            .await
            .unwrap();

        let provider = RunBackedSnapshotProvider::new(
            store,
            ds_store as Arc<dyn agent_core::traits::dataset_store::DatasetStore>,
            vec![],
            None,
        );

        let out = root.path().join("out.zip");
        // The assertion that matters is that this call *returns* (no panic).
        let result = provider.export("run-regression", &out.to_string_lossy());
        assert!(
            result.is_ok(),
            "export inside a multi-thread runtime must succeed without panicking, got: {result:?}"
        );
    }

    /// Hazard B regression: a present-but-malformed recorded SHA256 must be
    /// refused with a structured error, not silently decoded into a fabricated
    /// all-zero fingerprint that would forge the audit chain.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn export_refuses_malformed_dataset_sha256() {
        use agent_core::store::{FsDatasetStore, MemRunStore};
        use agent_core::traits::run_store::RunStore;

        let root = tempfile::tempdir().expect("tempdir");
        let ds_store = Arc::new(
            FsDatasetStore::new(root.path().to_path_buf())
                .await
                .expect("dataset store"),
        );
        let sid = agent_core::models::SessionId::new();
        let dataset_id = agent_core::models::DatasetId::new_v4();
        let path = ds_store.get_path(sid, dataset_id, "data.csv");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"col1\n1\n").unwrap();

        let store = Arc::new(MemRunStore::new());
        // 64 chars but contains non-hex ('z') → malformed, must be refused.
        store
            .begin_run(completed_run_json(&dataset_id.to_string(), &"z".repeat(64)))
            .await
            .unwrap();

        let provider = RunBackedSnapshotProvider::new(
            store,
            ds_store as Arc<dyn agent_core::traits::dataset_store::DatasetStore>,
            vec![],
            None,
        );

        let out = root.path().join("out.zip");
        let err = provider
            .export("run-regression", &out.to_string_lossy())
            .expect_err("malformed sha256 must be refused");
        match err {
            SnapshotProviderError::Internal(msg) => {
                assert!(msg.contains("malformed dataset SHA256"), "got: {msg}");
            }
            other => panic!("expected Internal(malformed sha), got {other:?}"),
        }
        // No partial output left behind.
        assert!(!out.exists(), "refused export must not leave a zip");
    }

    #[test]
    fn is_valid_dataset_sha256_accepts_only_64_lowercase_hex() {
        assert!(is_valid_dataset_sha256(&"a".repeat(64)));
        assert!(is_valid_dataset_sha256(
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
        ));
        // Too short / too long
        assert!(!is_valid_dataset_sha256(&"a".repeat(63)));
        assert!(!is_valid_dataset_sha256(&"a".repeat(65)));
        // Uppercase is not accepted (digests are emitted lowercase)
        assert!(!is_valid_dataset_sha256(&"A".repeat(64)));
        // Non-hex character
        assert!(!is_valid_dataset_sha256(&"z".repeat(64)));
        // Empty is NOT "valid" here; the caller treats "" as legacy-absent.
        assert!(!is_valid_dataset_sha256(""));
    }
}
