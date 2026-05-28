"""parity/cases — Hypothesis-driven Live test case definitions per matrix cell.

For every ``(algorithm_id, software)`` cell that the Algorithm Coverage
Matrix marks as ``live`` (Requirement 6.2), this package supplies a
:class:`LiveCase` describing how to drive the existing parity harness:

* a deterministic ``hypothesis`` dataset strategy that yields a small
  synthetic dataset (or, for dataset-free algorithms, the spec only),
* the ``spec`` dict that the corresponding ``parity.<algorithm>.collect``
  consumes,
* an adapter selector that picks the right Reference adapter for the
  cell — Python in-process for the ``Python`` column, Rscript for the
  ``R`` column — so ``collect()`` only runs the Reference path the cell
  is supposed to validate.

The goal of this package is to land the **input definitions** for the
Live arm of the parity suite (task 11.7). The actual parametrized
pytest driver lives in
``crates/stats-code/validation/tests/properties/test_live_parity_cases.py``;
this package is the registry it consumes.

Validates Requirements 4.3, 4.8.
"""

from .registry import LIVE_CASES, LiveCase, live_cells

__all__ = ["LIVE_CASES", "LiveCase", "live_cells"]
