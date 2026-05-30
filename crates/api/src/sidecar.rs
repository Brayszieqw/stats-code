//! Wire-format DTOs shared between the agent-server HTTP layer and the SPA
//! for the Equivalent Code Sidecar, the Algorithm Coverage Matrix, and the
//! Audit Snapshot Export endpoints.
//!
//! These types are pure data carriers (`serde::{Serialize, Deserialize}`) — no
//! domain logic, no I/O. Server-side handlers and SPA clients agree on this
//! schema; richer in-memory models live in `crates/stats-code`.
//!
//! Validates: Requirements 1.3 (Equivalent Code Sidecar transport),
//! 6.2 (Algorithm Coverage Matrix transport), 7.1 (Audit Snapshot export
//! request/response).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// One of the four reference software products tracked by the coverage
/// matrix and the sidecar generator. Serialized using the exact tokens
/// recorded in `coverage_matrix/matrix.toml` so the wire format is stable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ReferenceSoftware {
    R,
    SAS,
    Python,
    SPSS,
}

/// JSON union of the four lowercase coverage tokens.
///
/// The variant name `None_` carries a trailing underscore to avoid colliding
/// with `Option::None` in pattern positions; the wire form is exactly
/// `"none"` thanks to `rename`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum CoverageValueDto {
    #[serde(rename = "live")]
    Live,
    #[serde(rename = "recorded")]
    Recorded,
    #[serde(rename = "sidecar_only")]
    SidecarOnly,
    #[serde(rename = "none")]
    None_,
}

impl CoverageValueDto {
    /// Stable lowercase token, identical to the JSON serialization.
    #[must_use] 
    pub fn as_token(self) -> &'static str {
        match self {
            CoverageValueDto::Live => "live",
            CoverageValueDto::Recorded => "recorded",
            CoverageValueDto::SidecarOnly => "sidecar_only",
            CoverageValueDto::None_ => "none",
        }
    }
}

/// Pinned reference implementation metadata for one (algorithm, software)
/// cell of the Algorithm Coverage Matrix.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReferenceImplDto {
    /// Function name (e.g. `tableone::CreateTableOne`) or PROC name
    /// (e.g. `PROC FREQ;PROC MEANS`).
    pub callable: String,
    /// Optional package / library identifier (e.g. `tableone`, `scipy`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package: Option<String>,
    /// Pinned version recorded with the matrix entry.
    pub version: String,
}

/// One Output-Level Algorithm row in the Algorithm Coverage Matrix.
///
/// `coverage` and `reference` are keyed by `ReferenceSoftware` and serialize
/// as JSON objects with the `R | SAS | Python | SPSS` keys. `BTreeMap`
/// guarantees a deterministic key order (alphabetical by enum
/// representation), which keeps the wire bytes stable across hosts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AlgorithmEntryDto {
    /// Exact-match identifier used by the `--filter` flag and by the
    /// sidecar / snapshot path lookups.
    pub id: String,
    pub display_name: String,
    pub iterative: bool,
    pub coverage: BTreeMap<ReferenceSoftware, CoverageValueDto>,
    pub reference: BTreeMap<ReferenceSoftware, ReferenceImplDto>,
}

/// JSON shape returned by `GET /api/coverage-matrix` and embedded in the
/// `coverage.json` member of an Audit Snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoverageMatrixDto {
    /// Bumped on backwards-incompatible schema changes.
    pub schema_version: u32,
    /// Stats Code release that produced this matrix snapshot.
    pub release_version: String,
    pub algorithms: Vec<AlgorithmEntryDto>,
}

/// Response body of `GET /api/sidecar/{algorithm_id}?software=...&run_id=...`.
///
/// One DTO covers all four coverage states. When `coverage_value` is `None_`
/// the snippet body is omitted (the SPA renders the placeholder defined in
/// Requirement 1.5/1.6). When `coverage_value` is `SidecarOnly` the body is
/// present and the SPA renders the inline notice defined in Requirement 6.4.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SidecarSnippetDto {
    pub algorithm_id: String,
    pub software: ReferenceSoftware,
    pub coverage_value: CoverageValueDto,
    /// UTF-8 snippet text with LF line endings. Absent for `coverage_value
    /// = "none"`; present for the other three variants.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// 64-character lowercase hexadecimal SHA256 of the input dataset; the
    /// SPA's `<SidecarFooter>` renders this regardless of coverage state.
    pub sha256_of_dataset: String,
    /// Stats Code release version that emitted the snippet.
    pub release_version: String,
}

/// One input column carried in a [`SidecarRenderRequest`].
///
/// `dtype` is one of the four lowercase tokens the Sidecar Code Generator
/// understands (`numeric | categorical | date | string`); unknown tokens
/// are rejected server-side so a malformed column never silently renders
/// the wrong dtype.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SidecarColumnDto {
    pub name: String,
    pub dtype: String,
}

/// Request body of `POST /api/sidecar/{algorithm_id}`.
///
/// The Equivalent Code Sidecar is a **pure function** of
/// `(algorithm_id, software, columns, dataset_sha256, params)` — it needs
/// no server-side run state. The SPA already holds every field (the
/// algorithm id and params come from the configurator, the columns and
/// dataset SHA256 come from the dataset-upload response), so it posts them
/// directly and the server renders the snippet without any run-store
/// lookup. This is what makes the sidecar functional end-to-end.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SidecarRenderRequest {
    /// Reference software for the requested tab.
    pub software: ReferenceSoftware,
    /// 64-character lowercase hexadecimal SHA256 of the input dataset.
    pub dataset_sha256: String,
    /// Input column metadata in dataset order (drives `{{column.<i>.…}}`
    /// placeholders and the snippet header).
    #[serde(default)]
    pub columns: Vec<SidecarColumnDto>,
    /// Algorithm parameters as `{{params.<key>}}` substitutions. Values are
    /// pre-stringified by the caller.
    #[serde(default)]
    pub params: BTreeMap<String, String>,
}

/// Request body of `POST /api/snapshot/export`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotExportRequest {
    /// Identifier of the analysis run to export. The server enforces
    /// `run.status == completed` before producing an Audit Snapshot.
    pub run_id: String,
    /// User-selected destination path for the resulting `.zip` file.
    /// The exporter writes to `<destination>.tmp` and atomically renames.
    pub destination: String,
}

/// Response body of `POST /api/snapshot/export`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotExportResponse {
    /// Final path of the produced `.zip` Audit Snapshot. Equal to the
    /// `destination` field of the request on success.
    pub snapshot_path: String,
    /// 64-character lowercase hexadecimal SHA256 of the produced
    /// `.zip` bytes.
    pub sha256: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn coverage_value_serializes_to_lowercase_tokens() {
        assert_eq!(
            serde_json::to_value(CoverageValueDto::Live).unwrap(),
            json!("live")
        );
        assert_eq!(
            serde_json::to_value(CoverageValueDto::Recorded).unwrap(),
            json!("recorded")
        );
        assert_eq!(
            serde_json::to_value(CoverageValueDto::SidecarOnly).unwrap(),
            json!("sidecar_only")
        );
        assert_eq!(
            serde_json::to_value(CoverageValueDto::None_).unwrap(),
            json!("none")
        );
    }

    #[test]
    fn coverage_value_round_trip() {
        for v in [
            CoverageValueDto::Live,
            CoverageValueDto::Recorded,
            CoverageValueDto::SidecarOnly,
            CoverageValueDto::None_,
        ] {
            let s = serde_json::to_string(&v).unwrap();
            let back: CoverageValueDto = serde_json::from_str(&s).unwrap();
            assert_eq!(v, back);
            assert_eq!(s.trim_matches('"'), v.as_token());
        }
    }

    #[test]
    fn reference_software_keys_are_exact_tokens() {
        let mut coverage: BTreeMap<ReferenceSoftware, CoverageValueDto> = BTreeMap::new();
        coverage.insert(ReferenceSoftware::R, CoverageValueDto::Live);
        coverage.insert(ReferenceSoftware::SAS, CoverageValueDto::Recorded);
        coverage.insert(ReferenceSoftware::Python, CoverageValueDto::Live);
        coverage.insert(ReferenceSoftware::SPSS, CoverageValueDto::Recorded);

        let json = serde_json::to_value(&coverage).unwrap();
        // Keys must match the matrix.toml tokens verbatim.
        assert!(json.get("R").is_some(), "missing R key in {json}");
        assert!(json.get("SAS").is_some(), "missing SAS key in {json}");
        assert!(json.get("Python").is_some(), "missing Python key in {json}");
        assert!(json.get("SPSS").is_some(), "missing SPSS key in {json}");
    }

    #[test]
    fn sidecar_snippet_omits_text_for_none_variant() {
        let dto = SidecarSnippetDto {
            algorithm_id: "tableone".into(),
            software: ReferenceSoftware::SPSS,
            coverage_value: CoverageValueDto::None_,
            text: None,
            sha256_of_dataset: "0".repeat(64),
            release_version: "0.5.0".into(),
        };
        let v = serde_json::to_value(&dto).unwrap();
        assert!(v.get("text").is_none(), "text must be omitted, got {v}");
        assert_eq!(v["coverage_value"], json!("none"));
    }

    #[test]
    fn sidecar_snippet_round_trip_with_text() {
        let dto = SidecarSnippetDto {
            algorithm_id: "logistic".into(),
            software: ReferenceSoftware::R,
            coverage_value: CoverageValueDto::Live,
            text: Some("# header\nlibrary(stats)\n".into()),
            sha256_of_dataset: "a".repeat(64),
            release_version: "0.5.0".into(),
        };
        let s = serde_json::to_string(&dto).unwrap();
        let back: SidecarSnippetDto = serde_json::from_str(&s).unwrap();
        assert_eq!(dto, back);
    }

    #[test]
    fn coverage_matrix_round_trip() {
        let mut coverage = BTreeMap::new();
        coverage.insert(ReferenceSoftware::R, CoverageValueDto::Live);
        coverage.insert(ReferenceSoftware::SAS, CoverageValueDto::Recorded);
        coverage.insert(ReferenceSoftware::Python, CoverageValueDto::Live);
        coverage.insert(ReferenceSoftware::SPSS, CoverageValueDto::Recorded);

        let mut reference = BTreeMap::new();
        reference.insert(
            ReferenceSoftware::R,
            ReferenceImplDto {
                callable: "tableone::CreateTableOne".into(),
                package: Some("tableone".into()),
                version: "0.13.2".into(),
            },
        );

        let entry = AlgorithmEntryDto {
            id: "tableone".into(),
            display_name: "Table One".into(),
            iterative: false,
            coverage,
            reference,
        };
        let matrix = CoverageMatrixDto {
            schema_version: 1,
            release_version: "0.5.0".into(),
            algorithms: vec![entry],
        };
        let s = serde_json::to_string(&matrix).unwrap();
        let back: CoverageMatrixDto = serde_json::from_str(&s).unwrap();
        assert_eq!(matrix, back);
    }

    #[test]
    fn snapshot_export_round_trip() {
        let req = SnapshotExportRequest {
            run_id: "run-42".into(),
            destination: "C:/tmp/out.zip".into(),
        };
        let resp = SnapshotExportResponse {
            snapshot_path: "C:/tmp/out.zip".into(),
            sha256: "f".repeat(64),
        };
        let req_back: SnapshotExportRequest =
            serde_json::from_str(&serde_json::to_string(&req).unwrap()).unwrap();
        let resp_back: SnapshotExportResponse =
            serde_json::from_str(&serde_json::to_string(&resp).unwrap()).unwrap();
        assert_eq!(req, req_back);
        assert_eq!(resp, resp_back);
    }
}
