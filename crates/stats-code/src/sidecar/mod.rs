//! Sidecar Code Generator (Feature: parity-and-multilang-sidecar).
//!
//! This module pulls together the wave-1 nuts and bolts produced by
//! sibling tasks into a single deterministic entry point,
//! [`generate_snippet`]:
//!
//! - `header.rs`     — task 2.2 ([`header::format_header`]).
//! - `render.rs`     — task 2.3 ([`render::render_pure`] template engine).
//! - `redact.rs`     — task 2.4 ([`redact::redact_pure`] secret + path policy).
//! - `crate::spawn_policy` — task 2.6 (`forbid_external_runtimes_scope`).
//! - `crate::coverage_matrix` — tasks 1.1 / 1.2 (single source of truth).
//!
//! Every (algorithm × software) cell whose `coverage` value
//! ∈ `{live, recorded, sidecar_only}` is paired at compile time with an
//! embedded template under `templates/<software_lower>/<id>.tmpl.txt`;
//! `coverage = "none"` cells short-circuit to a structured
//! [`SidecarSnippet::Uncovered`] sentinel without rendering a body, a
//! comment, or any placeholder text (Requirement 2.4).
//!
//! Compile-time presence of `(algorithm × software) → template` mappings is
//! enforced by `crates/stats-code/build.rs::check_sidecar_templates`,
//! which parses the same matrix this module loads at runtime. The
//! [`load_template`] helper below mirrors that mapping with explicit
//! `include_str!` arms — one per non-`none` cell — so the contents of
//! every template ride into the binary alongside the matrix bytes.
//!
//! _Requirements: 2.1, 2.2, 2.4, 2.5, 2.6, 6.2, 10.1, 10.5._

pub mod header;
pub mod redact;
pub mod render;

use std::path::Path;

use thiserror::Error;

use crate::coverage_matrix::{
    CoverageMatrix, CoverageState, ReferenceSoftware,
};
use crate::redact::{redact_pure, RedactionPolicy};
use crate::spawn_policy::{forbid_external_runtimes_scope, SpawnError};

pub use self::header::{format_header, Column, ColumnDtype};
pub use self::render::{render_pure, RenderError, RenderParams};

/// One emitted Sidecar snippet, either fully rendered or a structured
/// "uncovered" sentinel.
///
/// The two variants encode the closed-set behavior of Requirement 2.4 /
/// Requirement 6.2:
///
/// * [`Self::Snippet`] — produced when the matrix value for the cell is
///   `live`, `recorded`, or `sidecar_only`. Carries the redacted UTF-8
///   text (LF line endings, no CR), the dataset SHA256 used to hash the
///   header, and the Stats Code release version recorded with the matrix.
/// * [`Self::Uncovered`] — produced when the matrix value for the cell is
///   `none`. Carries no body, no comment, no placeholder text — only the
///   identifying tuple plus the literal coverage value `"none"`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SidecarSnippet {
    /// Fully rendered snippet.
    Snippet {
        /// Reference Software for which this snippet was generated.
        software: ReferenceSoftware,
        /// Stable case-sensitive algorithm identifier (matches matrix key).
        algorithm_id: String,
        /// UTF-8 snippet bytes with LF line endings (no CR).
        text: String,
        /// 64-hex lowercase SHA256 of the input dataset.
        sha256_of_dataset: String,
        /// Stats Code release version recorded with the matrix.
        release_version: String,
    },
    /// Structured "uncovered" sentinel — Requirement 2.4.
    Uncovered {
        /// Stable case-sensitive algorithm identifier.
        algorithm_id: String,
        /// Reference Software for which no snippet exists.
        software: ReferenceSoftware,
        /// Always exactly the literal `"none"`. Carried as a string (and
        /// not derived from a `CoverageState` by serde) so the wire shape
        /// stays trivially explainable to the SPA's TypeScript consumer.
        coverage_value: String,
    },
}

/// Errors returned by [`generate_snippet`].
///
/// The variants are aligned with the four reasons the generator can
/// refuse a request:
///
/// * [`Self::UnknownAlgorithm`] — the algorithm id is not present in the
///   embedded coverage matrix.
/// * [`Self::MissingTemplate`] — the matrix cell requires a template but
///   [`load_template`] returned `None`. By construction this is impossible
///   when `build.rs::check_sidecar_templates` runs (it asserts presence
///   for every non-`none` cell), but we surface the variant defensively
///   so a hand-edited matrix without a matching `match` arm fails loudly.
/// * [`Self::Render`] — propagated from [`render_pure`].
/// * [`Self::ForbiddenSpawn`] — propagated from
///   [`forbid_external_runtimes_scope`] when a guarded path attempts to
///   spawn an external statistical runtime (Requirement 10.1, 10.5).
#[derive(Debug, Error)]
pub enum GenerateError {
    /// The algorithm id is not present in the embedded coverage matrix.
    #[error("unknown algorithm: {algorithm_id}")]
    UnknownAlgorithm { algorithm_id: String },

    /// The cell requires a template but no embedded mapping exists.
    #[error("missing template for ({algorithm_id}, {software:?})")]
    MissingTemplate {
        algorithm_id: String,
        software: ReferenceSoftware,
    },

    /// Template engine surfaced an error during rendering.
    #[error("render error: {0}")]
    Render(#[from] RenderError),

    /// Spawn policy rejected an external statistical runtime invocation.
    #[error("forbidden spawn: {0}")]
    ForbiddenSpawn(#[from] SpawnError),
}

/// Render a Sidecar snippet for the given (algorithm × software) cell.
///
/// # Algorithm
///
/// 1. Wrap the entire body in [`forbid_external_runtimes_scope`] so any
///    accidental `Command::spawn` of `{R, Rscript, python, python3,
///    pythonw, sas, spss, pspp, pspp-cli, statistics, stats}` would abort
///    the call (Requirement 10.1, 10.5). Wave-1 templating performs no
///    spawns; the scope guard documents the contract and is the entry
///    point future spawns would be checked through.
/// 2. Look the algorithm up in [`CoverageMatrix::get_loaded`]; on miss
///    return [`GenerateError::UnknownAlgorithm`].
/// 3. Read the coverage cell. If `None_`, return
///    [`SidecarSnippet::Uncovered`] with `coverage_value = "none"`,
///    carrying no body / no comment / no placeholder text
///    (Requirement 2.4).
/// 4. Otherwise resolve the `(algorithm_id, software) → &'static str`
///    template mapping via [`load_template`]. Render through
///    [`render_pure`].
/// 5. Apply [`redact_pure`] with a [`RedactionPolicy`] built from the
///    caller-supplied `api_keys` and (optional) `working_directory`
///    (Requirements 2.6, 9.1, 9.3, 9.4, 9.5).
/// 6. Prepend the [`format_header`] header. The header itself ends with
///    a single LF, so concatenating header + body yields LF-only output
///    when the body is LF-only (Requirement 2.1).
///
/// # Errors
///
/// Returns a [`GenerateError`] from steps 2 / 4 / 4 / 1 respectively.
///
/// _Requirements: 2.1, 2.2, 2.4, 2.5, 2.6, 10.1, 10.5._
pub fn generate_snippet(
    algorithm_id: &str,
    params: &RenderParams,
    columns: &[Column],
    dataset_sha256: &str,
    software: ReferenceSoftware,
    api_keys: &[&str],
    working_directory: Option<&Path>,
) -> Result<SidecarSnippet, GenerateError> {
    // The scope helper's closure must return `Result<T, SpawnError>`. We
    // therefore have the closure return `Result<Result<…, GenerateError>,
    // SpawnError>` and unwrap one layer outside the scope. This lets
    // future code paths inside the closure abort with a `SpawnError` (via
    // `?`) while still letting the outer function carry richer
    // `GenerateError` variants.
    let outcome: Result<Result<SidecarSnippet, GenerateError>, SpawnError> =
        forbid_external_runtimes_scope(|_policy| {
            Ok(generate_snippet_inner(
                algorithm_id,
                params,
                columns,
                dataset_sha256,
                software,
                api_keys,
                working_directory,
            ))
        });

    match outcome {
        Ok(inner) => inner,
        Err(spawn) => Err(GenerateError::ForbiddenSpawn(spawn)),
    }
}

/// Pure body of [`generate_snippet`], factored out so the surrounding
/// `forbid_external_runtimes_scope` closure stays a one-liner.
fn generate_snippet_inner(
    algorithm_id: &str,
    params: &RenderParams,
    columns: &[Column],
    dataset_sha256: &str,
    software: ReferenceSoftware,
    api_keys: &[&str],
    working_directory: Option<&Path>,
) -> Result<SidecarSnippet, GenerateError> {
    let matrix = CoverageMatrix::get_loaded();

    // Step 2: algorithm lookup.
    let entry = matrix
        .lookup(algorithm_id)
        .ok_or_else(|| GenerateError::UnknownAlgorithm {
            algorithm_id: algorithm_id.to_string(),
        })?;

    // Step 3: read the coverage cell. Every (algorithm, software) cell is
    // present by `coverage_matrix::parse` invariants — the `expect` is a
    // documented impossibility.
    let coverage = entry
        .coverage
        .get(&software)
        .copied()
        .expect("coverage_matrix::parse guarantees every (algorithm, software) cell exists");

    if matches!(coverage, CoverageState::None_) {
        return Ok(SidecarSnippet::Uncovered {
            algorithm_id: algorithm_id.to_string(),
            software,
            coverage_value: "none".to_string(),
        });
    }

    // Step 4: resolve the template and render.
    let template = load_template(algorithm_id, software).ok_or_else(|| {
        GenerateError::MissingTemplate {
            algorithm_id: algorithm_id.to_string(),
            software,
        }
    })?;

    let release_version = matrix.release_version();
    let body = render_pure(template, params, columns, dataset_sha256, release_version)?;

    // Step 5: redaction. Build the policy fresh per call so the
    // generator stays referentially transparent across invocations.
    let mut policy = RedactionPolicy::new().with_secrets(api_keys);
    if let Some(wd) = working_directory {
        policy = policy.with_working_directory(wd.to_path_buf());
    }
    let redacted_body = redact_pure(&body, &policy);

    // Step 6: header + body. `format_header` always ends with a single
    // LF, so concatenating header + body keeps LF-only output when the
    // body is LF-only. We do not insert an extra blank line between
    // header and body — the header itself terminates with `\n`, and
    // wave-1 templates may legitimately be empty (placeholder files), in
    // which case the resulting text is exactly the header.
    let header = format_header(columns, dataset_sha256, release_version);
    let mut text = String::with_capacity(header.len() + redacted_body.len());
    text.push_str(&header);
    text.push_str(&redacted_body);

    // Defense in depth: render and redact emit no CR; the header builder
    // emits no CR; templates on disk are LF-only. If any of those
    // invariants slip a regression in `cargo test` should catch it
    // before a release.
    debug_assert!(
        !text.contains('\r'),
        "generated snippet contains CR (LF-only invariant violated)"
    );

    Ok(SidecarSnippet::Snippet {
        software,
        algorithm_id: algorithm_id.to_string(),
        text,
        sha256_of_dataset: dataset_sha256.to_string(),
        release_version: release_version.to_string(),
    })
}

/// Resolve `(algorithm_id, software)` to an embedded template.
///
/// Returns `Some(&'static str)` for every (algorithm × software) cell
/// whose matrix coverage value ∈ `{live, recorded, sidecar_only}`, and
/// `None` for every cell whose coverage value is `none`. The
/// `build.rs::check_sidecar_templates` invariant guarantees the latter
/// case is unreachable for cells that genuinely require a snippet —
/// callers see [`GenerateError::MissingTemplate`] only when a future
/// matrix edit reaches the runtime ahead of a matching arm here.
///
/// Layout note: arms are grouped by algorithm (reflecting the matrix's
/// natural reading order) and ordered `R → SAS → Python → SPSS` within
/// each algorithm so a future audit can scan diff vs. matrix.toml in one
/// pass.
fn load_template(
    algorithm_id: &str,
    software: ReferenceSoftware,
) -> Option<&'static str> {
    use ReferenceSoftware as RS;
    match (algorithm_id, software) {
        // tableone: R live, SAS recorded, Python live, SPSS recorded
        ("tableone", RS::R) => Some(include_str!("templates/r/tableone.tmpl.txt")),
        ("tableone", RS::SAS) => Some(include_str!("templates/sas/tableone.tmpl.txt")),
        ("tableone", RS::Python) => Some(include_str!("templates/python/tableone.tmpl.txt")),
        ("tableone", RS::SPSS) => Some(include_str!("templates/spss/tableone.tmpl.txt")),

        // ttest: all four covered
        ("ttest", RS::R) => Some(include_str!("templates/r/ttest.tmpl.txt")),
        ("ttest", RS::SAS) => Some(include_str!("templates/sas/ttest.tmpl.txt")),
        ("ttest", RS::Python) => Some(include_str!("templates/python/ttest.tmpl.txt")),
        ("ttest", RS::SPSS) => Some(include_str!("templates/spss/ttest.tmpl.txt")),

        // anova: all four covered
        ("anova", RS::R) => Some(include_str!("templates/r/anova.tmpl.txt")),
        ("anova", RS::SAS) => Some(include_str!("templates/sas/anova.tmpl.txt")),
        ("anova", RS::Python) => Some(include_str!("templates/python/anova.tmpl.txt")),
        ("anova", RS::SPSS) => Some(include_str!("templates/spss/anova.tmpl.txt")),

        // nonparametric: all four covered
        ("nonparametric", RS::R) => Some(include_str!("templates/r/nonparametric.tmpl.txt")),
        ("nonparametric", RS::SAS) => Some(include_str!("templates/sas/nonparametric.tmpl.txt")),
        ("nonparametric", RS::Python) => {
            Some(include_str!("templates/python/nonparametric.tmpl.txt"))
        }
        ("nonparametric", RS::SPSS) => Some(include_str!("templates/spss/nonparametric.tmpl.txt")),

        // correlation: all four covered
        ("correlation", RS::R) => Some(include_str!("templates/r/correlation.tmpl.txt")),
        ("correlation", RS::SAS) => Some(include_str!("templates/sas/correlation.tmpl.txt")),
        ("correlation", RS::Python) => {
            Some(include_str!("templates/python/correlation.tmpl.txt"))
        }
        ("correlation", RS::SPSS) => Some(include_str!("templates/spss/correlation.tmpl.txt")),

        // standardization: R sidecar_only, SAS recorded, Python sidecar_only, SPSS none
        ("standardization", RS::R) => {
            Some(include_str!("templates/r/standardization.tmpl.txt"))
        }
        ("standardization", RS::SAS) => {
            Some(include_str!("templates/sas/standardization.tmpl.txt"))
        }
        ("standardization", RS::Python) => {
            Some(include_str!("templates/python/standardization.tmpl.txt"))
        }
        // ("standardization", RS::SPSS) — coverage = none, no template

        // or_rr: all four covered
        ("or_rr", RS::R) => Some(include_str!("templates/r/or_rr.tmpl.txt")),
        ("or_rr", RS::SAS) => Some(include_str!("templates/sas/or_rr.tmpl.txt")),
        ("or_rr", RS::Python) => Some(include_str!("templates/python/or_rr.tmpl.txt")),
        ("or_rr", RS::SPSS) => Some(include_str!("templates/spss/or_rr.tmpl.txt")),

        // attributable_risk: R sidecar_only, SAS recorded, Python sidecar_only, SPSS none
        ("attributable_risk", RS::R) => {
            Some(include_str!("templates/r/attributable_risk.tmpl.txt"))
        }
        ("attributable_risk", RS::SAS) => {
            Some(include_str!("templates/sas/attributable_risk.tmpl.txt"))
        }
        ("attributable_risk", RS::Python) => {
            Some(include_str!("templates/python/attributable_risk.tmpl.txt"))
        }
        // ("attributable_risk", RS::SPSS) — coverage = none, no template

        // kaplan_meier: all four covered
        ("kaplan_meier", RS::R) => Some(include_str!("templates/r/kaplan_meier.tmpl.txt")),
        ("kaplan_meier", RS::SAS) => Some(include_str!("templates/sas/kaplan_meier.tmpl.txt")),
        ("kaplan_meier", RS::Python) => {
            Some(include_str!("templates/python/kaplan_meier.tmpl.txt"))
        }
        ("kaplan_meier", RS::SPSS) => {
            Some(include_str!("templates/spss/kaplan_meier.tmpl.txt"))
        }

        // cox: all four covered
        ("cox", RS::R) => Some(include_str!("templates/r/cox.tmpl.txt")),
        ("cox", RS::SAS) => Some(include_str!("templates/sas/cox.tmpl.txt")),
        ("cox", RS::Python) => Some(include_str!("templates/python/cox.tmpl.txt")),
        ("cox", RS::SPSS) => Some(include_str!("templates/spss/cox.tmpl.txt")),

        // life_table: R sidecar_only, SAS recorded, Python sidecar_only, SPSS recorded
        ("life_table", RS::R) => Some(include_str!("templates/r/life_table.tmpl.txt")),
        ("life_table", RS::SAS) => Some(include_str!("templates/sas/life_table.tmpl.txt")),
        ("life_table", RS::Python) => {
            Some(include_str!("templates/python/life_table.tmpl.txt"))
        }
        ("life_table", RS::SPSS) => Some(include_str!("templates/spss/life_table.tmpl.txt")),

        // linear: all four covered
        ("linear", RS::R) => Some(include_str!("templates/r/linear.tmpl.txt")),
        ("linear", RS::SAS) => Some(include_str!("templates/sas/linear.tmpl.txt")),
        ("linear", RS::Python) => Some(include_str!("templates/python/linear.tmpl.txt")),
        ("linear", RS::SPSS) => Some(include_str!("templates/spss/linear.tmpl.txt")),

        // logistic: all four covered
        ("logistic", RS::R) => Some(include_str!("templates/r/logistic.tmpl.txt")),
        ("logistic", RS::SAS) => Some(include_str!("templates/sas/logistic.tmpl.txt")),
        ("logistic", RS::Python) => Some(include_str!("templates/python/logistic.tmpl.txt")),
        ("logistic", RS::SPSS) => Some(include_str!("templates/spss/logistic.tmpl.txt")),

        // power_single_arm: R live, SAS recorded, Python live, SPSS none
        ("power_single_arm", RS::R) => {
            Some(include_str!("templates/r/power_single_arm.tmpl.txt"))
        }
        ("power_single_arm", RS::SAS) => {
            Some(include_str!("templates/sas/power_single_arm.tmpl.txt"))
        }
        ("power_single_arm", RS::Python) => {
            Some(include_str!("templates/python/power_single_arm.tmpl.txt"))
        }
        // ("power_single_arm", RS::SPSS) — coverage = none, no template

        // power_phase2: R sidecar_only, SAS recorded, Python sidecar_only, SPSS none
        ("power_phase2", RS::R) => Some(include_str!("templates/r/power_phase2.tmpl.txt")),
        ("power_phase2", RS::SAS) => Some(include_str!("templates/sas/power_phase2.tmpl.txt")),
        ("power_phase2", RS::Python) => {
            Some(include_str!("templates/python/power_phase2.tmpl.txt"))
        }
        // ("power_phase2", RS::SPSS) — coverage = none, no template

        // power_phase3: R live, SAS recorded, Python live, SPSS none
        ("power_phase3", RS::R) => Some(include_str!("templates/r/power_phase3.tmpl.txt")),
        ("power_phase3", RS::SAS) => Some(include_str!("templates/sas/power_phase3.tmpl.txt")),
        ("power_phase3", RS::Python) => {
            Some(include_str!("templates/python/power_phase3.tmpl.txt"))
        }
        // ("power_phase3", RS::SPSS) — coverage = none, no template

        // diagnostic_roc: all four covered
        ("diagnostic_roc", RS::R) => Some(include_str!("templates/r/diagnostic_roc.tmpl.txt")),
        ("diagnostic_roc", RS::SAS) => {
            Some(include_str!("templates/sas/diagnostic_roc.tmpl.txt"))
        }
        ("diagnostic_roc", RS::Python) => {
            Some(include_str!("templates/python/diagnostic_roc.tmpl.txt"))
        }
        ("diagnostic_roc", RS::SPSS) => {
            Some(include_str!("templates/spss/diagnostic_roc.tmpl.txt"))
        }

        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Canonical 64-hex SHA256 fixture used across every test that does
    /// not specifically exercise SHA256 invariants.
    const SHA256: &str =
        "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    fn cols() -> Vec<Column> {
        vec![
            Column {
                name: "age".into(),
                dtype: ColumnDtype::Numeric,
            },
            Column {
                name: "sex".into(),
                dtype: ColumnDtype::Categorical,
            },
        ]
    }

    fn empty_params() -> RenderParams {
        RenderParams::new()
    }

    #[test]
    fn uncovered_cell_returns_structured_sentinel() {
        // standardization × SPSS is `none` in matrix.toml.
        let snippet = generate_snippet(
            "standardization",
            &empty_params(),
            &cols(),
            SHA256,
            ReferenceSoftware::SPSS,
            &[],
            None,
        )
        .expect("generation must succeed for an `none` cell");

        match snippet {
            SidecarSnippet::Uncovered {
                algorithm_id,
                software,
                coverage_value,
            } => {
                assert_eq!(algorithm_id, "standardization");
                assert_eq!(software, ReferenceSoftware::SPSS);
                assert_eq!(coverage_value, "none");
            }
            other => panic!("expected Uncovered sentinel, got {other:?}"),
        }
    }

    #[test]
    fn covered_cell_with_empty_template_returns_header_only_snippet() {
        // Wave-1 templates are empty placeholders, so the rendered body
        // is empty and the resulting `text` equals the header alone —
        // which is still a non-empty, LF-terminated UTF-8 string.
        let snippet = generate_snippet(
            "tableone",
            &empty_params(),
            &cols(),
            SHA256,
            ReferenceSoftware::R,
            &[],
            None,
        )
        .expect("generation must succeed for a covered cell");

        match snippet {
            SidecarSnippet::Snippet {
                software,
                algorithm_id,
                text,
                sha256_of_dataset,
                release_version,
            } => {
                assert_eq!(software, ReferenceSoftware::R);
                assert_eq!(algorithm_id, "tableone");
                assert_eq!(sha256_of_dataset, SHA256);
                assert_eq!(release_version, env!("CARGO_PKG_VERSION"));

                // Header invariants surface through the snippet text.
                assert!(!text.is_empty(), "snippet text must be non-empty");
                assert!(text.contains("data.csv"), "header must mention data.csv");
                assert!(text.contains(SHA256), "header must carry the dataset sha");
                assert!(
                    text.contains(env!("CARGO_PKG_VERSION")),
                    "header must carry the release version",
                );
                assert!(text.contains("# column.0.name: age"));
                assert!(text.contains("# column.1.dtype: categorical"));
                assert!(!text.contains('\r'), "snippet must be LF-only");
                assert!(text.ends_with('\n'), "snippet must end with LF");
            }
            other => panic!("expected Snippet variant, got {other:?}"),
        }
    }

    #[test]
    fn generation_is_byte_for_byte_deterministic() {
        let a = generate_snippet(
            "tableone",
            &empty_params(),
            &cols(),
            SHA256,
            ReferenceSoftware::Python,
            &[],
            None,
        )
        .unwrap();
        let b = generate_snippet(
            "tableone",
            &empty_params(),
            &cols(),
            SHA256,
            ReferenceSoftware::Python,
            &[],
            None,
        )
        .unwrap();
        assert_eq!(a, b, "two calls with identical inputs must agree byte-for-byte");
    }

    #[test]
    fn unknown_algorithm_returns_structured_error() {
        let err = generate_snippet(
            "no_such_algorithm",
            &empty_params(),
            &cols(),
            SHA256,
            ReferenceSoftware::R,
            &[],
            None,
        )
        .expect_err("unknown algorithm must error");

        match err {
            GenerateError::UnknownAlgorithm { algorithm_id } => {
                assert_eq!(algorithm_id, "no_such_algorithm");
            }
            other => panic!("expected UnknownAlgorithm, got {other:?}"),
        }
    }

    #[test]
    fn case_sensitive_algorithm_lookup_rejects_wrong_casing() {
        // "TableOne" must miss even though "tableone" hits — Requirement 5.5.
        let err = generate_snippet(
            "TableOne",
            &empty_params(),
            &cols(),
            SHA256,
            ReferenceSoftware::R,
            &[],
            None,
        )
        .expect_err("case-different id must miss the matrix lookup");
        assert!(matches!(err, GenerateError::UnknownAlgorithm { .. }));
    }

    #[test]
    fn every_covered_cell_is_renderable_and_every_none_cell_is_uncovered() {
        // Pure consistency walk: for every (algorithm × software) cell in
        // the loaded matrix, generate a snippet with the empty template
        // and assert the variant matches the matrix value. This locks
        // the `load_template` arm coverage against future matrix edits.
        let matrix = CoverageMatrix::get_loaded();
        let softwares = [
            ReferenceSoftware::R,
            ReferenceSoftware::SAS,
            ReferenceSoftware::Python,
            ReferenceSoftware::SPSS,
        ];
        for entry in matrix.algorithms() {
            for sw in softwares {
                let cov = matrix
                    .coverage(&entry.id, sw)
                    .expect("matrix invariant: every cell exists");
                let snippet = generate_snippet(
                    &entry.id,
                    &empty_params(),
                    &cols(),
                    SHA256,
                    sw,
                    &[],
                    None,
                )
                .unwrap_or_else(|e| {
                    panic!("({}, {sw:?}) failed to generate: {e}", entry.id)
                });
                match (cov, snippet) {
                    (CoverageState::None_, SidecarSnippet::Uncovered { coverage_value, .. }) => {
                        assert_eq!(coverage_value, "none");
                    }
                    (CoverageState::None_, other) => {
                        panic!("({}, {sw:?}) is none but produced {other:?}", entry.id);
                    }
                    (
                        CoverageState::Live
                        | CoverageState::Recorded
                        | CoverageState::SidecarOnly,
                        SidecarSnippet::Snippet { text, .. },
                    ) => {
                        assert!(
                            !text.is_empty(),
                            "({}, {sw:?}) covered cell produced empty text",
                            entry.id,
                        );
                    }
                    (cov, other) => {
                        panic!(
                            "({}, {sw:?}) coverage {cov:?} produced unexpected {other:?}",
                            entry.id,
                        );
                    }
                }
            }
        }
    }
}

/// Syntactic-shape unit tests for every (algorithm × software) template
/// (task 3.5).
///
/// For each covered cell — i.e. every `(algorithm_id, software)` whose
/// matrix coverage value is **not** `none` — we render a snippet with a
/// fixed deterministic input and assert the rendered `Snippet.text`
/// embeds the contractually required tokens:
///
/// 1. the literal `data.csv`,
/// 2. the supplied 64-hex lowercase `dataset_sha256`,
/// 3. every input column name, and
/// 4. the matrix-recorded primary identifier for that cell (the callable
///    for R / Python, the `PROC` / command for SAS / SPSS), plus the
///    recorded package where one is applicable.
///
/// No R / SAS / Python / SPSS runtime is ever spawned — these are pure
/// static string assertions over the generated text. The walk is driven
/// off [`CoverageMatrix::get_loaded`], so adding a new algorithm to the
/// matrix automatically extends this coverage with no edits here.
///
/// _Requirements: 2.2, 2.5_
#[cfg(test)]
mod template_shape_tests {
    use super::*;
    use crate::coverage_matrix::ReferenceImpl;

    /// Canonical 64-hex lowercase SHA256 fixture (deterministic input).
    const SHA256: &str =
        "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    /// All four reference softwares, canonical order.
    const SOFTWARES: [ReferenceSoftware; 4] = [
        ReferenceSoftware::R,
        ReferenceSoftware::SAS,
        ReferenceSoftware::Python,
        ReferenceSoftware::SPSS,
    ];

    /// Fixed deterministic column set: two columns named `outcome` and
    /// `group`. Every wave-1 template references at most
    /// `{{column.0.…}}` and `{{column.1.…}}`, so two columns are
    /// sufficient for all of them to render without a column-index error.
    fn fixed_columns() -> Vec<Column> {
        vec![
            Column {
                name: "outcome".into(),
                dtype: ColumnDtype::Numeric,
            },
            Column {
                name: "group".into(),
                dtype: ColumnDtype::Categorical,
            },
        ]
    }

    /// Case-insensitive substring test. SAS / SPSS templates spell their
    /// procedures in a different case than the uppercase matrix tokens
    /// (e.g. matrix `PROC FREQ` vs template `proc freq`), so identifier
    /// matching is case-folded.
    fn contains_ci(haystack: &str, needle: &str) -> bool {
        haystack
            .to_ascii_lowercase()
            .contains(&needle.to_ascii_lowercase())
    }

    /// Primary identifier token(s) that MUST appear in the rendered body
    /// for a cell, derived from its [`ReferenceImpl`].
    ///
    /// The matrix records a *decorated* identifier (fully-qualified dotted
    /// path, `PROC X /OPTION`, multi-statement `A;B`) that the template
    /// abbreviates. We therefore extract the stable primary token(s):
    ///
    /// * **R** — the callable is written `pkg::fn` and appears verbatim,
    ///   so the whole callable is the token.
    /// * **Python** — the template imports/uses the *leaf* symbol; the
    ///   fully-qualified path is abbreviated (e.g. `scipy.stats.ttest_ind`
    ///   → `stats.ttest_ind`). Three cells record `manual` (a hand-rolled
    ///   numpy/scipy implementation with no library callable); for those
    ///   the callable cannot appear, so we fall back to the recorded
    ///   package, which the template always imports.
    /// * **SAS** — `PROC <NAME>` for each `;`-separated procedure. Trailing
    ///   options (`/CMH`, `METHOD=LIFE`) are dropped because the template
    ///   spells them out separately from the `proc <name>` header.
    /// * **SPSS** — the command name up to the first `/option`, for each
    ///   `;`-separated command (e.g. `CROSSTABS /STATISTICS=RISK CMH` →
    ///   `CROSSTABS`).
    fn required_identifier_tokens(
        reference: &ReferenceImpl,
        software: ReferenceSoftware,
    ) -> Vec<String> {
        match software {
            ReferenceSoftware::R => {
                vec![reference
                    .callable
                    .clone()
                    .expect("R cell records a callable")]
            }
            ReferenceSoftware::Python => {
                let callable = reference.callable.as_deref().unwrap_or("");
                if callable.is_empty() || callable == "manual" || callable == "n/a" {
                    // Manual implementation: no library callable to assert.
                    // The package (numpy / scipy) is always imported, so it
                    // is the strongest token available for this cell.
                    vec![reference
                        .package
                        .clone()
                        .expect("manual Python cell records a package")]
                } else {
                    let leaf = callable.rsplit('.').next().unwrap_or(callable);
                    vec![leaf.to_string()]
                }
            }
            ReferenceSoftware::SAS => {
                let proc = reference.proc.as_deref().expect("SAS cell records a proc");
                proc.split(';')
                    .map(str::trim)
                    .filter(|piece| !piece.is_empty())
                    .map(|piece| {
                        piece
                            .split_whitespace()
                            .take(2)
                            .collect::<Vec<_>>()
                            .join(" ")
                    })
                    .collect()
            }
            ReferenceSoftware::SPSS => {
                let proc = reference
                    .proc
                    .as_deref()
                    .expect("SPSS cell records a proc");
                proc.split(';')
                    .map(str::trim)
                    .filter(|piece| !piece.is_empty())
                    .map(|piece| piece.split('/').next().unwrap_or(piece).trim().to_string())
                    .collect()
            }
        }
    }

    /// Recorded package token expected in R / Python bodies, with the one
    /// documented spelling exception: the matrix records `scikit-learn`
    /// (the `PyPI` distribution name) but the import statement spells it
    /// `sklearn`. SAS / SPSS built-in PROCs carry no package, so they
    /// return `None` and the package assertion is skipped for them.
    fn expected_package_token(
        reference: &ReferenceImpl,
        software: ReferenceSoftware,
    ) -> Option<String> {
        match software {
            ReferenceSoftware::R | ReferenceSoftware::Python => {
                reference.package.as_deref().map(|pkg| match pkg {
                    "scikit-learn" => "sklearn".to_string(),
                    other => other.to_string(),
                })
            }
            ReferenceSoftware::SAS | ReferenceSoftware::SPSS => None,
        }
    }

    #[test]
    fn every_covered_cell_template_has_expected_syntactic_shape() {
        let matrix = CoverageMatrix::get_loaded();
        let columns = fixed_columns();
        let params = RenderParams::new();

        let mut covered = 0usize;

        for entry in matrix.algorithms() {
            for software in SOFTWARES {
                let coverage = matrix
                    .coverage(&entry.id, software)
                    .expect("matrix invariant: every (algorithm, software) cell exists");

                // Only covered cells render a snippet; `none` cells are
                // exercised by the sibling `Uncovered` tests.
                if matches!(coverage, CoverageState::None_) {
                    continue;
                }
                covered += 1;

                let snippet = generate_snippet(
                    &entry.id, &params, &columns, SHA256, software, &[], None,
                )
                .unwrap_or_else(|e| {
                    panic!("({}, {software:?}) failed to render: {e}", entry.id)
                });

                let text = match snippet {
                    SidecarSnippet::Snippet { text, .. } => text,
                    SidecarSnippet::Uncovered { .. } => panic!(
                        "({}, {software:?}) is covered ({coverage:?}) but produced an Uncovered sentinel",
                        entry.id
                    ),
                };

                // 1. `data.csv` literal (Requirement 2.5).
                assert!(
                    text.contains("data.csv"),
                    "({}, {software:?}) snippet must reference the data.csv literal",
                    entry.id
                );

                // 2. dataset SHA256 (Requirement 2.5).
                assert!(
                    text.contains(SHA256),
                    "({}, {software:?}) snippet must embed the dataset sha256",
                    entry.id
                );

                // 3. every input column name (Requirement 2.5).
                for column in &columns {
                    assert!(
                        text.contains(&column.name),
                        "({}, {software:?}) snippet must reference column {:?}",
                        entry.id,
                        column.name
                    );
                }

                // 4. matrix-recorded primary identifier token(s)
                //    (Requirement 2.2 — the snippet uses the designated
                //    Reference Implementation for the cell).
                let reference = entry
                    .reference
                    .get(&software)
                    .expect("matrix invariant: every cell records a ReferenceImpl");

                for token in required_identifier_tokens(reference, software) {
                    assert!(
                        contains_ci(&text, &token),
                        "({}, {software:?}) snippet must contain the recorded identifier {:?}",
                        entry.id,
                        token
                    );
                }

                // 4b. recorded package, where one applies (R / Python).
                if let Some(package) = expected_package_token(reference, software) {
                    assert!(
                        contains_ci(&text, &package),
                        "({}, {software:?}) snippet must reference the recorded package {:?}",
                        entry.id,
                        package
                    );
                }
            }
        }

        // Sanity: the data-driven walk must actually have run. The wave-1
        // matrix has 17 algorithms × 4 software − 5 `none` cells = 63
        // covered cells; we assert a non-empty walk rather than a fixed
        // count so adding a new algorithm extends coverage automatically.
        assert!(covered > 0, "expected at least one covered cell to assert against");
    }

    /// Focused anchor example (Cox PH, R) documenting the exact shape the
    /// data-driven test enforces for one representative cell.
    #[test]
    fn cox_r_snippet_embeds_survival_coxph_and_inputs() {
        let columns = fixed_columns();
        let snippet = generate_snippet(
            "cox",
            &RenderParams::new(),
            &columns,
            SHA256,
            ReferenceSoftware::R,
            &[],
            None,
        )
        .expect("cox × R must render");

        let SidecarSnippet::Snippet { text, .. } = snippet else {
            panic!("cox × R must be a covered Snippet");
        };

        assert!(text.contains("data.csv"));
        assert!(text.contains(SHA256));
        assert!(text.contains("outcome"));
        assert!(text.contains("group"));
        assert!(text.contains("survival::coxph"));
        assert!(text.contains("library(survival)"));
        assert!(!text.contains('\r'), "snippet must stay LF-only");
    }
}
