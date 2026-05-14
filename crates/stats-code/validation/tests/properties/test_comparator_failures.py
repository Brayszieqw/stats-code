# Feature: validation-correctness, Property 4: Failure Result Completeness
"""
Property 4: For any inputs to compare_scalar, if status == FAIL then
expected, actual, difference, tolerance, and message are all populated.
"""
import math

import pytest
from hypothesis import given, settings
from hypothesis import strategies as st

from parity.common import compare_scalar
from parity.result import Status, ToleranceConfig


@given(
    expected=st.floats(allow_nan=False, allow_infinity=False, width=32),
    actual=st.floats(allow_nan=False, allow_infinity=False, width=32),
    tol=st.floats(min_value=0.0, max_value=1.0, allow_nan=False, allow_infinity=False),
)
@settings(max_examples=200)
def test_failure_result_completeness(expected: float, actual: float, tol: float) -> None:
    """Property 4: FAIL results must have all fields populated."""
    cfg = ToleranceConfig(per_metric={"dummy.metric": tol})
    r = compare_scalar("dummy", "metric", "d.csv", "ref", expected, actual, cfg)

    if r.status == Status.FAIL:
        assert r.expected == expected, "expected field must equal input expected"
        assert r.actual == actual, "actual field must equal input actual"
        assert r.difference == pytest.approx(abs(expected - actual), abs=1e-15), (
            "difference must equal |expected - actual|"
        )
        assert r.tolerance == tol, "tolerance field must equal input tolerance"
        assert r.message != "", "message must be non-empty for FAIL"


@given(
    expected=st.floats(allow_nan=False, allow_infinity=False, width=32),
    actual=st.floats(allow_nan=False, allow_infinity=False, width=32),
    tol=st.floats(min_value=0.0, max_value=1.0, allow_nan=False, allow_infinity=False),
)
@settings(max_examples=200)
def test_pass_result_has_zero_or_small_difference(
    expected: float, actual: float, tol: float
) -> None:
    """Property 4 (corollary): PASS results have difference ≤ tolerance."""
    cfg = ToleranceConfig(per_metric={"dummy.metric": tol})
    r = compare_scalar("dummy", "metric", "d.csv", "ref", expected, actual, cfg)

    if r.status == Status.PASS:
        assert r.difference is not None
        assert r.difference <= tol + 1e-15, (
            f"PASS result has difference {r.difference} > tolerance {tol}"
        )
