//! **Validates: Requirements 4.7, 6.1, 6.2, 6.5, 6.6**
//!
//! Property 5: Algorithm Coverage Matrix is structurally consistent with
//! the test surface.
//!
//! Generates arbitrary (matrix, sidecar templates, Live cases, Recorded
//! tables) combinations and asserts that [`check_consistency`] returns
//! structured errors identifying the offending cell when:
//!
//! - (a) a `live` cell lacks a Live case,
//! - (b) a `recorded` cell lacks a Known-Values Table,
//! - (c) a `sidecar_only` cell lacks a template,
//! - (d) a `none` cell has a template or case present.

use std::collections::BTreeMap;

use proptest::prelude::*;
use proptest::strategy::ValueTree;

use stats_code::coverage_matrix::{
    check_consistency, AlgorithmEntry, ConsistencyError, CoverageMatrix, CoverageState,
    ReferenceSoftware, ReferenceImpl, TestSurface,
};

// ─── Strategies ─────────────────────────────────────────────────────────

/// Strategy: generate a random `CoverageState`.
fn arb_coverage_state() -> impl Strategy<Value = CoverageState> {
    prop_oneof![
        Just(CoverageState::Live),
        Just(CoverageState::Recorded),
        Just(CoverageState::SidecarOnly),
        Just(CoverageState::None_),
    ]
}

/// Strategy: generate a valid algorithm id (lowercase alphanumeric + underscore).
fn arb_algorithm_id() -> impl Strategy<Value = String> {
    "[a-z][a-z0-9_]{1,12}"
}

/// All four reference softwares in canonical order.
const ALL_SOFTWARE: [ReferenceSoftware; 4] = [
    ReferenceSoftware::R,
    ReferenceSoftware::SAS,
    ReferenceSoftware::Python,
    ReferenceSoftware::SPSS,
];

/// Strategy: generate a single `AlgorithmEntry` with arbitrary coverage states.
fn arb_algorithm_entry() -> impl Strategy<Value = AlgorithmEntry> {
    (
        arb_algorithm_id(),
        arb_coverage_state(),
        arb_coverage_state(),
        arb_coverage_state(),
        arb_coverage_state(),
    )
        .prop_map(|(id, cov_r, cov_sas, cov_py, cov_spss)| {
            let mut coverage = BTreeMap::new();
            coverage.insert(ReferenceSoftware::R, cov_r);
            coverage.insert(ReferenceSoftware::SAS, cov_sas);
            coverage.insert(ReferenceSoftware::Python, cov_py);
            coverage.insert(ReferenceSoftware::SPSS, cov_spss);

            let mut reference = BTreeMap::new();
            for sw in ALL_SOFTWARE {
                reference.insert(sw, ReferenceImpl {
                    callable: Some("f".to_string()),
                    proc: None,
                    package: Some("pkg".to_string()),
                    version: "1.0.0".to_string(),
                });
            }

            AlgorithmEntry {
                id,
                display_name: "Test".to_string(),
                iterative: false,
                coverage,
                reference,
            }
        })
}

/// Strategy: generate a `CoverageMatrix` with 1..=4 algorithms (unique ids).
fn arb_matrix() -> impl Strategy<Value = CoverageMatrix> {
    proptest::collection::vec(arb_algorithm_entry(), 1..=4).prop_map(|mut entries| {
        // Ensure unique ids by appending index suffix.
        for (i, entry) in entries.iter_mut().enumerate() {
            entry.id = format!("{}_{}", entry.id, i);
        }
        CoverageMatrix {
            schema_version: 1,
            release_version: "0.0.0-test".to_string(),
            algorithms: entries,
        }
    })
}

/// Strategy: generate a `TestSurface` that is a random subset of all possible
/// (`algorithm_id`, software) pairs from the given matrix. Each cell has a
/// 50% chance of being present in each surface set.
fn arb_surface_for_matrix(matrix: &CoverageMatrix) -> impl Strategy<Value = TestSurface> {
    // Collect all (algorithm_id, software) pairs.
    let pairs: Vec<(String, ReferenceSoftware)> = matrix
        .algorithms
        .iter()
        .flat_map(|entry| ALL_SOFTWARE.iter().map(move |&sw| (entry.id.clone(), sw)))
        .collect();

    let n = pairs.len();
    // Generate a bitmask for each of the three surface sets.
    (
        proptest::collection::vec(proptest::bool::ANY, n),
        proptest::collection::vec(proptest::bool::ANY, n),
        proptest::collection::vec(proptest::bool::ANY, n),
    )
        .prop_map(move |(tmpl_bits, live_bits, rec_bits)| {
            let mut surface = TestSurface::default();
            for (i, pair) in pairs.iter().enumerate() {
                if tmpl_bits.get(i).copied().unwrap_or(false) {
                    surface.templates.insert(pair.clone());
                }
                if live_bits.get(i).copied().unwrap_or(false) {
                    surface.live_cases.insert(pair.clone());
                }
                if rec_bits.get(i).copied().unwrap_or(false) {
                    surface.recorded_tables.insert(pair.clone());
                }
            }
            surface
        })
}

/// Build a perfectly consistent `TestSurface` for a given matrix (every cell
/// has exactly the resources its coverage state demands, and no more).
fn consistent_surface(matrix: &CoverageMatrix) -> TestSurface {
    let mut surface = TestSurface::default();
    for entry in &matrix.algorithms {
        for &sw in &ALL_SOFTWARE {
            let state = entry.coverage.get(&sw).copied().unwrap_or(CoverageState::None_);
            let key = (entry.id.clone(), sw);
            match state {
                CoverageState::Live => {
                    surface.live_cases.insert(key);
                }
                CoverageState::Recorded => {
                    surface.recorded_tables.insert(key);
                }
                CoverageState::SidecarOnly => {
                    surface.templates.insert(key);
                }
                CoverageState::None_ => {
                    // No resources for `none` cells.
                }
            }
        }
    }
    surface
}

// ─── Properties ─────────────────────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig { cases: 256, failure_persistence: None, .. ProptestConfig::default() })]

    /// Property 5: A perfectly consistent surface produces zero errors.
    ///
    /// For any arbitrary matrix, building a surface that exactly matches
    /// the declared coverage states must yield an empty error list.
    #[test]
    fn consistent_surface_yields_no_errors(matrix in arb_matrix()) {
        let surface = consistent_surface(&matrix);
        let errors = check_consistency(&matrix, &surface);
        prop_assert!(
            errors.is_empty(),
            "consistent surface should produce no errors, got: {:?}",
            errors
        );
    }

    /// Property 5a: A `live` cell missing its Live case produces
    /// `MissingLiveCase` for that cell.
    #[test]
    fn live_cell_missing_case_produces_error(matrix in arb_matrix()) {
        // Start with a consistent surface, then remove all live cases.
        let mut surface = consistent_surface(&matrix);
        let removed: Vec<_> = surface.live_cases.iter().cloned().collect();
        surface.live_cases.clear();

        let errors = check_consistency(&matrix, &surface);

        // Every removed live case should produce a MissingLiveCase error.
        for (alg_id, sw) in &removed {
            prop_assert!(
                errors.iter().any(|e| matches!(
                    e,
                    ConsistencyError::MissingLiveCase { algorithm_id, software }
                    if algorithm_id == alg_id && *software == *sw
                )),
                "expected MissingLiveCase for ({}, {:?}) but not found in {:?}",
                alg_id, sw, errors
            );
        }
    }

    /// Property 5b: A `recorded` cell missing its Known-Values Table
    /// produces `MissingKnownValues` for that cell.
    #[test]
    fn recorded_cell_missing_table_produces_error(matrix in arb_matrix()) {
        let mut surface = consistent_surface(&matrix);
        let removed: Vec<_> = surface.recorded_tables.iter().cloned().collect();
        surface.recorded_tables.clear();

        let errors = check_consistency(&matrix, &surface);

        for (alg_id, sw) in &removed {
            prop_assert!(
                errors.iter().any(|e| matches!(
                    e,
                    ConsistencyError::MissingKnownValues { algorithm_id, software }
                    if algorithm_id == alg_id && *software == *sw
                )),
                "expected MissingKnownValues for ({}, {:?}) but not found in {:?}",
                alg_id, sw, errors
            );
        }
    }

    /// Property 5c: A `sidecar_only` cell missing its template produces
    /// `MissingTemplate` for that cell.
    #[test]
    fn sidecar_only_cell_missing_template_produces_error(matrix in arb_matrix()) {
        let mut surface = consistent_surface(&matrix);
        let removed: Vec<_> = surface.templates.iter().cloned().collect();
        surface.templates.clear();

        let errors = check_consistency(&matrix, &surface);

        for (alg_id, sw) in &removed {
            prop_assert!(
                errors.iter().any(|e| matches!(
                    e,
                    ConsistencyError::MissingTemplate { algorithm_id, software }
                    if algorithm_id == alg_id && *software == *sw
                )),
                "expected MissingTemplate for ({}, {:?}) but not found in {:?}",
                alg_id, sw, errors
            );
        }
    }

    /// Property 5d: A `none` cell that has a template, live case, or
    /// recorded table present produces the corresponding `Unexpected*`
    /// error.
    #[test]
    fn none_cell_with_resources_produces_error(matrix in arb_matrix()) {
        // Start with a consistent surface (none cells have nothing),
        // then add all (algorithm, software) pairs to all surface sets.
        let mut surface = consistent_surface(&matrix);

        // Collect all `none` cells.
        let none_cells: Vec<(String, ReferenceSoftware)> = matrix
            .algorithms
            .iter()
            .flat_map(|entry| {
                ALL_SOFTWARE.iter().filter_map(move |&sw| {
                    if entry.coverage.get(&sw) == Some(&CoverageState::None_) {
                        Some((entry.id.clone(), sw))
                    } else {
                        None
                    }
                })
            })
            .collect();

        // Add all none cells to every surface set.
        for key in &none_cells {
            surface.templates.insert(key.clone());
            surface.live_cases.insert(key.clone());
            surface.recorded_tables.insert(key.clone());
        }

        let errors = check_consistency(&matrix, &surface);

        for (alg_id, sw) in &none_cells {
            prop_assert!(
                errors.iter().any(|e| matches!(
                    e,
                    ConsistencyError::UnexpectedTemplate { algorithm_id, software }
                    if algorithm_id == alg_id && *software == *sw
                )),
                "expected UnexpectedTemplate for ({}, {:?}) but not found in {:?}",
                alg_id, sw, errors
            );
            prop_assert!(
                errors.iter().any(|e| matches!(
                    e,
                    ConsistencyError::UnexpectedLiveCase { algorithm_id, software }
                    if algorithm_id == alg_id && *software == *sw
                )),
                "expected UnexpectedLiveCase for ({}, {:?}) but not found in {:?}",
                alg_id, sw, errors
            );
            prop_assert!(
                errors.iter().any(|e| matches!(
                    e,
                    ConsistencyError::UnexpectedKnownValues { algorithm_id, software }
                    if algorithm_id == alg_id && *software == *sw
                )),
                "expected UnexpectedKnownValues for ({}, {:?}) but not found in {:?}",
                alg_id, sw, errors
            );
        }
    }

    /// Property 5 (general): For any arbitrary (matrix, surface)
    /// combination, every error returned by `check_consistency` correctly
    /// identifies an offending cell whose coverage state contradicts the
    /// surface.
    #[test]
    fn every_error_identifies_a_genuine_inconsistency(matrix in arb_matrix()) {
        // Generate a random surface for this matrix.
        let surface_strategy = arb_surface_for_matrix(&matrix);
        let mut runner = proptest::test_runner::TestRunner::new(
            ProptestConfig { cases: 1, failure_persistence: None, .. ProptestConfig::default() }
        );
        let surface = surface_strategy
            .new_tree(&mut runner)
            .unwrap()
            .current();

        let errors = check_consistency(&matrix, &surface);

        for error in &errors {
            match error {
                ConsistencyError::MissingLiveCase { algorithm_id, software } => {
                    let state = matrix.coverage(algorithm_id, *software);
                    prop_assert!(
                        state == Some(CoverageState::Live),
                        "MissingLiveCase reported but state is {:?}", state
                    );
                    prop_assert!(
                        !surface.live_cases.contains(&(algorithm_id.clone(), *software)),
                        "MissingLiveCase reported but live_cases contains the pair"
                    );
                }
                ConsistencyError::MissingKnownValues { algorithm_id, software } => {
                    let state = matrix.coverage(algorithm_id, *software);
                    prop_assert!(
                        state == Some(CoverageState::Recorded),
                        "MissingKnownValues reported but state is {:?}", state
                    );
                    prop_assert!(
                        !surface.recorded_tables.contains(&(algorithm_id.clone(), *software)),
                        "MissingKnownValues reported but recorded_tables contains the pair"
                    );
                }
                ConsistencyError::MissingTemplate { algorithm_id, software } => {
                    let state = matrix.coverage(algorithm_id, *software);
                    prop_assert!(
                        state == Some(CoverageState::SidecarOnly),
                        "MissingTemplate reported but state is {:?}", state
                    );
                    prop_assert!(
                        !surface.templates.contains(&(algorithm_id.clone(), *software)),
                        "MissingTemplate reported but templates contains the pair"
                    );
                }
                ConsistencyError::UnexpectedTemplate { algorithm_id, software } => {
                    let state = matrix.coverage(algorithm_id, *software);
                    prop_assert!(
                        state == Some(CoverageState::None_),
                        "UnexpectedTemplate reported but state is {:?}", state
                    );
                    prop_assert!(
                        surface.templates.contains(&(algorithm_id.clone(), *software)),
                        "UnexpectedTemplate reported but templates does NOT contain the pair"
                    );
                }
                ConsistencyError::UnexpectedLiveCase { algorithm_id, software } => {
                    let state = matrix.coverage(algorithm_id, *software);
                    prop_assert!(
                        state == Some(CoverageState::None_),
                        "UnexpectedLiveCase reported but state is {:?}", state
                    );
                    prop_assert!(
                        surface.live_cases.contains(&(algorithm_id.clone(), *software)),
                        "UnexpectedLiveCase reported but live_cases does NOT contain the pair"
                    );
                }
                ConsistencyError::UnexpectedKnownValues { algorithm_id, software } => {
                    let state = matrix.coverage(algorithm_id, *software);
                    prop_assert!(
                        state == Some(CoverageState::None_),
                        "UnexpectedKnownValues reported but state is {:?}", state
                    );
                    prop_assert!(
                        surface.recorded_tables.contains(&(algorithm_id.clone(), *software)),
                        "UnexpectedKnownValues reported but recorded_tables does NOT contain the pair"
                    );
                }
            }
        }
    }
}
