//! **Validates: Requirements 1.5, 1.6, 1.8, 2.4, 6.3**
//!
//! Property 2: Coverage drives snippet variant; uncovered cells emit a
//! structured sentinel.
//!
//! For any (algorithm, software) pair drawn from the embedded coverage
//! matrix:
//!
//! - If the matrix value is `none`, `generate_snippet` returns
//!   `SidecarSnippet::Uncovered` with `coverage_value == "none"` and no
//!   body / comment / placeholder text.
//! - If the matrix value is non-`none` (live, recorded, sidecar_only),
//!   `generate_snippet` returns `SidecarSnippet::Snippet` with non-empty
//!   `text`.

use proptest::prelude::*;

use stats_code::coverage_matrix::{CoverageMatrix, CoverageState, ReferenceSoftware};
use stats_code::sidecar::{generate_snippet, Column, ColumnDtype, RenderParams, SidecarSnippet};

/// Strategy: pick a random algorithm index from the loaded matrix.
fn arb_algorithm_index() -> impl Strategy<Value = usize> {
    let matrix = CoverageMatrix::get_loaded();
    0..matrix.algorithms().len()
}

/// Strategy: pick a random ReferenceSoftware variant.
fn arb_software() -> impl Strategy<Value = ReferenceSoftware> {
    prop_oneof![
        Just(ReferenceSoftware::R),
        Just(ReferenceSoftware::SAS),
        Just(ReferenceSoftware::Python),
        Just(ReferenceSoftware::SPSS),
    ]
}

/// Strategy: generate a valid 64-character lowercase hex SHA256 string.
fn arb_sha256() -> impl Strategy<Value = String> {
    proptest::collection::vec(prop::sample::select(b"0123456789abcdef".as_slice()), 64)
        .prop_map(|bytes| bytes.iter().map(|b| *b as char).collect::<String>())
}

/// Strategy: generate an arbitrary column list (2..=8 columns).
///
/// The lower bound is 2 because the embedded Wave-3 templates reference
/// column slots up to `{{column.1.…}}` (e.g. the `tableone` R template
/// uses `{{column.0.name}}` as the variable and `{{column.1.name}}` as the
/// stratum). A real analysis always supplies at least as many columns as
/// the algorithm consumes, so constraining the generator to ≥2 columns
/// keeps it inside the valid input space for Property 2 (coverage drives
/// snippet variant) rather than exercising the orthogonal
/// `ColumnIndexOutOfRange` render error, which is covered by `render.rs`'s
/// own unit tests.
fn arb_columns() -> impl Strategy<Value = Vec<Column>> {
    let arb_dtype = prop_oneof![
        Just(ColumnDtype::Numeric),
        Just(ColumnDtype::Categorical),
        Just(ColumnDtype::Date),
        Just(ColumnDtype::String),
    ];
    proptest::collection::vec(
        ("[a-z][a-z0-9_]{0,15}", arb_dtype).prop_map(|(name, dtype)| Column { name, dtype }),
        2..=8,
    )
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 128, failure_persistence: None, .. ProptestConfig::default() })]

    /// Property 2: Coverage drives snippet variant; uncovered cells emit a
    /// structured sentinel.
    ///
    /// For arbitrary (matrix algorithm, software) combinations:
    /// - `none` ⇒ `Uncovered { coverage_value == "none" }` with no body,
    ///   no comment, no placeholder text.
    /// - non-`none` ⇒ `Snippet { text 非空 }`.
    #[test]
    fn coverage_drives_variant_and_uncovered_sentinel(
        alg_idx in arb_algorithm_index(),
        software in arb_software(),
        sha256 in arb_sha256(),
        columns in arb_columns(),
    ) {
        let matrix = CoverageMatrix::get_loaded();
        let entry = &matrix.algorithms()[alg_idx];
        let algorithm_id = &entry.id;

        let coverage = entry
            .coverage
            .get(&software)
            .copied()
            .expect("matrix invariant: every (algorithm, software) cell exists");

        let params = RenderParams::new();
        let result = generate_snippet(
            algorithm_id,
            &params,
            &columns,
            &sha256,
            software,
            &[],   // no API keys
            None,  // no working directory
        );

        // The generator must succeed for every cell in the matrix.
        let snippet = result.unwrap_or_else(|e| {
            panic!("generate_snippet({algorithm_id}, {software:?}) failed: {e}");
        });

        match coverage {
            CoverageState::None_ => {
                // Requirement 2.4: structured "uncovered" sentinel with
                // coverage_value == "none", no body, no comment, no
                // placeholder text.
                match snippet {
                    SidecarSnippet::Uncovered {
                        algorithm_id: ref aid,
                        software: sw,
                        ref coverage_value,
                    } => {
                        prop_assert_eq!(aid, algorithm_id);
                        prop_assert_eq!(sw, software);
                        prop_assert_eq!(coverage_value.as_str(), "none");
                        // The Uncovered variant carries no body / comment /
                        // placeholder text by construction — the enum has
                        // no such fields. This assertion documents the
                        // structural guarantee.
                    }
                    SidecarSnippet::Snippet { .. } => {
                        prop_assert!(
                            false,
                            "coverage == none but got Snippet variant for ({}, {:?})",
                            algorithm_id,
                            software,
                        );
                    }
                }
            }
            CoverageState::Live | CoverageState::Recorded | CoverageState::SidecarOnly => {
                // Non-none coverage must produce a Snippet with non-empty
                // text.
                match snippet {
                    SidecarSnippet::Snippet {
                        software: sw,
                        algorithm_id: ref aid,
                        ref text,
                        ref sha256_of_dataset,
                        ref release_version,
                    } => {
                        prop_assert_eq!(sw, software);
                        prop_assert_eq!(aid, algorithm_id);
                        prop_assert!(
                            !text.is_empty(),
                            "non-none coverage must produce non-empty snippet text for ({}, {:?})",
                            algorithm_id,
                            software,
                        );
                        prop_assert_eq!(sha256_of_dataset.as_str(), sha256.as_str());
                        prop_assert_eq!(
                            release_version.as_str(),
                            matrix.release_version(),
                        );
                    }
                    SidecarSnippet::Uncovered { .. } => {
                        prop_assert!(
                            false,
                            "coverage == {:?} but got Uncovered variant for ({}, {:?})",
                            coverage,
                            algorithm_id,
                            software,
                        );
                    }
                }
            }
        }
    }
}
