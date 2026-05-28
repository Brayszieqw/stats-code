"""Pure threshold predicate shared by the CI Parity Suite and the
``--replay`` numeric drift gate.

This module is intentionally dependency-free and side-effect-free: it must be
safely importable from both the parity reporter (task 11.4 / 11.5) and the
``--replay`` numeric drift gate invoked by the snapshot exporter (task 7.2).

Validates Requirements 3.4, 5.4, 8.7.
"""

from __future__ import annotations

from typing import Optional


def fail_predicate(
    abs_diff: float,
    rel_diff: Optional[float],
    abs_tol: float,
    rel_tol: float,
) -> bool:
    """Return ``True`` iff the parity row should be marked ``fail``.

    Spec rule (Requirements 3.4 / 5.4 / 8.7)::

        fail = (abs_diff > abs_tol)
               AND (rel_diff is defined)
               AND (rel_diff > rel_tol)

    Where "rel_diff is defined" means ``rel_diff is not None``. Per
    Requirement 3.3, the relative difference is rendered as the literal
    ``n/a`` (and represented here as ``None``) whenever the reference
    magnitude is at or below ``abs_tol``; in that case the row never fails
    on the relative test, so the conjunction short-circuits to ``False``.

    The predicate is reused verbatim by:

    * the CI Parity Suite reporter (task 11.4) when assigning the
      ``pass``/``fail`` verdict to a row whose reference value is
      available;
    * the ``stats-code --replay`` numeric drift gate (task 7.2) when
      checking that a re-executed step's output still matches the snapshot
      within the active per-algorithm Parity Threshold.

    Args:
        abs_diff: Absolute difference ``|engine - reference|``. Must be
            non-negative; this is enforced by the caller (the reporter and
            the replay gate both compute it via ``abs(...)``).
        rel_diff: Relative difference ``abs_diff / |reference|`` when the
            reference magnitude exceeds ``abs_tol``; ``None`` when the
            reference magnitude is at or below ``abs_tol`` (the ``n/a``
            case from Requirement 3.3).
        abs_tol: Active absolute tolerance for this algorithm/metric. The
            spec requires this to be non-negative; validation lives in the
            tolerance loader (task 14.x), not in this hot-path predicate.
        rel_tol: Active relative tolerance for this algorithm/metric. As
            with ``abs_tol``, non-negativity is caller-validated.

    Returns:
        ``True`` if and only if the row's verdict is ``fail``.

    Notes:
        * The comparisons use strict ``>`` per the spec wording "exceeds";
          a difference exactly equal to its tolerance does **not** trip
          the gate.
        * When ``rel_diff is None`` the row never fails on the relative
          side; the absolute side alone is not sufficient to fail the row
          (Requirement 3.4 conjoins absolute and relative).
    """
    if rel_diff is None:
        return False
    return abs_diff > abs_tol and rel_diff > rel_tol
