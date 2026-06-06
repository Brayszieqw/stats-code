//! `parity::run_local` driver — wave-1 entry point for the internal
//! `parity` Internal Subcommand.
//!
//! Feature: parity-and-multilang-sidecar (task 9.2).
//! _Requirements: 5.1, 5.4, 5.5, 5.6, 5.7, 6.6, 12.1, 12.6_
//!
//! # Where this fits
//!
//! `Command::Parity` (task 9.1) is dispatched in [`crate::handlers::run`]
//! by calling [`run_local`] and exiting with the returned code. The
//! launcher path is **not** taken: no port bind, no browser launch, no
//! single-instance lock (Requirement 5.8). The deterministic exit-code
//! map is the public contract observed by CI and by the maintainer's
//! local re-run loop:
//!
//! | exit | meaning                                                       | outcome variant         |
//! |------|---------------------------------------------------------------|-------------------------|
//! | `0`  | every gate passes                                             | [`ParityOutcome::AllPass`] |
//! | `2`  | at least one `fail` row in the Parity Validation Report       | [`ParityOutcome::FailRows`] |
//! | `3`  | `--filter <id>` did not match any algorithm in the matrix     | [`ParityOutcome::UnknownFilter`] |
//! | `4`  | `tolerance_config.yaml` missing, unreadable, or malformed     | [`ParityOutcome::ToleranceConfigError`] |
//! | `5`  | matrix declares cells inconsistent with the actual test surface | [`ParityOutcome::MatrixInconsistent`] |
//!
//! # Wave-1 simplification
//!
//! Design.md §6 ("Internal `parity` subcommand") spawns
//! `python validation/run_validation.py` as a subprocess and aggregates
//! its `report.json` into the exit-code map above. Wave-1 lands only the
//! *Rust-side* gates so the CLI variant (task 9.1) can stop returning the
//! eprintln placeholder and start enforcing the deterministic contract:
//!
//! 1. Load `validation/tolerance_config.yaml` (Requirement 12.1) — IO or
//!    parse failure ⇒ exit `4` (Requirement 12.6) without producing a
//!    Parity Validation Report.
//! 2. If `--filter <id>` is set, look it up in [`CoverageMatrix`]
//!    (Requirement 5.5) — miss ⇒ exit `3` (Requirement 5.7) without
//!    producing a Parity Validation Report.
//! 3. Otherwise return `AllPass` and exit `0`. The python suite spawn
//!    (Requirement 5.1, 5.4, 5.6, 6.6) lands in tasks 11.4 / 11.5 / 11.6,
//!    at which point the [`ParityOutcome::FailRows`] and
//!    [`ParityOutcome::MatrixInconsistent`] branches start firing too.
//!
//! The `FailRows` and `MatrixInconsistent` variants therefore exist
//! today only to lock down the public exit-code surface so wave-2 does
//! not need to re-shape it. They are unreachable from the wave-1 code
//! path.
//!
//! # Testability
//!
//! Two entry points:
//!
//! - [`run_local`] — production entry. Resolves the tolerance config to
//!   `validation/tolerance_config.yaml` relative to the current working
//!   directory (the CI runner and `cargo run -p stats-code -- parity`
//!   invocations both run from `crates/stats-code/`).
//! - [`run_local_with_tolerance_path`] — explicit-path variant. The unit
//!   tests below use it to exercise the IO and parse failure paths
//!   without depending on the on-disk file.
//!
//! Both end in [`ParityOutcome::exit_code`], a pure function of the
//! closed-set [`ParityOutcome`] enum, so the dispatcher is trivially
//! observable from tests.

use std::path::{Path, PathBuf};

use crate::cli::ParityArgs;
use crate::coverage_matrix::CoverageMatrix;
use crate::parity::tolerance;

/// Closed-set classification of a single `parity` Internal Subcommand
/// invocation.
///
/// The variants map 1:1 onto the exit-code table in the module docs.
/// They are deliberately structured (no formatted-string-only payloads)
/// so callers — and property tests in task 9.5 — can match on the cause
/// class without parsing message text.
#[derive(Debug, Clone, PartialEq)]
pub enum ParityOutcome {
    /// Every gate passed. Wave-1 reaches this whenever the tolerance
    /// config loads and the optional `--filter` resolves; wave-2 also
    /// requires the python suite to report no `fail` rows and no
    /// `reference_software_unavailable` skipped rows.
    AllPass,

    /// `--filter <value>` was supplied but no algorithm with that exact
    /// id exists in [`CoverageMatrix`] (Requirement 5.7). The launcher
    /// must exit non-zero, must not produce a Parity Validation Report,
    /// and must surface the unmatched filter to stderr.
    UnknownFilter {
        /// The exact value passed via `--filter`; preserved verbatim for
        /// the stderr message.
        filter: String,
    },

    /// `validation/tolerance_config.yaml` was missing, unreadable, or
    /// did not validate (Requirement 12.6). Holds the resolved path so
    /// the stderr message can name it precisely.
    ToleranceConfigError {
        /// Path the loader attempted to read.
        path: PathBuf,
        /// Loader error, rendered via [`std::fmt::Display`] for the
        /// stderr message. Underlying variants stay structured inside
        /// [`crate::parity::tolerance::ToleranceConfigError`].
        reason: String,
    },

    /// The matrix declares coverage cells that contradict the actual
    /// test surface (Requirement 6.6). Detected in wave-2 by the python
    /// suite (`validation/tests/test_coverage_matrix_consistency.py`,
    /// task 11.6). Wave-1 never produces this variant; the slot exists
    /// only to keep the public exit-code surface stable.
    MatrixInconsistent {
        /// Human-readable summary identifying the offending cell.
        reason: String,
    },

    /// At least one `fail` row in the Parity Validation Report
    /// (Requirement 5.4). Wave-1 never produces this variant; it ships
    /// in wave-2 once the python suite is invoked.
    FailRows {
        /// Number of `fail` rows in the report.
        count: usize,
    },
}

impl ParityOutcome {
    /// Map an outcome onto the deterministic exit code surfaced by
    /// [`run_local`]. The mapping is the public contract from
    /// design.md §6 "Internal `parity` subcommand → Exit codes".
    #[must_use] 
    pub fn exit_code(&self) -> i32 {
        match self {
            Self::AllPass => 0,
            Self::FailRows { .. } => 2,
            Self::UnknownFilter { .. } => 3,
            Self::ToleranceConfigError { .. } => 4,
            Self::MatrixInconsistent { .. } => 5,
        }
    }
}

/// Default location for `tolerance_config.yaml`, resolved relative to
/// the current working directory.
///
/// CI invokes the parity job from `crates/stats-code/`, and
/// `cargo run -p stats-code -- parity` runs from the same directory, so
/// the relative path `validation/tolerance_config.yaml` resolves to the
/// authoritative on-disk file in both contexts. Tests use
/// [`run_local_with_tolerance_path`] instead and never touch this
/// helper, so its return value is intentionally unconfigurable here.
#[must_use] 
pub fn default_tolerance_path() -> PathBuf {
    PathBuf::from("validation").join("tolerance_config.yaml")
}

/// Pure outcome classifier — the gate sequence with no IO beyond the
/// tolerance config read. Property-testable in isolation; called by
/// both [`run_local`] and [`run_local_with_tolerance_path`].
///
/// Gate order matches the spec contract: tolerance config first
/// (Requirement 12.6), filter second (Requirement 5.7), then the
/// (wave-2) python suite. Wave-1 short-circuits past the python suite
/// to [`ParityOutcome::AllPass`].
#[must_use] 
pub fn classify_outcome(
    matrix: &CoverageMatrix,
    filter: Option<&str>,
    tolerance_path: &Path,
) -> ParityOutcome {
    // Gate 1: tolerance_config.yaml. Any IO or schema failure ⇒ exit 4.
    if let Err(e) = tolerance::load_from_path(tolerance_path) {
        return ParityOutcome::ToleranceConfigError {
            path: tolerance_path.to_path_buf(),
            reason: e.to_string(),
        };
    }

    // Gate 2: --filter, if supplied. Case-sensitive exact match against
    // CoverageMatrix.lookup (Requirement 5.5). Miss ⇒ exit 3.
    if let Some(f) = filter {
        if matrix.lookup(f).is_none() {
            return ParityOutcome::UnknownFilter {
                filter: f.to_owned(),
            };
        }
    }

    // Wave-2 gates 3/4 (python suite + matrix consistency) live in
    // tasks 11.4 / 11.5 / 11.6. Wave-1 returns AllPass past gate 2.
    ParityOutcome::AllPass
}

/// Production entry point for the `parity` Internal Subcommand.
///
/// Resolves the tolerance config to its default location and delegates
/// to [`run_local_with_tolerance_path`]. Returns the exit code; the
/// caller in [`crate::handlers::run`] is responsible for
/// `std::process::exit`-ing with it.
#[must_use] 
pub fn run_local(args: &ParityArgs) -> i32 {
    run_local_with_tolerance_path(args, &default_tolerance_path())
}

/// Test-friendly entry point that takes an explicit tolerance config
/// path. Production callers go through [`run_local`].
#[must_use] 
pub fn run_local_with_tolerance_path(args: &ParityArgs, tolerance_path: &Path) -> i32 {
    let matrix = CoverageMatrix::get_loaded();
    let outcome = classify_outcome(matrix, args.filter.as_deref(), tolerance_path);
    write_stderr_for(&outcome);
    outcome.exit_code()
}

/// Render an outcome onto stderr in the format expected by CI / log
/// readers. Kept private — the stable surface is [`ParityOutcome`] and
/// its [`ParityOutcome::exit_code`].
fn write_stderr_for(outcome: &ParityOutcome) {
    match outcome {
        ParityOutcome::AllPass => {
            eprintln!(
                "parity: wave-1 placeholder — tolerance config loaded and matrix lookup OK; \
                 python parity suite spawn deferred to wave-2 (tasks 11.4 / 11.5)"
            );
        }
        ParityOutcome::UnknownFilter { filter } => {
            eprintln!(
                "parity: --filter {filter:?} did not match any algorithm in the Algorithm \
                 Coverage Matrix; no Parity Validation Report produced (Requirement 5.7)"
            );
        }
        ParityOutcome::ToleranceConfigError { path, reason } => {
            eprintln!(
                "parity: failed to load tolerance config at {path}: {reason} \
                 (Requirement 12.6)",
                path = path.display(),
            );
        }
        ParityOutcome::MatrixInconsistent { reason } => {
            eprintln!(
                "parity: Algorithm Coverage Matrix consistency check failed: {reason} \
                 (Requirement 6.6)"
            );
        }
        ParityOutcome::FailRows { count } => {
            eprintln!(
                "parity: {count} fail row(s) in Parity Validation Report (Requirement 5.4)"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    /// Schema-correct `tolerance_config.yaml` body matching what
    /// `crate::parity::tolerance::load_from_path` accepts.
    const VALID_TOLERANCE_YAML: &str = "\
version: 1
defaults:
  non_iterative: { absolute: 1.0e-9, relative: 1.0e-6 }
  iterative:     { absolute: 1.0e-7, relative: 1.0e-4 }
algorithms:
  cox:      { absolute: 1.0e-7, relative: 1.0e-4 }
  logistic: { absolute: 1.0e-7, relative: 1.0e-4 }
";

    fn write_temp_yaml(body: &str) -> NamedTempFile {
        let mut tmp = NamedTempFile::new().expect("create temp file");
        tmp.write_all(body.as_bytes()).expect("write yaml body");
        tmp.flush().expect("flush yaml body");
        tmp
    }

    #[test]
    fn exit_code_table_matches_design() {
        // Lock down the public exit-code surface so wave-2 can't drift
        // it without a code review surfacing the change.
        assert_eq!(ParityOutcome::AllPass.exit_code(), 0);
        assert_eq!(
            ParityOutcome::FailRows { count: 1 }.exit_code(),
            2
        );
        assert_eq!(
            ParityOutcome::UnknownFilter {
                filter: "x".into()
            }
            .exit_code(),
            3
        );
        assert_eq!(
            ParityOutcome::ToleranceConfigError {
                path: PathBuf::from("validation/tolerance_config.yaml"),
                reason: "synthetic".into(),
            }
            .exit_code(),
            4
        );
        assert_eq!(
            ParityOutcome::MatrixInconsistent {
                reason: "synthetic".into()
            }
            .exit_code(),
            5
        );
    }

    #[test]
    fn happy_path_no_filter_returns_zero() {
        let tmp = write_temp_yaml(VALID_TOLERANCE_YAML);
        let args = ParityArgs { filter: None };
        let code = run_local_with_tolerance_path(&args, tmp.path());
        assert_eq!(code, 0);
    }

    #[test]
    fn happy_path_filter_hits_returns_zero() {
        // `tableone` is the first entry in the embedded coverage
        // matrix and is locked down by
        // `coverage_matrix::tests::lookup_hit_and_miss`.
        let tmp = write_temp_yaml(VALID_TOLERANCE_YAML);
        let args = ParityArgs {
            filter: Some("tableone".to_owned()),
        };
        let code = run_local_with_tolerance_path(&args, tmp.path());
        assert_eq!(code, 0);
    }

    #[test]
    fn filter_miss_returns_three() {
        let tmp = write_temp_yaml(VALID_TOLERANCE_YAML);
        let args = ParityArgs {
            filter: Some("not_in_matrix_definitely".to_owned()),
        };
        let code = run_local_with_tolerance_path(&args, tmp.path());
        assert_eq!(code, 3);
    }

    #[test]
    fn filter_is_case_sensitive() {
        // Requirement 5.5: case-sensitive exact match. `TableOne` must
        // miss even though `tableone` hits.
        let tmp = write_temp_yaml(VALID_TOLERANCE_YAML);
        let args = ParityArgs {
            filter: Some("TableOne".to_owned()),
        };
        let code = run_local_with_tolerance_path(&args, tmp.path());
        assert_eq!(code, 3);
    }

    #[test]
    fn malformed_tolerance_config_returns_four() {
        // Wrong schema (missing `version`) — `tolerance::load_from_path`
        // returns `MissingField { field: "version" }`, which collapses
        // to exit code 4.
        let tmp = write_temp_yaml("not_a_valid_schema: true\n");
        let args = ParityArgs { filter: None };
        let code = run_local_with_tolerance_path(&args, tmp.path());
        assert_eq!(code, 4);
    }

    #[test]
    fn missing_tolerance_config_returns_four() {
        // Path that does not exist on disk — IO error must collapse to
        // exit code 4 with the offending path preserved in the variant.
        let missing = PathBuf::from("definitely-does-not-exist-12345.yaml");
        let args = ParityArgs { filter: None };
        let code = run_local_with_tolerance_path(&args, &missing);
        assert_eq!(code, 4);
    }

    #[test]
    fn classify_outcome_preserves_filter_text_on_miss() {
        // The stderr message identifies the unmatched filter verbatim
        // (Requirement 5.7). Classifying without going through
        // `run_local_with_tolerance_path` lets us inspect the variant
        // directly without scraping stderr.
        let tmp = write_temp_yaml(VALID_TOLERANCE_YAML);
        let matrix = CoverageMatrix::get_loaded();
        let outcome = classify_outcome(matrix, Some("WeIrD-CaSe-XYZ"), tmp.path());
        match outcome {
            ParityOutcome::UnknownFilter { filter } => {
                assert_eq!(filter, "WeIrD-CaSe-XYZ");
            }
            other => panic!("expected UnknownFilter, got {other:?}"),
        }
    }

    #[test]
    fn classify_outcome_preserves_path_on_tolerance_error() {
        let missing = PathBuf::from("definitely-does-not-exist-67890.yaml");
        let matrix = CoverageMatrix::get_loaded();
        let outcome = classify_outcome(matrix, None, &missing);
        match outcome {
            ParityOutcome::ToleranceConfigError { path, reason } => {
                assert_eq!(path, missing);
                assert!(
                    !reason.is_empty(),
                    "tolerance error reason must not be empty"
                );
            }
            other => panic!("expected ToleranceConfigError, got {other:?}"),
        }
    }

    #[test]
    fn default_tolerance_path_is_relative_to_cwd() {
        let p = default_tolerance_path();
        assert!(p.is_relative(), "default path must be relative, got {p:?}");
        // Path components are platform-aware; check tail rather than
        // string contents to stay correct on Windows.
        assert_eq!(
            p.file_name().and_then(|n| n.to_str()),
            Some("tolerance_config.yaml")
        );
    }
}
