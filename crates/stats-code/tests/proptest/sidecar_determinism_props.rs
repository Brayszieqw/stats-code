//! Property test: Sidecar snippet generator is deterministic and emits LF UTF-8.
//!
//! **Validates: Requirements 2.1**
//!
//! For any valid (`algorithm_id`, software, params, columns, `dataset_sha256`)
//! input drawn from the coverage matrix, two consecutive calls to
//! `generate_snippet` with identical inputs produce byte-identical output,
//! and the output contains no `\r` bytes.

use proptest::prelude::*;

use stats_code::coverage_matrix::{CoverageMatrix, CoverageState, ReferenceSoftware};
use stats_code::sidecar::{
    generate_snippet, Column, ColumnDtype, RenderParams, SidecarSnippet,
};

/// Strategy: pick a valid (`algorithm_id`, software) pair from the loaded
/// coverage matrix whose coverage state is NOT `none` (i.e. the cell
/// actually produces a snippet).
fn arb_covered_cell() -> impl Strategy<Value = (String, ReferenceSoftware)> {
    let matrix = CoverageMatrix::get_loaded();
    let softwares = [
        ReferenceSoftware::R,
        ReferenceSoftware::SAS,
        ReferenceSoftware::Python,
        ReferenceSoftware::SPSS,
    ];

    // Collect all (algorithm_id, software) pairs that are covered.
    let mut covered: Vec<(String, ReferenceSoftware)> = Vec::new();
    for entry in matrix.algorithms() {
        for sw in softwares {
            let cov = entry.coverage.get(&sw).copied().unwrap_or(CoverageState::None_);
            if !matches!(cov, CoverageState::None_) {
                covered.push((entry.id.clone(), sw));
            }
        }
    }

    // Also include some `none` cells to test the Uncovered path.
    let mut all_cells: Vec<(String, ReferenceSoftware)> = covered;
    for entry in matrix.algorithms() {
        for sw in softwares {
            let cov = entry.coverage.get(&sw).copied().unwrap_or(CoverageState::None_);
            if matches!(cov, CoverageState::None_) {
                all_cells.push((entry.id.clone(), sw));
            }
        }
    }

    // Use prop::sample::select to pick uniformly from the collected cells.
    prop::sample::select(all_cells)
}

/// Strategy: generate an arbitrary `ColumnDtype`.
fn arb_dtype() -> impl Strategy<Value = ColumnDtype> {
    prop_oneof![
        Just(ColumnDtype::Numeric),
        Just(ColumnDtype::Categorical),
        Just(ColumnDtype::Date),
        Just(ColumnDtype::String),
    ]
}

/// Strategy: generate an arbitrary Column with a valid identifier-like name.
fn arb_column() -> impl Strategy<Value = Column> {
    ("[a-z][a-z0-9_]{0,15}", arb_dtype()).prop_map(|(name, dtype)| Column { name, dtype })
}

/// Strategy: generate a valid 64-character lowercase hex SHA256 string.
fn arb_sha256() -> impl Strategy<Value = String> {
    "[0-9a-f]{64}"
}

/// Strategy: generate arbitrary `RenderParams` (0–4 key-value pairs with
/// simple alphanumeric keys and values).
fn arb_params() -> impl Strategy<Value = RenderParams> {
    prop::collection::btree_map("[a-z][a-z0-9_.]{0,10}", "[a-zA-Z0-9._]{0,20}", 0..5)
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 64, failure_persistence: None, .. ProptestConfig::default() })]

    /// **Property 1: Sidecar snippet generator is deterministic and emits LF UTF-8**
    ///
    /// **Validates: Requirements 2.1**
    ///
    /// For any covered cell input, two consecutive calls to `generate_snippet`
    /// with identical arguments produce byte-identical output, and the output
    /// text (when present) contains no `\r` bytes.
    #[test]
    fn sidecar_deterministic_and_lf_utf8(
        (algorithm_id, software) in arb_covered_cell(),
        columns in prop::collection::vec(arb_column(), 1..6),
        sha256 in arb_sha256(),
        params in arb_params(),
    ) {
        let result_a = generate_snippet(
            &algorithm_id,
            &params,
            &columns,
            &sha256,
            software,
            &[],
            None,
        );
        let result_b = generate_snippet(
            &algorithm_id,
            &params,
            &columns,
            &sha256,
            software,
            &[],
            None,
        );

        // Both calls must succeed or both must fail with the same error kind.
        match (&result_a, &result_b) {
            (Ok(snippet_a), Ok(snippet_b)) => {
                // Byte-identical output (determinism).
                prop_assert_eq!(snippet_a, snippet_b,
                    "two calls with identical inputs must produce byte-identical output");

                // No CR bytes in the text (LF-only invariant).
                match snippet_a {
                    SidecarSnippet::Snippet { text, .. } => {
                        prop_assert!(
                            !text.contains('\r'),
                            "snippet text must not contain \\r (LF-only): algorithm={}, software={:?}",
                            algorithm_id, software
                        );
                        // Valid UTF-8 is guaranteed by Rust's String type, but
                        // let's also assert the text is non-empty for covered cells.
                        prop_assert!(!text.is_empty(),
                            "covered cell must produce non-empty snippet text");
                    }
                    SidecarSnippet::Uncovered { coverage_value, .. } => {
                        prop_assert_eq!(coverage_value, "none",
                            "uncovered sentinel must carry coverage_value = \"none\"");
                    }
                }
            }
            (Err(_), Err(_)) => {
                // Both failed — determinism holds (same error for same input).
                // We don't assert error equality since Error types may not impl Eq,
                // but the important property is that both calls agree on success/failure.
            }
            _ => {
                prop_assert!(false,
                    "one call succeeded and the other failed — non-deterministic behavior \
                     for algorithm={}, software={:?}", algorithm_id, software);
            }
        }
    }
}
