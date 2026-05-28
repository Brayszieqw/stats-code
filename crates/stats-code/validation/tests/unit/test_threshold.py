"""Unit tests for the pure parity-fail predicate ``parity.threshold.fail_predicate``.

Covers task 11.3 of the parity-and-multilang-sidecar spec.
Validates Requirements 3.4, 5.4, 8.7.

The same predicate is exercised by the CI Parity Suite reporter
(task 11.4) and by the ``stats-code --replay`` numeric drift gate
(task 7.2); both consumers must observe identical ``pass`` / ``fail``
decisions for any given ``(abs_diff, rel_diff, abs_tol, rel_tol)``
4-tuple.
"""

from __future__ import annotations

from parity.threshold import fail_predicate


# ---------------------------------------------------------------------------
# Core conjunction (Requirement 3.4)
# ---------------------------------------------------------------------------

def test_fails_when_both_diffs_exceed_their_tolerances():
    """Both abs_diff > abs_tol AND rel_diff > rel_tol => fail."""
    assert fail_predicate(
        abs_diff=1e-8,
        rel_diff=1e-5,
        abs_tol=1e-9,
        rel_tol=1e-6,
    ) is True


def test_passes_when_only_absolute_side_trips():
    """abs_diff > abs_tol but rel_diff <= rel_tol => the conjunction is False."""
    # rel_diff strictly below rel_tol
    assert fail_predicate(
        abs_diff=1e-8,
        rel_diff=1e-7,
        abs_tol=1e-9,
        rel_tol=1e-6,
    ) is False

    # rel_diff exactly equal to rel_tol — strict ``>`` does not trip
    assert fail_predicate(
        abs_diff=1e-8,
        rel_diff=1e-6,
        abs_tol=1e-9,
        rel_tol=1e-6,
    ) is False


def test_passes_when_only_relative_side_trips():
    """rel_diff > rel_tol but abs_diff <= abs_tol => the conjunction is False."""
    # abs_diff strictly below abs_tol
    assert fail_predicate(
        abs_diff=1e-12,
        rel_diff=1e-5,
        abs_tol=1e-9,
        rel_tol=1e-6,
    ) is False

    # abs_diff exactly equal to abs_tol — strict ``>`` does not trip
    assert fail_predicate(
        abs_diff=1e-9,
        rel_diff=1e-5,
        abs_tol=1e-9,
        rel_tol=1e-6,
    ) is False


def test_passes_when_both_within_tolerance():
    """Both differences within their tolerances => pass."""
    assert fail_predicate(
        abs_diff=1e-12,
        rel_diff=1e-9,
        abs_tol=1e-9,
        rel_tol=1e-6,
    ) is False


def test_passes_when_diffs_are_zero():
    """Engine == reference => zero diffs => never fails."""
    assert fail_predicate(
        abs_diff=0.0,
        rel_diff=0.0,
        abs_tol=1e-9,
        rel_tol=1e-6,
    ) is False


# ---------------------------------------------------------------------------
# rel_diff == None (the "n/a" case from Requirement 3.3)
# ---------------------------------------------------------------------------

def test_never_fails_when_rel_diff_is_none_even_if_abs_diff_huge():
    """rel_diff = None propagates through the AND as 'no fail'.

    This is the ``n/a`` branch from Requirement 3.3 (reference magnitude
    at or below abs_tol). Per Requirement 3.4 the row only fails when the
    relative difference is *defined* and exceeds rel_tol, so a None
    rel_diff must short-circuit to False regardless of the absolute side.
    """
    # abs_diff arbitrarily large, rel_diff missing
    assert fail_predicate(
        abs_diff=1.0e6,
        rel_diff=None,
        abs_tol=1e-9,
        rel_tol=1e-6,
    ) is False


def test_never_fails_when_rel_diff_is_none_and_abs_diff_zero():
    """Sanity: rel_diff = None with zero abs_diff is also pass."""
    assert fail_predicate(
        abs_diff=0.0,
        rel_diff=None,
        abs_tol=1e-9,
        rel_tol=1e-6,
    ) is False


def test_rel_diff_none_with_abs_diff_at_boundary_is_pass():
    """rel_diff = None and abs_diff exactly at abs_tol is still pass."""
    assert fail_predicate(
        abs_diff=1e-9,
        rel_diff=None,
        abs_tol=1e-9,
        rel_tol=1e-6,
    ) is False


# ---------------------------------------------------------------------------
# Strict-inequality boundary contract (Requirement 3.4 wording: "exceeds")
# ---------------------------------------------------------------------------

def test_exact_equality_on_both_boundaries_does_not_fail():
    """``>`` is strict on both sides: equality on the boundaries is pass."""
    assert fail_predicate(
        abs_diff=1e-9,
        rel_diff=1e-6,
        abs_tol=1e-9,
        rel_tol=1e-6,
    ) is False


def test_exact_equality_on_abs_boundary_with_rel_above_is_pass():
    """abs_diff == abs_tol does not trip even when rel_diff > rel_tol."""
    assert fail_predicate(
        abs_diff=1e-9,
        rel_diff=2e-6,
        abs_tol=1e-9,
        rel_tol=1e-6,
    ) is False


def test_exact_equality_on_rel_boundary_with_abs_above_is_pass():
    """rel_diff == rel_tol does not trip even when abs_diff > abs_tol."""
    assert fail_predicate(
        abs_diff=2e-9,
        rel_diff=1e-6,
        abs_tol=1e-9,
        rel_tol=1e-6,
    ) is False


def test_just_above_both_boundaries_fails():
    """A hair above both boundaries flips the verdict to fail."""
    assert fail_predicate(
        abs_diff=1.000_001e-9,
        rel_diff=1.000_001e-6,
        abs_tol=1e-9,
        rel_tol=1e-6,
    ) is True


# ---------------------------------------------------------------------------
# Realistic scales (logistic regression beta, Cox HR)
# ---------------------------------------------------------------------------

def test_realistic_logistic_regression_beta_is_pass():
    """Typical logistic-regression beta delta vs default tolerance => pass."""
    # |engine - reference| ~ 3e-11 on a coefficient of magnitude ~0.72
    abs_diff = 3.3e-11
    rel_diff = abs_diff / 0.7234567890456
    assert fail_predicate(
        abs_diff=abs_diff,
        rel_diff=rel_diff,
        abs_tol=1e-9,
        rel_tol=1e-6,
    ) is False


def test_realistic_iterative_algorithm_just_above_tolerance_fails():
    """Iterative algorithm (rel_tol = 1e-4) drifting > 1e-4 in both senses
    must be flagged as a fail."""
    assert fail_predicate(
        abs_diff=2e-3,
        rel_diff=2e-4,
        abs_tol=1e-6,
        rel_tol=1e-4,
    ) is True


# ---------------------------------------------------------------------------
# Caller-validated tolerance contract
# ---------------------------------------------------------------------------

def test_zero_tolerances_are_accepted_as_strict_equality_gate():
    """Spec says tolerances are non-negative; ``0.0`` is a valid (strictest)
    setting and the strict ``>`` makes it equivalent to "exact match required".

    The function does not validate sign; that lives in the tolerance loader
    (task 14.x). This test pins the in-bounds zero behaviour so the replay
    gate (task 7.2) can rely on it.
    """
    # Any positive diff trips the gate
    assert fail_predicate(
        abs_diff=1e-300,
        rel_diff=1e-300,
        abs_tol=0.0,
        rel_tol=0.0,
    ) is True

    # Exact equality (zero diffs) does not trip even at zero tolerance
    assert fail_predicate(
        abs_diff=0.0,
        rel_diff=0.0,
        abs_tol=0.0,
        rel_tol=0.0,
    ) is False
