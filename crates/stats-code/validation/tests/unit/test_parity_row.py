"""Unit tests for ParityRow / ParityVerdict / SkippedReason / compute_differences.

Covers task 11.1 of the parity-and-multilang-sidecar spec.
Validates Requirements 3.2, 3.3, 12.5.
"""

from __future__ import annotations

import math
from dataclasses import FrozenInstanceError, fields

import pytest

from parity.result import (
    ParityRow,
    ParityVerdict,
    ReferenceImplDescriptor,
    SkippedReason,
    compute_differences,
)


# ---------------------------------------------------------------------------
# Enum value contracts (Requirement 3.3, 4.10)
# ---------------------------------------------------------------------------

def test_parity_verdict_values_match_spec():
    """Verdict set is exactly {pass, fail, skipped} per Requirement 3.3."""
    assert {v.value for v in ParityVerdict} == {"pass", "fail", "skipped"}
    assert ParityVerdict.PASS.value == "pass"
    assert ParityVerdict.FAIL.value == "fail"
    assert ParityVerdict.SKIPPED.value == "skipped"


def test_skipped_reason_values_match_spec():
    """Skipped reasons must distinguish unavailable reference from uncovered cell."""
    assert {r.value for r in SkippedReason} == {
        "reference_software_unavailable",
        "uncovered_cell",
    }
    assert (
        SkippedReason.REFERENCE_SOFTWARE_UNAVAILABLE.value
        == "reference_software_unavailable"
    )
    assert SkippedReason.UNCOVERED_CELL.value == "uncovered_cell"


def test_parity_verdict_is_string_enum():
    """ParityVerdict must compare equal to its string form so JSON serialisation
    matches the Rust `#[serde(rename_all = "snake_case")]` contract."""
    assert ParityVerdict.PASS == "pass"
    assert ParityVerdict.FAIL == "fail"
    assert ParityVerdict.SKIPPED == "skipped"


# ---------------------------------------------------------------------------
# ParityRow shape (Requirement 3.2)
# ---------------------------------------------------------------------------

EXPECTED_FIELDS = {
    "algorithm_id",
    "algorithm_display_name",
    "software",
    "reference_impl",
    "case_id",
    "metric",
    "stats_engine_value",
    "reference_value_or_na",
    "absolute_difference",
    "relative_difference",
    "active_absolute_tolerance",
    "active_relative_tolerance",
    "verdict",
    "skipped_reason",
}


def _make_row(**overrides) -> ParityRow:
    """Construct a fully populated ParityRow with sane defaults."""
    base = dict(
        algorithm_id="logistic",
        algorithm_display_name="Logistic Regression",
        software="R",
        reference_impl=ReferenceImplDescriptor(
            name="stats::glm",
            pkg="stats",
            version="4.4.1",
        ),
        case_id="synthetic_n100_seed42",
        metric="beta[age]",
        stats_engine_value=0.123456789012,
        reference_value_or_na=0.123456789013,
        absolute_difference=1.0e-12,
        relative_difference=8.1e-12,
        active_absolute_tolerance=1e-9,
        active_relative_tolerance=1e-6,
        verdict=ParityVerdict.PASS,
        skipped_reason=None,
    )
    base.update(overrides)
    return ParityRow(**base)


def test_parity_row_has_exact_field_set():
    """Field set must match the Requirement 3.2 contract verbatim."""
    actual = {f.name for f in fields(ParityRow)}
    assert actual == EXPECTED_FIELDS


def test_parity_row_can_be_built_with_full_payload():
    row = _make_row()
    assert row.algorithm_id == "logistic"
    assert row.software == "R"
    assert row.reference_impl.name == "stats::glm"
    assert row.reference_impl.pkg == "stats"
    assert row.verdict is ParityVerdict.PASS
    assert row.skipped_reason is None


def test_parity_row_skipped_form_carries_reason():
    """A skipped row may have None numeric fields and a reason set."""
    row = _make_row(
        software="SAS",
        reference_impl=ReferenceImplDescriptor(
            name="PROC LOGISTIC", pkg=None, version="9.4M8"
        ),
        stats_engine_value=None,
        reference_value_or_na=None,
        absolute_difference=None,
        relative_difference=None,
        verdict=ParityVerdict.SKIPPED,
        skipped_reason=SkippedReason.REFERENCE_SOFTWARE_UNAVAILABLE,
    )
    assert row.verdict is ParityVerdict.SKIPPED
    assert row.skipped_reason is SkippedReason.REFERENCE_SOFTWARE_UNAVAILABLE
    assert row.reference_impl.pkg is None


def test_parity_row_is_frozen():
    """Rows must be immutable so the reporter can hash / dedupe them."""
    row = _make_row()
    with pytest.raises(FrozenInstanceError):
        row.verdict = ParityVerdict.FAIL  # type: ignore[misc]


def test_parity_row_is_hashable():
    """Frozen dataclasses with hashable members must themselves be hashable."""
    row1 = _make_row()
    row2 = _make_row()
    # equal rows hash equally
    assert hash(row1) == hash(row2)
    # set membership works
    assert {row1, row2} == {row1}


def test_reference_impl_descriptor_is_frozen():
    """The descriptor is shared by reference / deduplicated by reporter."""
    desc = ReferenceImplDescriptor(name="x", pkg="y", version="1.0")
    with pytest.raises(FrozenInstanceError):
        desc.version = "2.0"  # type: ignore[misc]


# ---------------------------------------------------------------------------
# compute_differences (Requirement 3.3, 12.5)
# ---------------------------------------------------------------------------

def test_compute_differences_returns_pair_of_none_when_reference_is_none():
    """No reference numeric => both differences are 'n/a'."""
    assert compute_differences(0.5, None, abs_tol=1e-9) == (None, None)
    assert compute_differences(0.0, None, abs_tol=0.0) == (None, None)


def test_compute_differences_relative_is_na_when_reference_at_or_below_abs_tol():
    """|reference| <= abs_tol => relative_difference is None per Requirement 3.3."""
    abs_tol = 1e-9

    # |reference| strictly below abs_tol
    abs_diff, rel_diff = compute_differences(1e-12, 5e-10, abs_tol=abs_tol)
    assert abs_diff == pytest.approx(abs(1e-12 - 5e-10))
    assert rel_diff is None

    # |reference| exactly at abs_tol — boundary is inclusive, still 'n/a'
    abs_diff, rel_diff = compute_differences(0.0, abs_tol, abs_tol=abs_tol)
    assert abs_diff == abs_tol
    assert rel_diff is None

    # negative reference at boundary still triggers the 'n/a' rule
    abs_diff, rel_diff = compute_differences(0.0, -abs_tol, abs_tol=abs_tol)
    assert abs_diff == abs_tol
    assert rel_diff is None


def test_compute_differences_relative_is_ratio_when_reference_above_abs_tol():
    """|reference| > abs_tol => relative_difference = abs_diff / |reference|."""
    abs_diff, rel_diff = compute_differences(1.0, 2.0, abs_tol=1e-9)
    assert abs_diff == pytest.approx(1.0)
    assert rel_diff == pytest.approx(0.5)

    # negative reference: division uses |reference|
    abs_diff, rel_diff = compute_differences(1.0, -4.0, abs_tol=1e-9)
    assert abs_diff == pytest.approx(5.0)
    assert rel_diff == pytest.approx(5.0 / 4.0)


def test_compute_differences_zero_difference_yields_zero_relative():
    """When stats == reference, both differences are zero (not 'n/a')."""
    abs_diff, rel_diff = compute_differences(2.0, 2.0, abs_tol=1e-9)
    assert abs_diff == 0.0
    assert rel_diff == 0.0


def test_compute_differences_handles_realistic_logistic_regression_numbers():
    """End-to-end: simulate a typical logistic-regression beta comparison."""
    stats = 0.7234567890123
    reference = 0.7234567890456
    abs_tol = 1e-9

    abs_diff, rel_diff = compute_differences(stats, reference, abs_tol=abs_tol)
    assert abs_diff == pytest.approx(abs(stats - reference))
    assert rel_diff is not None
    assert rel_diff == pytest.approx(abs_diff / abs(reference))
    # row-level fail predicate should NOT trip
    assert not (abs_diff > abs_tol and rel_diff > 1e-6)


def test_compute_differences_does_not_blow_up_on_negative_zero_reference():
    """-0.0 is treated as 0.0 by `abs`, so the 'n/a' rule applies."""
    abs_diff, rel_diff = compute_differences(0.0, -0.0, abs_tol=1e-9)
    assert abs_diff == 0.0
    assert rel_diff is None


def test_compute_differences_finite_inputs_produce_finite_outputs():
    """Sanity: finite inputs above the tolerance band yield finite outputs."""
    abs_diff, rel_diff = compute_differences(1.5e10, 1.5e10 + 1.0, abs_tol=1e-9)
    assert math.isfinite(abs_diff)
    assert rel_diff is not None
    assert math.isfinite(rel_diff)
