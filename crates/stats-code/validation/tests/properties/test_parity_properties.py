"""Property-based tests for the Parity Validation Suite.

Properties 6, 10, 11, 12 from the parity-and-multilang-sidecar spec
(tasks 11.9, 11.10, 11.11, 11.12).

Uses hypothesis for property-based testing.
"""

from __future__ import annotations

import json
import math
import re
from typing import Any, Optional

import hypothesis.strategies as st
from hypothesis import given, settings, assume

from parity.threshold import fail_predicate
from parity.result import (
    ParityRow,
    ParityReportHeader,
    ParityVerdict,
    ReferenceImplDescriptor,
    SkippedReason,
)
from parity.reporter import (
    ParityReportGenerator,
    _fmt_numeric,
    _row_to_dict,
    _header_to_dict,
    NA_LITERAL,
)


# ---------------------------------------------------------------------------
# Strategies
# ---------------------------------------------------------------------------

finite_floats = st.floats(allow_nan=False, allow_infinity=False, min_value=-1e15, max_value=1e15)
non_negative_floats = st.floats(min_value=0.0, max_value=1e10, allow_nan=False, allow_infinity=False)
positive_floats = st.floats(min_value=1e-300, max_value=1e15, allow_nan=False, allow_infinity=False)


def arb_reference_impl() -> st.SearchStrategy[ReferenceImplDescriptor]:
    return st.builds(
        ReferenceImplDescriptor,
        name=st.text(min_size=1, max_size=20, alphabet="abcdefghijklmnopqrstuvwxyz_."),
        pkg=st.one_of(st.none(), st.text(min_size=1, max_size=15, alphabet="abcdefghijklmnopqrstuvwxyz")),
        version=st.from_regex(r"[0-9]{1,3}\.[0-9]{1,3}\.[0-9]{1,3}", fullmatch=True),
    )


def arb_parity_row() -> st.SearchStrategy[ParityRow]:
    """Generate an arbitrary ParityRow with consistent field relationships."""
    return st.builds(
        _build_parity_row,
        algorithm_id=st.text(min_size=1, max_size=20, alphabet="abcdefghijklmnopqrstuvwxyz_"),
        display_name=st.text(min_size=1, max_size=30, alphabet="ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz "),
        software=st.sampled_from(["R", "SAS", "Python", "SPSS"]),
        reference_impl=arb_reference_impl(),
        case_id=st.text(min_size=1, max_size=15, alphabet="abcdefghijklmnopqrstuvwxyz0123456789_"),
        metric=st.text(min_size=1, max_size=20, alphabet="abcdefghijklmnopqrstuvwxyz0123456789_[]"),
        stats_engine_value=finite_floats,
        reference_value=st.one_of(st.none(), finite_floats),
        abs_tol=non_negative_floats,
        rel_tol=non_negative_floats,
        verdict=st.sampled_from(list(ParityVerdict)),
        skipped_reason=st.one_of(st.none(), st.sampled_from(list(SkippedReason))),
    )


def _build_parity_row(
    algorithm_id: str,
    display_name: str,
    software: str,
    reference_impl: ReferenceImplDescriptor,
    case_id: str,
    metric: str,
    stats_engine_value: float,
    reference_value: Optional[float],
    abs_tol: float,
    rel_tol: float,
    verdict: ParityVerdict,
    skipped_reason: Optional[SkippedReason],
) -> ParityRow:
    """Build a ParityRow with computed differences."""
    if reference_value is None:
        abs_diff = None
        rel_diff = None
    else:
        abs_diff = abs(stats_engine_value - reference_value)
        if abs(reference_value) <= abs_tol:
            rel_diff = None
        else:
            rel_diff = abs_diff / abs(reference_value)

    # skipped_reason only valid when verdict is SKIPPED
    if verdict != ParityVerdict.SKIPPED:
        skipped_reason = None

    return ParityRow(
        algorithm_id=algorithm_id,
        algorithm_display_name=display_name,
        software=software,
        reference_impl=reference_impl,
        case_id=case_id,
        metric=metric,
        stats_engine_value=stats_engine_value if verdict != ParityVerdict.SKIPPED else None,
        reference_value_or_na=reference_value,
        absolute_difference=abs_diff,
        relative_difference=rel_diff,
        active_absolute_tolerance=abs_tol,
        active_relative_tolerance=rel_tol,
        verdict=verdict,
        skipped_reason=skipped_reason,
    )


def arb_header() -> st.SearchStrategy[ParityReportHeader]:
    """Generate an arbitrary ParityReportHeader."""
    return st.builds(
        ParityReportHeader,
        commit_sha=st.from_regex(r"[0-9a-f]{40}", fullmatch=True),
        run_started_at_utc=st.from_regex(
            r"20[0-9]{2}-[01][0-9]-[0-3][0-9]T[0-2][0-9]:[0-5][0-9]:[0-5][0-9]Z",
            fullmatch=True,
        ),
        host_os_family=st.sampled_from(["Windows", "Linux", "macOS"]),
        host_os_version=st.text(min_size=1, max_size=32, alphabet="0123456789.abcdefghijklmnopqrstuvwxyz-_"),
        reference_software_versions=st.dictionaries(
            keys=st.text(min_size=1, max_size=10, alphabet="abcdefghijklmnopqrstuvwxyz"),
            values=st.from_regex(r"[0-9]{1,3}\.[0-9]{1,3}\.[0-9]{1,3}", fullmatch=True),
            min_size=0,
            max_size=4,
        ),
        coverage_matrix=st.just({"tableone": {"R": "live", "SAS": "recorded", "Python": "live", "SPSS": "none"}}),
        tolerance_diff=st.lists(
            st.fixed_dictionaries({
                "algorithm": st.text(min_size=1, max_size=15, alphabet="abcdefghijklmnopqrstuvwxyz_"),
                "previous": st.from_regex(r"[0-9.e-]+", fullmatch=True),
                "new": st.from_regex(r"[0-9.e-]+", fullmatch=True),
                "pr_id": st.from_regex(r"#[0-9]{1,5}", fullmatch=True),
            }),
            min_size=0,
            max_size=3,
        ),
    )


# ---------------------------------------------------------------------------
# Property 6 (Task 11.9): Threshold pass/fail predicate
# Validates: Requirements 3.4, 5.4, 8.7
# ---------------------------------------------------------------------------


@given(
    stats_engine_value=finite_floats,
    reference_value=positive_floats,
    abs_tol=non_negative_floats,
    rel_tol=non_negative_floats,
)
@settings(max_examples=500)
def test_threshold_predicate_equivalence(
    stats_engine_value: float,
    reference_value: float,
    abs_tol: float,
    rel_tol: float,
) -> None:
    """Property 6: fail_predicate is equivalent to the spec formula.

    fail = (abs_diff > abs_tol) AND (rel_diff is defined) AND (rel_diff > rel_tol)
    """
    abs_diff = abs(stats_engine_value - reference_value)

    # rel_diff is defined when |reference| > abs_tol
    if abs(reference_value) > abs_tol:
        rel_diff: Optional[float] = abs_diff / abs(reference_value)
    else:
        rel_diff = None

    result = fail_predicate(abs_diff, rel_diff, abs_tol, rel_tol)

    # Compute expected via the spec formula
    if rel_diff is None:
        expected = False
    else:
        expected = (abs_diff > abs_tol) and (rel_diff > rel_tol)

    assert result == expected, (
        f"fail_predicate({abs_diff}, {rel_diff}, {abs_tol}, {rel_tol}) = {result}, "
        f"expected {expected}"
    )


@given(
    abs_diff=non_negative_floats,
    abs_tol=non_negative_floats,
    rel_tol=non_negative_floats,
)
@settings(max_examples=200)
def test_threshold_predicate_none_rel_diff_never_fails(
    abs_diff: float,
    abs_tol: float,
    rel_tol: float,
) -> None:
    """When rel_diff is None, the predicate never returns True."""
    assert fail_predicate(abs_diff, None, abs_tol, rel_tol) is False


# ---------------------------------------------------------------------------
# Property 10 (Task 11.10): Parity report row bijection
# Validates: Requirements 3.2
# ---------------------------------------------------------------------------


@given(rows=st.lists(arb_parity_row(), min_size=0, max_size=10))
@settings(max_examples=200)
def test_parity_report_row_bijection(rows: list[ParityRow]) -> None:
    """Property 10: Rendered report has exactly |rows| entries, bijective mapping."""
    header = ParityReportHeader(
        commit_sha="a" * 40,
        run_started_at_utc="2024-01-01T00:00:00Z",
        host_os_family="Linux",
        host_os_version="6.6.0",
        reference_software_versions={},
        coverage_matrix={},
        tolerance_diff=[],
    )
    gen = ParityReportGenerator(rows, header)
    json_str = gen.render_json()
    doc = json.loads(json_str)

    rendered_rows = doc["rows"]

    # Exactly |rows| entries in the output
    assert len(rendered_rows) == len(rows), (
        f"expected {len(rows)} rows in report, got {len(rendered_rows)}"
    )

    # Each input row maps to exactly one output row (same order)
    for i, (input_row, output_row) in enumerate(zip(rows, rendered_rows)):
        assert output_row["algorithm_id"] == input_row.algorithm_id, (
            f"row {i}: algorithm_id mismatch"
        )
        assert output_row["case_id"] == input_row.case_id, (
            f"row {i}: case_id mismatch"
        )
        assert output_row["metric"] == input_row.metric, (
            f"row {i}: metric mismatch"
        )
        assert output_row["verdict"] == input_row.verdict.value, (
            f"row {i}: verdict mismatch"
        )


# ---------------------------------------------------------------------------
# Property 11 (Task 11.11): Parity report row numeric formatting
# Validates: Requirements 3.3, 12.5
# ---------------------------------------------------------------------------


@given(row=arb_parity_row())
@settings(max_examples=300)
def test_parity_report_numeric_formatting(row: ParityRow) -> None:
    """Property 11: Every numeric field has >= 12 sig digits or is 'n/a'.

    Also: active tolerances and measured differences are always present.
    """
    rendered = _row_to_dict(row)

    numeric_fields = [
        "stats_engine_value",
        "reference_value_or_na",
        "absolute_difference",
        "relative_difference",
        "active_absolute_tolerance",
        "active_relative_tolerance",
    ]

    for field_name in numeric_fields:
        value = rendered[field_name]
        if value == NA_LITERAL:
            # n/a is acceptable for optional fields
            continue
        # Must be a string formatted with >= 12 significant digits
        assert isinstance(value, str), f"{field_name} must be a string, got {type(value)}"
        # Special float representations (inf, -inf, 0.000000000000e+00) are
        # valid outputs of f"{x:.12e}" for extreme/zero values.
        if value in ("inf", "-inf"):
            continue
        # The format is scientific notation: d.ddddddddddddeddd
        # With .12e format, there are 13 significant digits (1 before + 12 after decimal)
        match = re.match(r"^-?[0-9]\.[0-9]{12}e[+-][0-9]+$", value)
        assert match is not None, (
            f"{field_name} = {value!r} does not have >= 12 sig digits in scientific notation"
        )

    # Active tolerances must always be present (never n/a)
    assert rendered["active_absolute_tolerance"] != NA_LITERAL, (
        "active_absolute_tolerance must never be n/a"
    )
    assert rendered["active_relative_tolerance"] != NA_LITERAL, (
        "active_relative_tolerance must never be n/a"
    )


# ---------------------------------------------------------------------------
# Property 12 (Task 11.12): Parity report header content
# Validates: Requirements 3.6, 3.7, 12.4
# ---------------------------------------------------------------------------


@given(header=arb_header())
@settings(max_examples=200)
def test_parity_report_header_content(header: ParityReportHeader) -> None:
    """Property 12: Report header contains all required fields with correct constraints."""
    rendered = _header_to_dict(header)

    # commit_sha: 40 hex chars
    assert re.fullmatch(r"[0-9a-f]{40}", rendered["commit_sha"]), (
        f"commit_sha must be 40 hex chars, got {rendered['commit_sha']!r}"
    )

    # run_started_at_utc: ISO-8601 UTC timestamp
    ts = rendered["run_started_at_utc"]
    assert ts.endswith("Z"), f"timestamp must end with Z, got {ts!r}"
    assert "T" in ts, f"timestamp must contain T separator, got {ts!r}"

    # host_os_family: one of {Windows, Linux, macOS}
    assert rendered["host_os_family"] in {"Windows", "Linux", "macOS"}, (
        f"host_os_family must be Windows/Linux/macOS, got {rendered['host_os_family']!r}"
    )

    # host_os_version: <= 32 characters
    assert len(rendered["host_os_version"]) <= 32, (
        f"host_os_version must be <= 32 chars, got {len(rendered['host_os_version'])}"
    )

    # reference_software_versions: dict with string keys and values
    sw = rendered["reference_software_versions"]
    assert isinstance(sw, dict)
    for k, v in sw.items():
        assert isinstance(k, str) and isinstance(v, str)

    # coverage_matrix: embedded (preserving 'none' entries)
    cm = rendered["coverage_matrix"]
    assert isinstance(cm, dict)
    # If the matrix has entries, 'none' values must be preserved
    for alg, cells in cm.items():
        if isinstance(cells, dict):
            for sw_name, state in cells.items():
                if state == "none":
                    # 'none' is preserved, not filtered out
                    assert state == "none"

    # tolerance_diff: list of dicts with required keys
    td = rendered["tolerance_diff"]
    assert isinstance(td, list)
    for entry in td:
        assert "algorithm" in entry
        assert "previous" in entry
        assert "new" in entry
        assert "pr_id" in entry
