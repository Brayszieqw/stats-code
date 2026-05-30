//! **Validates: Requirements 4.5, 4.9, 4.10, 5.4, 5.7**
//!
//! Property 7: Suite exit-code aggregation.
//!
//! Generates arbitrary `Vec<ParityReportRow>` (0–10 rows, each with a
//! random `ParityVerdict` ∈ {Pass, Fail, Skipped} and random
//! `skipped_reason`) plus an optional filter string. Then calls
//! [`aggregate_exit_code`] and asserts:
//!
//! - Exit code `0` ⇔ no `Fail` rows AND no `Skipped` rows with
//!   `skipped_reason == "reference_software_unavailable"` AND (filter is
//!   `None` OR filter matches at least one row's `algorithm_id`).
//! - Exit code `2` ⇔ at least one `Fail` row OR at least one `Skipped`
//!   row with `skipped_reason == "reference_software_unavailable"`.
//! - Exit code `3` ⇔ filter is `Some(f)` AND no row has
//!   `algorithm_id == f` AND no `Fail` rows AND no unavailable-skip rows.
//! - Exit code is always in {0, 2, 3}.

use proptest::prelude::*;

use stats_code::coverage_matrix::ReferenceSoftware;
use stats_code::parity::{
    aggregate_exit_code, ParityReportRow, ParityVerdict, REASON_REFERENCE_UNAVAILABLE,
};

// ─── Strategies ─────────────────────────────────────────────────────────

/// Strategy: generate a random ParityVerdict.
fn arb_verdict() -> impl Strategy<Value = ParityVerdict> {
    prop_oneof![
        Just(ParityVerdict::Pass),
        Just(ParityVerdict::Fail),
        Just(ParityVerdict::Skipped),
    ]
}

/// Strategy: generate a random ReferenceSoftware.
fn arb_software() -> impl Strategy<Value = ReferenceSoftware> {
    prop_oneof![
        Just(ReferenceSoftware::R),
        Just(ReferenceSoftware::SAS),
        Just(ReferenceSoftware::Python),
        Just(ReferenceSoftware::SPSS),
    ]
}

/// Strategy: generate a random skipped_reason.
/// When verdict is Skipped, the reason may be the sentinel
/// "reference_software_unavailable" or some other reason or None.
fn arb_skipped_reason(verdict: ParityVerdict) -> impl Strategy<Value = Option<String>> {
    match verdict {
        ParityVerdict::Skipped => prop_oneof![
            Just(Some(REASON_REFERENCE_UNAVAILABLE.to_string())),
            Just(Some("timeout".to_string())),
            Just(Some("other_reason".to_string())),
            Just(None),
        ]
        .boxed(),
        _ => Just(None).boxed(),
    }
}

/// Strategy: generate a random algorithm_id from a small fixed set.
fn arb_algorithm_id() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("tableone".to_string()),
        Just("cox".to_string()),
        Just("logistic".to_string()),
        Just("ttest".to_string()),
        Just("anova".to_string()),
    ]
}

/// Strategy: generate a single ParityReportRow with arbitrary verdict and reason.
fn arb_report_row() -> impl Strategy<Value = ParityReportRow> {
    (arb_algorithm_id(), arb_software(), arb_verdict()).prop_flat_map(
        |(algorithm_id, software, verdict)| {
            arb_skipped_reason(verdict).prop_map(move |skipped_reason| ParityReportRow {
                algorithm_id: algorithm_id.clone(),
                algorithm_display_name: "Test".to_string(),
                software,
                case_id: "case_1".to_string(),
                metric: "mean".to_string(),
                stats_engine_value: 1.0,
                reference_value_or_na: if verdict == ParityVerdict::Skipped {
                    None
                } else {
                    Some(1.0)
                },
                absolute_difference: if verdict == ParityVerdict::Skipped {
                    None
                } else {
                    Some(0.0)
                },
                relative_difference: if verdict == ParityVerdict::Skipped {
                    None
                } else {
                    Some(0.0)
                },
                active_absolute_tolerance: 1e-9,
                active_relative_tolerance: 1e-6,
                verdict,
                skipped_reason,
            })
        },
    )
}

/// Strategy: generate a Vec of 0–10 report rows.
fn arb_rows() -> impl Strategy<Value = Vec<ParityReportRow>> {
    proptest::collection::vec(arb_report_row(), 0..=10)
}

/// Strategy: generate an optional filter string.
/// The filter may be one of the known algorithm ids (hit) or a
/// completely unknown id (miss).
fn arb_filter() -> impl Strategy<Value = Option<String>> {
    prop_oneof![
        Just(None),
        arb_algorithm_id().prop_map(Some),
        Just(Some("nonexistent_algorithm_xyz".to_string())),
    ]
}

// ─── Helper predicates ──────────────────────────────────────────────────

fn has_fail(rows: &[ParityReportRow]) -> bool {
    rows.iter().any(|r| r.verdict == ParityVerdict::Fail)
}

fn has_unavailable_skip(rows: &[ParityReportRow]) -> bool {
    rows.iter().any(|r| {
        r.verdict == ParityVerdict::Skipped
            && r.skipped_reason.as_deref() == Some(REASON_REFERENCE_UNAVAILABLE)
    })
}

fn filter_hits(rows: &[ParityReportRow], filter: Option<&str>) -> bool {
    match filter {
        None => true,
        Some(f) => rows.iter().any(|r| r.algorithm_id == f),
    }
}

// ─── Properties ─────────────────────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig { cases: 512, failure_persistence: None, .. ProptestConfig::default() })]

    /// Property 7: Exit code 0 ⇔ no fail ∧ no reference_software_unavailable
    /// skipped ∧ filter hit (or no filter).
    ///
    /// **Validates: Requirements 4.5, 4.9, 4.10, 5.4, 5.7**
    #[test]
    fn exit_code_zero_iff_no_fail_no_unavailable_skip_and_filter_hit(
        rows in arb_rows(),
        filter in arb_filter(),
    ) {
        let code = aggregate_exit_code(&rows, filter.as_deref());

        let expect_zero = !has_fail(&rows)
            && !has_unavailable_skip(&rows)
            && filter_hits(&rows, filter.as_deref());

        if expect_zero {
            prop_assert!(
                code == 0,
                "expected exit 0 (no fail, no unavailable skip, filter hit), got {}",
                code
            );
        } else {
            prop_assert!(
                code != 0,
                "expected non-zero exit, but got 0. has_fail={}, has_unavailable_skip={}, filter_hits={}",
                has_fail(&rows),
                has_unavailable_skip(&rows),
                filter_hits(&rows, filter.as_deref()),
            );
        }
    }

    /// Property 7a: Exit code is strictly positive when there are failures,
    /// and the cause class is identifiable (exit 2 = fail or unavailable skip).
    ///
    /// **Validates: Requirements 4.5, 5.4**
    #[test]
    fn fail_rows_produce_exit_two(
        rows in arb_rows().prop_filter("must have at least one fail row", |rows| {
            rows.iter().any(|r| r.verdict == ParityVerdict::Fail)
        }),
        filter in arb_filter(),
    ) {
        let code = aggregate_exit_code(&rows, filter.as_deref());
        prop_assert!(
            code == 2,
            "expected exit 2 for fail rows, got {}",
            code
        );
    }

    /// Property 7b: Exit code 3 ⇔ filter miss with no fail and no unavailable skip.
    ///
    /// **Validates: Requirement 5.7**
    #[test]
    fn filter_miss_without_fail_produces_exit_three(
        rows in arb_rows().prop_filter("no fail and no unavailable skip", |rows| {
            !has_fail(rows) && !has_unavailable_skip(rows)
        }),
        filter in arb_filter().prop_filter("filter must miss", |f| {
            // Only keep filters that are Some and won't match known algorithm ids
            matches!(f.as_deref(), Some("nonexistent_algorithm_xyz"))
        }),
    ) {
        let code = aggregate_exit_code(&rows, filter.as_deref());
        // The filter misses only if no row has algorithm_id == filter value.
        // Our "nonexistent_algorithm_xyz" is guaranteed to miss the known set.
        if !filter_hits(&rows, filter.as_deref()) {
            prop_assert!(
                code == 3,
                "expected exit 3 for filter miss (no fail, no unavailable skip), got {}",
                code
            );
        }
    }

    /// Property 7c: Exit code is always in the closed set {0, 2, 3}.
    ///
    /// **Validates: Requirements 4.5, 4.9, 4.10, 5.4, 5.7**
    #[test]
    fn exit_code_is_in_valid_set(
        rows in arb_rows(),
        filter in arb_filter(),
    ) {
        let code = aggregate_exit_code(&rows, filter.as_deref());
        prop_assert!(
            code == 0 || code == 2 || code == 3,
            "exit code must be in {{0, 2, 3}}, got {}",
            code
        );
    }

    /// Property 7d: Unavailable-skip rows (without any fail) produce exit 2.
    ///
    /// **Validates: Requirements 4.9, 4.10**
    #[test]
    fn unavailable_skip_without_fail_produces_exit_two(
        rows in arb_rows().prop_filter("has unavailable skip, no fail", |rows| {
            has_unavailable_skip(rows) && !has_fail(rows)
        }),
        filter in arb_filter().prop_filter("filter must hit or be None", |f| {
            // Use None to avoid filter-miss interference
            f.is_none()
        }),
    ) {
        let code = aggregate_exit_code(&rows, filter.as_deref());
        prop_assert!(
            code == 2,
            "expected exit 2 for unavailable-skip rows (no fail), got {}",
            code
        );
    }
}
