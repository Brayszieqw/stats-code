# Feature: parity-and-multilang-sidecar, Property 6: Threshold pass/fail predicate
"""
Property 6: Threshold pass/fail predicate.

**Validates: Requirements 3.4, 5.4, 8.7**

hypothesis generates (stats_engine_value, reference_value, abs_tol, rel_tol) tuples
with finite floats and non-negative tolerances. Asserts that `fail_predicate` is
equivalent to: (abs_diff > abs_tol) AND (rel_diff is defined) AND (rel_diff > rel_tol).
"""
from typing import Optional

import pytest
from hypothesis import given, settings, assume
from hypothesis import strategies as st

from parity.threshold import fail_predicate


# Strategy: finite floats for values, non-negative finite floats for tolerances
_finite_float = st.floats(allow_nan=False, allow_infinity=False)
_non_negative_float = st.floats(min_value=0.0, allow_nan=False, allow_infinity=False)


def _compute_expected_fail(
    abs_diff: float,
    rel_diff: Optional[float],
    abs_tol: float,
    rel_tol: float,
) -> bool:
    """Reference implementation of the fail predicate formula.

    fail = (abs_diff > abs_tol) AND (rel_diff is defined) AND (rel_diff > rel_tol)
    """
    if rel_diff is None:
        return False
    return abs_diff > abs_tol and rel_diff > rel_tol


@given(
    abs_diff=_non_negative_float,
    rel_diff=st.one_of(st.none(), _non_negative_float),
    abs_tol=_non_negative_float,
    rel_tol=_non_negative_float,
)
@settings(max_examples=500)
def test_fail_predicate_matches_spec_formula(
    abs_diff: float,
    rel_diff: Optional[float],
    abs_tol: float,
    rel_tol: float,
) -> None:
    """Property 6: fail_predicate result equals the manual conjunction formula.

    **Validates: Requirements 3.4, 5.4, 8.7**
    """
    actual = fail_predicate(abs_diff, rel_diff, abs_tol, rel_tol)
    expected = _compute_expected_fail(abs_diff, rel_diff, abs_tol, rel_tol)
    assert actual == expected, (
        f"fail_predicate({abs_diff}, {rel_diff}, {abs_tol}, {rel_tol}) "
        f"returned {actual}, expected {expected}"
    )


@given(
    abs_diff=_non_negative_float,
    abs_tol=_non_negative_float,
    rel_tol=_non_negative_float,
)
@settings(max_examples=200)
def test_fail_predicate_none_rel_diff_never_fails(
    abs_diff: float,
    abs_tol: float,
    rel_tol: float,
) -> None:
    """Property 6: when rel_diff is None (n/a), the predicate always returns False.

    **Validates: Requirements 3.4, 5.4, 8.7**

    Per Requirement 3.3, relative difference is n/a when reference magnitude
    is at or below abs_tol. In that case the conjunction short-circuits.
    """
    assert fail_predicate(abs_diff, None, abs_tol, rel_tol) is False


@given(
    abs_diff=_non_negative_float,
    rel_diff=_non_negative_float,
    abs_tol=_non_negative_float,
    rel_tol=_non_negative_float,
)
@settings(max_examples=200)
def test_fail_predicate_requires_both_thresholds_exceeded(
    abs_diff: float,
    rel_diff: float,
    abs_tol: float,
    rel_tol: float,
) -> None:
    """Property 6: fail requires BOTH abs and rel thresholds exceeded (conjunction).

    **Validates: Requirements 3.4, 5.4, 8.7**

    If only one threshold is exceeded, the predicate must return False.
    """
    result = fail_predicate(abs_diff, rel_diff, abs_tol, rel_tol)

    # If abs_diff <= abs_tol, cannot fail
    if abs_diff <= abs_tol:
        assert result is False, (
            f"abs_diff={abs_diff} <= abs_tol={abs_tol} but predicate returned True"
        )

    # If rel_diff <= rel_tol, cannot fail
    if rel_diff <= rel_tol:
        assert result is False, (
            f"rel_diff={rel_diff} <= rel_tol={rel_tol} but predicate returned True"
        )
