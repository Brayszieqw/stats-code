//! Algorithm Coverage Matrix — single source of truth for parity coverage.
//!
//! Feature: parity-and-multilang-sidecar
//! Requirements: 6.1, 6.2
//!
//! For every Output-Level Algorithm in the Stats Engine, the matrix records
//! exactly one entry per Reference Software in `{R, SAS, Python, SPSS}`,
//! whose value is one of [`CoverageState::Live`], [`CoverageState::Recorded`],
//! [`CoverageState::SidecarOnly`], or [`CoverageState::None_`]. The matrix is
//! consumed by:
//!
//! - the Sidecar Code Generator, to decide whether to emit a snippet or a
//!   structured "uncovered" sentinel;
//! - the Equivalent Code Sidecar in the SPA, to decide whether to render a
//!   snippet, a snippet with an inline notice, or a labelled placeholder;
//! - the CI Parity Suite, to gate on coverage drift between declarations
//!   and the actual test surface;
//! - the Audit Snapshot Exporter, to write `coverage.json` into the snapshot.
//!
//! ## Wave-0 skeleton (task 1.1) and loader (task 1.2)
//!
//! Task 1.1 defined the data types and embedded the authoritative
//! `matrix.toml` text into the binary via [`MATRIX_TOML`]. Task 1.2 layered
//! on top: a TOML [`parse`] function with a structured [`ParseError`], an
//! `OnceLock`-backed global loader [`CoverageMatrix::get_loaded`], and
//! [`CoverageMatrix::lookup`] / [`CoverageMatrix::coverage`] /
//! [`CoverageMatrix::algorithms`] / [`CoverageMatrix::release_version`]
//! accessors. The `build.rs` step that injects the live `release_version`
//! and mirrors the file under `validation/` lands in task 1.3.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

/// Authoritative TOML text for the Algorithm Coverage Matrix, embedded into
/// the binary at compile time. Parsed lazily by the loader implemented in
/// task 1.2.
///
/// The bytes are the **build-rs–injected variant** of the matrix, not the
/// on-disk skeleton at `src/coverage_matrix/matrix.toml`. `build.rs`
/// (task 1.3) reads that skeleton, replaces its `release_version` placeholder
/// with the live `CARGO_PKG_VERSION`, normalizes line endings to LF, and
/// writes the result to `$OUT_DIR/matrix.toml`. The same bytes are mirrored
/// to `validation/coverage_matrix.toml` so the pytest parity suite reads
/// the version-stamped artifact too — keeping Rust and Python locked to a
/// single source of truth (Requirement 6.1).
pub const MATRIX_TOML: &str = include_str!(concat!(env!("OUT_DIR"), "/matrix.toml"));

/// Coverage state for a single (Output-Level Algorithm, Reference Software)
/// cell of the Algorithm Coverage Matrix.
///
/// The four variants are exhaustive (Requirement 6.2):
///
/// - [`Self::Live`] — the Live Reference Suite executes the Reference
///   Implementation in process and asserts parity within the Parity
///   Threshold for that algorithm.
/// - [`Self::Recorded`] — the Recorded Reference Suite asserts parity
///   against a Known-Values Table within the Parity Threshold.
/// - [`Self::SidecarOnly`] — the Sidecar Code Generator emits a snippet
///   for this cell, but no automated parity assertion exists.
/// - [`Self::None_`] — no Sidecar snippet is emitted and no parity
///   assertion exists. The trailing underscore avoids colliding with the
///   `None` variant of [`Option`].
///
/// Serde representation matches the wire / TOML form (`snake_case`), which
/// is what the SPA's TypeScript `CoverageState` union expects.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoverageState {
    /// Live Reference Suite covers this cell.
    Live,
    /// Recorded Reference Suite (Known-Values Table) covers this cell.
    Recorded,
    /// Sidecar Code Generator emits a snippet but no parity assertion exists.
    SidecarOnly,
    /// No snippet, no parity assertion. Spelled `None_` in Rust to avoid
    /// clashing with [`Option::None`]; the wire / TOML form is the bare
    /// token `"none"` (Requirement 6.2). The explicit `#[serde(rename)]`
    /// is required because `rename_all = "snake_case"` preserves the
    /// trailing underscore on `None_`, which would otherwise produce
    /// `"none_"` and reject the matrix file.
    #[serde(rename = "none")]
    None_,
}

/// One of the four Reference Software products recognized by the matrix.
///
/// "Python" specifically denotes the statsmodels / scipy / lifelines
/// ecosystem as appropriate per algorithm (Glossary in requirements.md).
///
/// Serde uses the canonical SPA-facing spelling (`R`, `SAS`, `Python`,
/// `SPSS`) so the JSON DTO and the TOML key strings agree byte-for-byte.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, serde::Serialize, serde::Deserialize)]
pub enum ReferenceSoftware {
    R,
    SAS,
    Python,
    SPSS,
}

/// Designated Reference Implementation for one (Output-Level Algorithm,
/// Reference Software) cell — i.e. the function or PROC that is treated as
/// the parity ground truth.
///
/// At least one of [`Self::callable`] or [`Self::proc`] is populated; SAS /
/// SPSS entries typically use `proc`, R / Python entries typically use
/// `callable`. [`Self::package`] is the host library or package and may be
/// `None` for built-in PROCs. [`Self::version`] is the pinned version
/// recorded with the entry, used by the Parity Validation Report header
/// (Requirement 3.6) and by the snapshot's `versions.json` (Requirement
/// 7.4).
#[derive(Clone, Eq, PartialEq, Debug, serde::Serialize, serde::Deserialize)]
pub struct ReferenceImpl {
    /// Function name (R / Python) when applicable, e.g. `stats::lm` or
    /// `scipy.stats.ttest_ind`. Mutually compatible with [`Self::proc`];
    /// the matrix entry sets the field that matches the software's
    /// invocation style.
    #[serde(rename = "fn", default, skip_serializing_if = "Option::is_none")]
    pub callable: Option<String>,

    /// PROC / procedure name (SAS / SPSS) when applicable, e.g.
    /// `PROC LIFETEST` or `LOGISTIC REGRESSION`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proc: Option<String>,

    /// Host package or library, e.g. `survival` (R) or `scipy` (Python).
    /// Optional for built-in PROCs.
    #[serde(rename = "pkg", default, skip_serializing_if = "Option::is_none")]
    pub package: Option<String>,

    /// Pinned version of the reference implementation as recorded in the
    /// matrix entry, e.g. `"3.7-0"`, `"9.4M8"`, `"1.13.0"`, `"29.0.1"`.
    pub version: String,
}

/// One entry of the Algorithm Coverage Matrix, describing every cell for a
/// single Output-Level Algorithm.
///
/// [`Self::id`] is the case-sensitive exact-match key used by
/// `--filter <algorithm>` on the internal `parity` subcommand
/// (Requirement 5.5).
///
/// [`Self::iterative`] flags Iterative Algorithms (Cox proportional
/// hazards, logistic regression). The flag drives Parity Threshold
/// defaults (relative `1e-4` for iterative, `1e-6` otherwise; see
/// requirements.md Glossary "Parity Threshold").
///
/// [`Self::coverage`] and [`Self::reference`] use [`BTreeMap`] so that
/// iteration order is the canonical [`ReferenceSoftware`] order, which
/// keeps emitted JSON / TOML byte-deterministic across hosts and clocks
/// (Requirement 2.1 idempotence-style guarantees).
#[derive(Clone, Eq, PartialEq, Debug, serde::Serialize, serde::Deserialize)]
pub struct AlgorithmEntry {
    /// Stable case-sensitive identifier, e.g. `"tableone"`, `"cox"`,
    /// `"logistic"`. Used for `--filter` matching.
    pub id: String,
    /// Human-readable display name, e.g. `"Cox Proportional Hazards"`.
    pub display_name: String,
    /// True for Iterative Algorithms (Cox PH, logistic regression).
    pub iterative: bool,
    /// Coverage state per Reference Software (one entry each for R, SAS,
    /// Python, SPSS).
    pub coverage: BTreeMap<ReferenceSoftware, CoverageState>,
    /// Designated Reference Implementation per Reference Software.
    pub reference: BTreeMap<ReferenceSoftware, ReferenceImpl>,
}

/// Parsed, immutable Algorithm Coverage Matrix.
///
/// Embedded once at process start (see [`MATRIX_TOML`]) and exposed as a
/// `&'static` reference by the loader implemented in task 1.2. Consumers
/// must treat the matrix as read-only — any drift between this in-memory
/// view and the on-disk `validation/coverage_matrix.toml` mirror is a CI
/// failure (Requirement 6.6, gated by task 1.4 / task 11.6).
#[derive(Clone, Eq, PartialEq, Debug, serde::Serialize, serde::Deserialize)]
pub struct CoverageMatrix {
    /// Matrix schema version. Currently always `1`.
    pub schema_version: u32,
    /// Stats Code release version that produced this matrix (injected by
    /// `build.rs` in task 1.3 from `CARGO_PKG_VERSION`). The literal
    /// `"0.0.0-build-injected"` shipped in the on-disk skeleton is
    /// replaced before the binary is linked.
    pub release_version: String,
    /// One entry per Output-Level Algorithm.
    #[serde(rename = "algorithm", default)]
    pub algorithms: Vec<AlgorithmEntry>,
}

/// Set of every Reference Software the matrix MUST cover for every
/// algorithm (Requirement 6.1: "exactly one entry per Reference Software in
/// `{R, SAS, Python, SPSS}`"). Keep this in sync with [`ReferenceSoftware`].
const REQUIRED_SOFTWARE: [ReferenceSoftware; 4] = [
    ReferenceSoftware::R,
    ReferenceSoftware::SAS,
    ReferenceSoftware::Python,
    ReferenceSoftware::SPSS,
];

impl ReferenceSoftware {
    /// Stable wire token, identical to the TOML key spelling and the JSON
    /// representation produced by `serde::Serialize`.
    #[must_use] 
    pub fn as_token(self) -> &'static str {
        match self {
            ReferenceSoftware::R => "R",
            ReferenceSoftware::SAS => "SAS",
            ReferenceSoftware::Python => "Python",
            ReferenceSoftware::SPSS => "SPSS",
        }
    }
}

/// Errors returned by [`parse`] when the embedded — or any otherwise
/// supplied — `matrix.toml` text cannot be turned into a structurally valid
/// [`CoverageMatrix`].
///
/// The variants are split between failures surfaced by `serde` /
/// `toml::de::Error` (covered by [`ParseError::Toml`]) and structural
/// invariants enforced after deserialization succeeds. Hand-rolled
/// validation gives actionable, location-bearing error messages without
/// having to reach into `toml`'s span types.
///
/// _Requirements: 6.1, 6.2_
#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    /// TOML deserialization failure. `toml::de::Error` carries the original
    /// span so callers (notably the `OnceLock` loader's panic message) can
    /// pinpoint the offending line and column.
    #[error("matrix.toml is not valid TOML: {0}")]
    Toml(#[from] toml::de::Error),

    /// A required field was missing on a specific algorithm entry — for
    /// example a `[[algorithm]]` block without an `id`. `entry` is the best
    /// identifier available (the algorithm `id` if present, otherwise the
    /// 0-based index of the block).
    #[error("algorithm entry {entry:?} is missing required field {field:?}")]
    MissingField {
        entry: String,
        field: &'static str,
    },

    /// An enum-valued field (`coverage` value, `ReferenceSoftware` key) had
    /// an unrecognized string. `serde` already returns these via
    /// [`ParseError::Toml`]; this hand-rolled variant exists for cases
    /// where validation runs after deserialization (e.g. a future strict
    /// mode that re-validates token sets).
    #[error("algorithm entry {entry:?} field {field:?} has unknown value {value:?}")]
    UnknownEnum {
        entry: String,
        field: &'static str,
        value: String,
    },

    /// Two coverage cells under the same `[algorithm.coverage]` block point
    /// at the same software. With the current TOML representation this is
    /// rejected by the underlying parser; the variant is kept so the
    /// loader can surface a uniform error if a future schema (e.g.
    /// list-of-pairs) re-enables ambiguity.
    #[error(
        "algorithm entry {entry:?} declares software {software:?} more than once in its coverage map"
    )]
    DuplicateCell { entry: String, software: String },

    /// Two `[[algorithm]]` blocks share the same `id` (Requirement 5.5
    /// requires identifiers to be exact-match-unique).
    #[error("duplicate algorithm id {id:?}")]
    DuplicateAlgorithmId { id: String },

    /// An algorithm declared coverage for fewer than four softwares
    /// (Requirement 6.1 mandates exactly one entry per `{R, SAS, Python,
    /// SPSS}` per algorithm). `missing_software` lists the offending key.
    #[error(
        "algorithm entry {entry:?} is missing coverage cell for software {missing_software:?}"
    )]
    IncompleteCoverage {
        entry: String,
        missing_software: String,
    },
}

/// Parse a TOML byte slice into a [`CoverageMatrix`], applying the
/// structural invariants of Requirement 6.1 / 6.2 on top of `serde`'s
/// schema check.
///
/// The strategy is intentionally cheap: validate UTF-8, run
/// `toml::from_str`, then walk the parsed `algorithms` vector to enforce
/// uniqueness of algorithm ids, and presence of every (algorithm, software)
/// cell in both the `coverage` and `reference` maps. Errors are returned
/// by value via [`ParseError`] so callers (e.g. the `OnceLock` loader) can
/// log `Display` text and decide whether to panic.
///
/// _Requirements: 6.1, 6.2_
pub fn parse(toml_bytes: &[u8]) -> Result<CoverageMatrix, ParseError> {
    // serde's TOML deserializer takes `&str`; surface bad UTF-8 as a
    // `Toml` error by reusing toml's own message. We synthesize a
    // `toml::de::Error` via the public `serde::de::Error::custom`
    // constructor.
    let toml_str = std::str::from_utf8(toml_bytes).map_err(|e| {
        let msg = format!("matrix.toml is not valid UTF-8: {e}");
        // `toml::de::Error` does not expose a public constructor, but
        // `serde::de::Error::custom` does, and `toml::de::Error` implements
        // `serde::de::Error`. Going through `custom` keeps the variant
        // shape clean without introducing a separate `Utf8` variant.
        <toml::de::Error as serde::de::Error>::custom(msg)
    })?;

    let matrix: CoverageMatrix = toml::from_str(toml_str)?;

    // Algorithm-id uniqueness (Requirement 5.5 case-sensitive exact match).
    let mut seen_ids: BTreeSet<&str> = BTreeSet::new();
    for entry in &matrix.algorithms {
        if entry.id.is_empty() {
            return Err(ParseError::MissingField {
                entry: format!("[{}]", index_label(&matrix.algorithms, entry)),
                field: "id",
            });
        }
        if !seen_ids.insert(entry.id.as_str()) {
            return Err(ParseError::DuplicateAlgorithmId {
                id: entry.id.clone(),
            });
        }
    }

    // Per-entry structural checks: every `{R, SAS, Python, SPSS}` cell
    // must appear in both `coverage` and `reference`.
    for entry in &matrix.algorithms {
        for software in REQUIRED_SOFTWARE {
            if !entry.coverage.contains_key(&software) {
                return Err(ParseError::IncompleteCoverage {
                    entry: entry.id.clone(),
                    missing_software: software.as_token().to_string(),
                });
            }
            if !entry.reference.contains_key(&software) {
                return Err(ParseError::IncompleteCoverage {
                    entry: entry.id.clone(),
                    missing_software: software.as_token().to_string(),
                });
            }
        }
    }

    Ok(matrix)
}

fn index_label(all: &[AlgorithmEntry], target: &AlgorithmEntry) -> String {
    // Compute the stable 0-based index of `target` within `all` by pointer
    // equality, falling back to "?" if the entry is somehow not part of
    // the slice (defensive — the only caller passes `all = matrix.algorithms`).
    all.iter()
        .position(|e| std::ptr::eq(e, target))
        .map_or_else(|| "?".to_string(), |i| i.to_string())
}

/// One-time process-wide cache for the embedded matrix. Filled on the first
/// call to [`CoverageMatrix::get_loaded`] and never invalidated.
static MATRIX: OnceLock<CoverageMatrix> = OnceLock::new();

impl CoverageMatrix {
    /// Return the immutable, process-wide [`CoverageMatrix`] backed by
    /// [`MATRIX_TOML`].
    ///
    /// The first call parses the embedded TOML text via [`parse`]; later
    /// calls return the cached reference. Because the matrix is a
    /// compile-time invariant of the binary, a parse failure here is a
    /// programming error rather than a runtime condition the caller can
    /// recover from — the loader therefore panics with the
    /// [`ParseError`]'s `Display` text. The first failed `get_loaded`
    /// call after a bad edit to `matrix.toml` will land at process start
    /// and surface the location-bearing TOML error.
    ///
    /// _Requirements: 6.1, 6.2_
    pub fn get_loaded() -> &'static CoverageMatrix {
        MATRIX.get_or_init(|| {
            parse(MATRIX_TOML.as_bytes()).unwrap_or_else(|e| {
                panic!("embedded coverage matrix.toml failed to parse: {e}")
            })
        })
    }

    /// Case-sensitive exact-match lookup against [`AlgorithmEntry::id`].
    /// Returns `None` for unknown ids; the caller decides whether to map
    /// that to a 4xx (sidecar handler) or a non-zero exit (`--filter`).
    ///
    /// _Requirements: 5.5, 6.1_
    #[must_use] 
    pub fn lookup(&self, id: &str) -> Option<&AlgorithmEntry> {
        self.algorithms.iter().find(|e| e.id == id)
    }

    /// Convenience: read the [`CoverageState`] for a single (algorithm,
    /// software) cell. `None` is returned only when the algorithm is
    /// unknown — every existing algorithm carries cells for all four
    /// reference softwares (enforced by [`parse`]).
    ///
    /// _Requirements: 6.2_
    #[must_use] 
    pub fn coverage(&self, id: &str, software: ReferenceSoftware) -> Option<CoverageState> {
        self.lookup(id)
            .and_then(|e| e.coverage.get(&software).copied())
    }

    /// Direct slice accessor in declared TOML order — callers that need to
    /// iterate every algorithm (CI consistency check, `report.json`
    /// emitter, snapshot `coverage.json`) use this instead of cloning the
    /// `Vec`.
    ///
    /// _Requirements: 6.1_
    #[must_use] 
    pub fn algorithms(&self) -> &[AlgorithmEntry] {
        &self.algorithms
    }

    /// Pinned Stats Code release version recorded with the matrix. Until
    /// task 1.3 wires `build.rs`, this is the placeholder
    /// `"0.0.0-build-injected"` shipped in the on-disk skeleton.
    ///
    /// _Requirements: 6.1_
    #[must_use] 
    pub fn release_version(&self) -> &str {
        &self.release_version
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn first_algorithm_id() -> &'static str {
        "tableone"
    }

    #[test]
    fn parse_embedded_matrix_succeeds() {
        let matrix = parse(MATRIX_TOML.as_bytes()).expect("embedded matrix must parse");
        assert_eq!(matrix.schema_version, 1);
        assert!(!matrix.algorithms.is_empty());
        // Every entry must carry all four cells.
        for entry in matrix.algorithms() {
            for software in REQUIRED_SOFTWARE {
                assert!(
                    entry.coverage.contains_key(&software),
                    "algorithm {:?} missing coverage cell for {:?}",
                    entry.id,
                    software
                );
                assert!(
                    entry.reference.contains_key(&software),
                    "algorithm {:?} missing reference cell for {:?}",
                    entry.id,
                    software
                );
            }
        }
    }

    #[test]
    fn get_loaded_returns_equivalent_data() {
        let parsed = parse(MATRIX_TOML.as_bytes()).expect("parse");
        let loaded = CoverageMatrix::get_loaded();
        assert_eq!(loaded.schema_version, parsed.schema_version);
        assert_eq!(loaded.release_version, parsed.release_version);
        assert_eq!(loaded.algorithms.len(), parsed.algorithms.len());
        // Same call returns the same `&'static` reference.
        let again = CoverageMatrix::get_loaded();
        assert!(std::ptr::eq(loaded, again));
    }

    #[test]
    fn lookup_hit_and_miss() {
        let matrix = CoverageMatrix::get_loaded();
        let entry = matrix
            .lookup(first_algorithm_id())
            .expect("tableone must be present");
        assert_eq!(entry.id, first_algorithm_id());
        assert!(matrix.lookup("does_not_exist").is_none());
        // Case-sensitive: `"TableOne"` must miss even though `"tableone"` hits.
        assert!(matrix.lookup("TableOne").is_none());
    }

    #[test]
    fn coverage_returns_expected_states() {
        let matrix = CoverageMatrix::get_loaded();
        assert_eq!(
            matrix.coverage("tableone", ReferenceSoftware::R),
            Some(CoverageState::Live)
        );
        assert_eq!(
            matrix.coverage("tableone", ReferenceSoftware::SAS),
            Some(CoverageState::Recorded)
        );
        assert_eq!(
            matrix.coverage("standardization", ReferenceSoftware::SPSS),
            Some(CoverageState::None_)
        );
        assert_eq!(
            matrix.coverage("standardization", ReferenceSoftware::R),
            Some(CoverageState::SidecarOnly)
        );
        assert_eq!(matrix.coverage("does_not_exist", ReferenceSoftware::R), None);
    }

    #[test]
    fn release_version_returns_embedded_literal() {
        let matrix = CoverageMatrix::get_loaded();
        // Task 1.3 wires `build.rs` to replace the on-disk
        // `"0.0.0-build-injected"` placeholder with the live
        // `CARGO_PKG_VERSION` before `coverage_matrix/mod.rs` includes the
        // file. The injection is locked to the package version so a wrong
        // build.rs (placeholder unchanged, OUT_DIR copy missing) fails this
        // assertion immediately.
        assert_eq!(matrix.release_version(), env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn algorithms_preserves_declared_order() {
        let matrix = CoverageMatrix::get_loaded();
        // matrix.toml lists tableone first, ttest second, anova third.
        let ids: Vec<&str> = matrix
            .algorithms()
            .iter()
            .map(|e| e.id.as_str())
            .take(3)
            .collect();
        assert_eq!(ids, vec!["tableone", "ttest", "anova"]);
    }

    #[test]
    fn malformed_toml_returns_toml_variant() {
        // A stray opening bracket makes the input syntactically invalid.
        let bad = b"schema_version = 1\nrelease_version = \"0.5.0\"\n[[algorithm\n";
        let err = parse(bad).expect_err("malformed TOML must error");
        match err {
            ParseError::Toml(_) => {}
            other => panic!("expected ParseError::Toml, got {other:?}"),
        }
    }

    #[test]
    fn duplicate_algorithm_id_is_rejected() {
        // Two `[[algorithm]]` blocks with the same `id`. Both blocks
        // satisfy serde's schema, so the duplicate-id check must fire
        // from our hand-rolled validation.
        let dup = r#"
schema_version = 1
release_version = "0.5.0"

[[algorithm]]
id = "demo"
display_name = "Demo"
iterative = false
[algorithm.coverage]
R = "live"
SAS = "live"
Python = "live"
SPSS = "live"
[algorithm.reference]
R      = { fn = "f", pkg = "p", version = "1" }
SAS    = { proc = "P", version = "1" }
Python = { fn = "f", pkg = "p", version = "1" }
SPSS   = { proc = "P", version = "1" }

[[algorithm]]
id = "demo"
display_name = "Demo Two"
iterative = false
[algorithm.coverage]
R = "live"
SAS = "live"
Python = "live"
SPSS = "live"
[algorithm.reference]
R      = { fn = "f", pkg = "p", version = "1" }
SAS    = { proc = "P", version = "1" }
Python = { fn = "f", pkg = "p", version = "1" }
SPSS   = { proc = "P", version = "1" }
"#;
        let err = parse(dup.as_bytes()).expect_err("duplicate id must error");
        match err {
            ParseError::DuplicateAlgorithmId { id } => assert_eq!(id, "demo"),
            other => panic!("expected ParseError::DuplicateAlgorithmId, got {other:?}"),
        }
    }

    #[test]
    fn missing_software_cell_is_rejected() {
        // Coverage map only declares R / SAS / Python — SPSS missing.
        let incomplete = r#"
schema_version = 1
release_version = "0.5.0"

[[algorithm]]
id = "demo"
display_name = "Demo"
iterative = false
[algorithm.coverage]
R = "live"
SAS = "live"
Python = "live"
[algorithm.reference]
R      = { fn = "f", pkg = "p", version = "1" }
SAS    = { proc = "P", version = "1" }
Python = { fn = "f", pkg = "p", version = "1" }
SPSS   = { proc = "P", version = "1" }
"#;
        let err = parse(incomplete.as_bytes()).expect_err("incomplete coverage must error");
        match err {
            ParseError::IncompleteCoverage {
                entry,
                missing_software,
            } => {
                assert_eq!(entry, "demo");
                assert_eq!(missing_software, "SPSS");
            }
            other => panic!("expected ParseError::IncompleteCoverage, got {other:?}"),
        }
    }

    #[test]
    fn missing_reference_cell_is_rejected() {
        // Reference map missing SPSS.
        let incomplete = r#"
schema_version = 1
release_version = "0.5.0"

[[algorithm]]
id = "demo"
display_name = "Demo"
iterative = false
[algorithm.coverage]
R = "live"
SAS = "live"
Python = "live"
SPSS = "live"
[algorithm.reference]
R      = { fn = "f", pkg = "p", version = "1" }
SAS    = { proc = "P", version = "1" }
Python = { fn = "f", pkg = "p", version = "1" }
"#;
        let err = parse(incomplete.as_bytes()).expect_err("incomplete reference must error");
        match err {
            ParseError::IncompleteCoverage {
                entry,
                missing_software,
            } => {
                assert_eq!(entry, "demo");
                assert_eq!(missing_software, "SPSS");
            }
            other => panic!("expected ParseError::IncompleteCoverage, got {other:?}"),
        }
    }

    #[test]
    fn unknown_coverage_token_surfaces_as_toml_error() {
        // serde rejects unknown tokens with its own error; our
        // `UnknownEnum` variant is reserved for future hand-rolled
        // strict-mode checks.
        let bad_token = r#"
schema_version = 1
release_version = "0.5.0"

[[algorithm]]
id = "demo"
display_name = "Demo"
iterative = false
[algorithm.coverage]
R = "MAYBE"
SAS = "live"
Python = "live"
SPSS = "live"
[algorithm.reference]
R      = { fn = "f", pkg = "p", version = "1" }
SAS    = { proc = "P", version = "1" }
Python = { fn = "f", pkg = "p", version = "1" }
SPSS   = { proc = "P", version = "1" }
"#;
        let err = parse(bad_token.as_bytes()).expect_err("unknown enum token must error");
        match err {
            ParseError::Toml(_) => {}
            other => panic!("expected ParseError::Toml for unknown enum, got {other:?}"),
        }
    }

    #[test]
    fn parse_error_variants_render_actionable_messages() {
        // Sanity-check the Display impls for the hand-rolled variants so
        // the panic message in `get_loaded` is operator-friendly.
        let m = ParseError::MissingField {
            entry: "demo".into(),
            field: "id",
        };
        assert!(format!("{m}").contains("demo"));
        assert!(format!("{m}").contains("id"));

        let u = ParseError::UnknownEnum {
            entry: "demo".into(),
            field: "coverage",
            value: "MAYBE".into(),
        };
        assert!(format!("{u}").contains("MAYBE"));

        let d = ParseError::DuplicateCell {
            entry: "demo".into(),
            software: "R".into(),
        };
        assert!(format!("{d}").contains("R"));

        let dup = ParseError::DuplicateAlgorithmId { id: "demo".into() };
        assert!(format!("{dup}").contains("demo"));

        let inc = ParseError::IncompleteCoverage {
            entry: "demo".into(),
            missing_software: "SPSS".into(),
        };
        assert!(format!("{inc}").contains("SPSS"));
    }
}
