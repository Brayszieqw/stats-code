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
}
