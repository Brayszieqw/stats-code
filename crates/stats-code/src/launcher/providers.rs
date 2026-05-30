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
//! ## Sidecar / snapshot
//!
//! [`RunBackedSidecarProvider`] and [`RunBackedSnapshotProvider`] depend on
//! a [`RunDataSource`] that can resolve a `run_id` to the per-run column
//! metadata, dataset bytes, workflow, and artifacts that
//! `sidecar::generate_snippet` (Requirement 2.5) and
//! `snapshot::export_snapshot` (Requirement 7.2) require. That run-state
//! store does not yet exist in the product (the design's "in-memory
//! `RunStore`" was never built), so the launcher injects
//! [`UnavailableRunDataSource`], which reports a structured
//! "run state unavailable" error. This keeps the endpoints honest — they
//! surface a clear 4xx/5xx instead of fabricating empty column lists that
//! would violate Requirement 2.5 — while leaving a single seam
//! ([`RunDataSource`]) to implement once a real run store lands.

use std::sync::Arc;

use agent_server::state::{
    CoverageMatrixProvider, SidecarProvider, SidecarProviderError, SnapshotProvider,
    SnapshotProviderError,
};
use api::sidecar::{
    AlgorithmEntryDto, CoverageMatrixDto, CoverageValueDto, ReferenceImplDto,
    ReferenceSoftware as DtoSoftware, SidecarSnippetDto, SnapshotExportResponse,
};

use crate::coverage_matrix::{
    CoverageMatrix, CoverageState, ReferenceImpl, ReferenceSoftware,
};

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
// RunDataSource — the seam the sidecar / snapshot providers need
// ---------------------------------------------------------------------------

/// Resolves a `run_id` to the per-run state required to render a sidecar
/// snippet or export an audit snapshot.
///
/// This is the single integration seam between the HTTP providers and a
/// future run-state store. A real implementation must return the input
/// column metadata, the dataset SHA256, the resolved analysis parameters
/// (sidecar), and the full materialized run (snapshot). Until that store
/// exists, [`UnavailableRunDataSource`] is injected.
pub trait RunDataSource: Send + Sync {
    /// Whether run-state lookups are supported in this deployment.
    fn is_available(&self) -> bool;
}

/// The no-op run data source injected while no run-state store exists.
///
/// Every lookup is unavailable, so the sidecar / snapshot providers
/// surface a structured error rather than fabricate run state.
#[derive(Debug, Default, Clone, Copy)]
pub struct UnavailableRunDataSource;

impl RunDataSource for UnavailableRunDataSource {
    fn is_available(&self) -> bool {
        false
    }
}

const RUN_STATE_UNAVAILABLE_MSG: &str =
    "analysis run-state store is not yet available in this build; \
     sidecar snippet generation and snapshot export require per-run \
     column metadata, dataset bytes, and artifacts that no run store \
     currently provides";

// ---------------------------------------------------------------------------
// SidecarProvider — gated on RunDataSource availability
// ---------------------------------------------------------------------------

/// Generates Equivalent Code Sidecar snippets for the active run.
///
/// Functional once a [`RunDataSource`] that resolves real per-run column
/// metadata and dataset SHA256 is supplied. With
/// [`UnavailableRunDataSource`] every call returns
/// [`SidecarProviderError::Internal`] carrying [`RUN_STATE_UNAVAILABLE_MSG`].
pub struct RunBackedSidecarProvider {
    run_source: Arc<dyn RunDataSource>,
}

impl RunBackedSidecarProvider {
    /// Construct a sidecar provider backed by the given run data source.
    #[must_use]
    pub fn new(run_source: Arc<dyn RunDataSource>) -> Self {
        Self { run_source }
    }
}

impl SidecarProvider for RunBackedSidecarProvider {
    fn generate(
        &self,
        algorithm_id: &str,
        _software: DtoSoftware,
        _run_id: &str,
    ) -> Result<SidecarSnippetDto, SidecarProviderError> {
        // Validate the algorithm id against the embedded matrix first so a
        // genuinely unknown id surfaces as 404 regardless of run-state
        // availability.
        let matrix = CoverageMatrix::get_loaded();
        if matrix.lookup(algorithm_id).is_none() {
            return Err(SidecarProviderError::UnknownAlgorithm(
                algorithm_id.to_string(),
            ));
        }

        if !self.run_source.is_available() {
            return Err(SidecarProviderError::Internal(
                RUN_STATE_UNAVAILABLE_MSG.to_string(),
            ));
        }

        // A real RunDataSource would resolve columns / dataset SHA256 /
        // params here and call `sidecar::generate_snippet`. The seam is
        // intentionally left for the run-store feature.
        Err(SidecarProviderError::Internal(
            RUN_STATE_UNAVAILABLE_MSG.to_string(),
        ))
    }
}

// ---------------------------------------------------------------------------
// SnapshotProvider — gated on RunDataSource availability
// ---------------------------------------------------------------------------

/// Exports Audit Snapshots for completed runs.
///
/// Functional once a [`RunDataSource`] can materialize a
/// `snapshot::RunSnapshot`. With [`UnavailableRunDataSource`] every call
/// returns [`SnapshotProviderError::Internal`].
pub struct RunBackedSnapshotProvider {
    run_source: Arc<dyn RunDataSource>,
}

impl RunBackedSnapshotProvider {
    /// Construct a snapshot provider backed by the given run data source.
    #[must_use]
    pub fn new(run_source: Arc<dyn RunDataSource>) -> Self {
        Self { run_source }
    }
}

impl SnapshotProvider for RunBackedSnapshotProvider {
    fn export(
        &self,
        _run_id: &str,
        _destination: &str,
    ) -> Result<SnapshotExportResponse, SnapshotProviderError> {
        if !self.run_source.is_available() {
            return Err(SnapshotProviderError::Internal(
                RUN_STATE_UNAVAILABLE_MSG.to_string(),
            ));
        }

        // A real RunDataSource would materialize a `RunSnapshot` here and
        // call `snapshot::export_snapshot`.
        Err(SnapshotProviderError::Internal(
            RUN_STATE_UNAVAILABLE_MSG.to_string(),
        ))
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
    fn sidecar_unknown_algorithm_is_404_even_without_run_state() {
        let provider =
            RunBackedSidecarProvider::new(Arc::new(UnavailableRunDataSource));
        let err = provider
            .generate("no_such_algorithm", DtoSoftware::R, "run-1")
            .expect_err("unknown algorithm must error");
        assert_eq!(
            err,
            SidecarProviderError::UnknownAlgorithm("no_such_algorithm".to_string())
        );
    }

    #[test]
    fn sidecar_known_algorithm_reports_run_state_unavailable() {
        let provider =
            RunBackedSidecarProvider::new(Arc::new(UnavailableRunDataSource));
        // `tableone` is present in the embedded matrix.
        let err = provider
            .generate("tableone", DtoSoftware::R, "run-1")
            .expect_err("run-state-less build must report unavailable");
        match err {
            SidecarProviderError::Internal(msg) => {
                assert!(msg.contains("run-state"), "got: {msg}");
            }
            other => panic!("expected Internal, got {other:?}"),
        }
    }

    #[test]
    fn snapshot_reports_run_state_unavailable() {
        let provider =
            RunBackedSnapshotProvider::new(Arc::new(UnavailableRunDataSource));
        let err = provider
            .export("run-1", "C:/tmp/out.zip")
            .expect_err("run-state-less build must report unavailable");
        match err {
            SnapshotProviderError::Internal(msg) => {
                assert!(msg.contains("run-state"), "got: {msg}");
            }
            other => panic!("expected Internal, got {other:?}"),
        }
    }
}
