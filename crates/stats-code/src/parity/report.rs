//! Parity Validation Report data types and exit-code aggregation.
//!
//! Feature: parity-and-multilang-sidecar (task 9.5).
//! _Requirements: 4.5, 4.9, 4.10, 5.4, 5.7_
//!
//! This module defines the [`ParityReportRow`] and [`ParityVerdict`] types
//! from design.md §"Data Models", plus the [`aggregate_exit_code`] function
//! that maps a completed parity run (rows + optional filter) onto the
//! deterministic exit-code surface defined in design.md §6.
//!
//! The exit-code contract:
//!
//! | exit | meaning                                                       |
//! |------|---------------------------------------------------------------|
//! | `0`  | no `fail` rows, no `skipped` rows with reason                |
//! |      | `"reference_software_unavailable"`, and filter (if any) hit   |
//! | `2`  | at least one `fail` row                                       |
//! | `3`  | `--filter` did not match any row's `algorithm_id`             |
//!
//! Exit code `2` takes priority over `3` when both conditions hold
//! (a fail row is always surfaced).

use serde::{Deserialize, Serialize};

use crate::coverage_matrix::ReferenceSoftware;

/// Verdict for a single row in the Parity Validation Report.
///
/// _Requirements: 3.2, 4.5, 4.9_
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParityVerdict {
    Pass,
    Fail,
    Skipped,
}

/// A single row in the Parity Validation Report.
///
/// Corresponds to design.md §"Data Models" → `ParityReportRow`.
/// Each row represents one (algorithm, software, test case, metric) tuple.
///
/// _Requirements: 3.2, 3.3, 4.5, 4.9, 4.10, 5.4_
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParityReportRow {
    pub algorithm_id: String,
    pub algorithm_display_name: String,
    pub software: ReferenceSoftware,
    pub case_id: String,
    pub metric: String,
    pub stats_engine_value: f64,
    pub reference_value_or_na: Option<f64>,
    pub absolute_difference: Option<f64>,
    pub relative_difference: Option<f64>,
    pub active_absolute_tolerance: f64,
    pub active_relative_tolerance: f64,
    pub verdict: ParityVerdict,
    /// Reason for `Skipped` verdict. The sentinel value
    /// `"reference_software_unavailable"` triggers a non-zero exit code
    /// per Requirements 4.9, 4.10.
    pub skipped_reason: Option<String>,
}

/// The reason string that triggers non-zero exit when a row is skipped.
///
/// Per Requirements 4.9, 4.10: if any row is skipped because the reference
/// software is unavailable, the suite must exit non-zero.
pub const REASON_REFERENCE_UNAVAILABLE: &str = "reference_software_unavailable";

/// Aggregate a completed parity run into a single exit code.
///
/// # Arguments
///
/// - `rows` — all rows produced by the parity suite run.
/// - `filter` — the `--filter <algorithm_id>` value, if supplied.
///
/// # Exit-code contract
///
/// - `0` — no `fail` rows AND no `skipped` rows with
///   `skipped_reason == "reference_software_unavailable"` AND (filter is
///   `None` OR filter matches at least one row's `algorithm_id`).
/// - `2` — at least one `fail` row (Requirement 4.5, 5.4).
/// - `3` — `--filter` was supplied but did not match any row's
///   `algorithm_id` (Requirement 5.7). Note: if there are also `fail`
///   rows, exit `2` takes priority.
///
/// _Requirements: 4.5, 4.9, 4.10, 5.4, 5.7_
#[must_use]
pub fn aggregate_exit_code(rows: &[ParityReportRow], filter: Option<&str>) -> i32 {
    // Priority 1: any fail row ⇒ exit 2
    let has_fail = rows.iter().any(|r| r.verdict == ParityVerdict::Fail);
    if has_fail {
        return 2;
    }

    // Priority 2: filter supplied but no row matches ⇒ exit 3
    if let Some(f) = filter {
        let filter_hit = rows.iter().any(|r| r.algorithm_id == f);
        if !filter_hit {
            return 3;
        }
    }

    // Priority 3: any skipped row with reason "reference_software_unavailable" ⇒ exit 2
    // (Design.md §6 maps this to non-zero; per Requirement 4.10 the suite
    // exits non-zero. We use exit 2 as the "failure" bucket since the
    // unavailability is treated as a blocking condition.)
    let has_unavailable_skip = rows.iter().any(|r| {
        r.verdict == ParityVerdict::Skipped
            && r.skipped_reason.as_deref() == Some(REASON_REFERENCE_UNAVAILABLE)
    });
    if has_unavailable_skip {
        return 2;
    }

    // All clear
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_row(verdict: ParityVerdict, skipped_reason: Option<&str>) -> ParityReportRow {
        ParityReportRow {
            algorithm_id: "tableone".to_string(),
            algorithm_display_name: "Table One".to_string(),
            software: ReferenceSoftware::R,
            case_id: "case_1".to_string(),
            metric: "mean".to_string(),
            stats_engine_value: 1.0,
            reference_value_or_na: Some(1.0),
            absolute_difference: Some(0.0),
            relative_difference: Some(0.0),
            active_absolute_tolerance: 1e-9,
            active_relative_tolerance: 1e-6,
            verdict,
            skipped_reason: skipped_reason.map(|s| s.to_string()),
        }
    }

    #[test]
    fn all_pass_returns_zero() {
        let rows = vec![make_row(ParityVerdict::Pass, None)];
        assert_eq!(aggregate_exit_code(&rows, None), 0);
    }

    #[test]
    fn empty_rows_no_filter_returns_zero() {
        assert_eq!(aggregate_exit_code(&[], None), 0);
    }

    #[test]
    fn fail_row_returns_two() {
        let rows = vec![
            make_row(ParityVerdict::Pass, None),
            make_row(ParityVerdict::Fail, None),
        ];
        assert_eq!(aggregate_exit_code(&rows, None), 2);
    }

    #[test]
    fn filter_miss_returns_three() {
        let rows = vec![make_row(ParityVerdict::Pass, None)];
        assert_eq!(aggregate_exit_code(&rows, Some("nonexistent")), 3);
    }

    #[test]
    fn filter_hit_all_pass_returns_zero() {
        let rows = vec![make_row(ParityVerdict::Pass, None)];
        assert_eq!(aggregate_exit_code(&rows, Some("tableone")), 0);
    }

    #[test]
    fn fail_takes_priority_over_filter_miss() {
        let mut rows = vec![make_row(ParityVerdict::Fail, None)];
        rows[0].algorithm_id = "cox".to_string();
        // filter "nonexistent" misses, but fail row takes priority
        assert_eq!(aggregate_exit_code(&rows, Some("nonexistent")), 2);
    }

    #[test]
    fn skipped_reference_unavailable_returns_two() {
        let rows = vec![make_row(
            ParityVerdict::Skipped,
            Some(REASON_REFERENCE_UNAVAILABLE),
        )];
        assert_eq!(aggregate_exit_code(&rows, None), 2);
    }

    #[test]
    fn skipped_other_reason_returns_zero() {
        let rows = vec![make_row(ParityVerdict::Skipped, Some("other_reason"))];
        assert_eq!(aggregate_exit_code(&rows, None), 0);
    }
}
