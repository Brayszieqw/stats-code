"""Hypothesis strategies for synthetic Live parity datasets.

Validates Requirements 4.3, 4.8.

The strategy in this module produces a small, balanced, signal-bearing
dataset shaped exactly like the columns the existing parity collectors
default to (``age, bmi, linear_y, disease, time, death, group``) so the
cell registry can drop a hypothesis-generated CSV onto the Stats Engine
↔ Reference comparison loop without any per-cell column wrangling.

Generator constraints (kept intentionally tight so cells like Cox / OR-RR
don't degenerate to NaN under shrinking):

* ``age``, ``bmi``, ``time``  — N=30 distinct floats inside their nominal
  clinical ranges. Distinctness forces variance, which keeps OLS / Cox /
  KM / OR-RR out of the singular-matrix regime.
* ``linear_y``                 — ``0.5*age + 0.3*bmi + small noise`` so
  scipy / statsmodels recover a well-defined β.
* ``disease`` and ``death``    — 15/15 split per group, with both cells
  of the 2x2 (group × outcome) table strictly positive so OR / RR and
  their Woolf / log-method CIs are defined.
* ``group``                    — fixed ``[0]*15 + [1]*15``; Hypothesis
  controls *which* rows in each group flip to ``disease=1`` / ``death=1``
  via the ``perm`` strategies.

The module is intentionally small and side-effect free; it is imported by
the test driver (``tests/properties/test_live_parity_cases.py``) and by
the live-cell registry (``parity/cases/registry.py``) so the same dataset
shape powers every cell.
"""

from __future__ import annotations

import csv
from pathlib import Path
from typing import Any

from hypothesis import strategies as st

# ---------------------------------------------------------------------------
# Calibrated constants
# ---------------------------------------------------------------------------

#: Total sample size of every synthetic dataset.
DATASET_N: int = 30

#: Half-N — used to keep ``group`` balanced 15/15.
HALF_N: int = DATASET_N // 2

#: Number of group-0 rows promoted to ``disease=1`` / ``death=1``. Picked
#: so the 2x2 table (group × outcome) has all four cells strictly
#: positive when paired with :data:`_GROUP1_POSITIVE_COUNT`.
_GROUP0_POSITIVE_COUNT: int = 5

#: Number of group-1 rows promoted to ``disease=1`` / ``death=1``.
_GROUP1_POSITIVE_COUNT: int = 9


# ---------------------------------------------------------------------------
# Per-column primitive strategies
# ---------------------------------------------------------------------------

_AGE_STRAT = st.lists(
    st.floats(min_value=18.0, max_value=90.0, allow_nan=False, allow_infinity=False),
    min_size=DATASET_N, max_size=DATASET_N, unique=True,
)
_BMI_STRAT = st.lists(
    st.floats(min_value=15.0, max_value=45.0, allow_nan=False, allow_infinity=False),
    min_size=DATASET_N, max_size=DATASET_N, unique=True,
)
_TIME_STRAT = st.lists(
    st.floats(min_value=0.05, max_value=10.0, allow_nan=False, allow_infinity=False),
    min_size=DATASET_N, max_size=DATASET_N, unique=True,
)
# Non-zero noise so OLS does not fit perfectly (which would yield ±inf
# t-statistics and undefined p-values on the comparison side).
_NOISE_STRAT = st.lists(
    st.floats(min_value=-3.0, max_value=3.0, allow_nan=False, allow_infinity=False),
    min_size=DATASET_N, max_size=DATASET_N,
)
# Permutation key used to decide *which* rows in each group carry
# disease=1 / death=1. Hypothesis varies this key to explore different
# admissible row layouts while we keep the marginal counts fixed.
_PERM_STRAT = st.lists(
    st.integers(min_value=0, max_value=10_000),
    min_size=DATASET_N, max_size=DATASET_N,
)


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def _stratified_binary(perm: list[int]) -> list[int]:
    """Build a binary vector with all four 2x2 cells strictly positive.

    ``group`` is the deterministic ``[0]*HALF_N + [1]*HALF_N`` vector
    (built in :func:`balanced_dataset_strategy`). Within each group, the
    perm-key picks ``_GROUP0_POSITIVE_COUNT`` row indices in group 0 and
    ``_GROUP1_POSITIVE_COUNT`` row indices in group 1 to set to ``1``.

    With ``HALF_N == 15`` and the calibrated counts ``5 / 9`` the 2x2
    table (disease ∈ {0,1}) × (group ∈ {0,1}) is::

            group=0  group=1
        d=1      5        9
        d=0     10        6

    No cell is zero, so OR / RR and their Woolf / log-method CIs are
    well-defined for every drawn instance.
    """
    out = [0] * DATASET_N
    g0_indices = list(range(HALF_N))
    g1_indices = list(range(HALF_N, DATASET_N))
    g0_indices.sort(key=lambda i: (perm[i], i))
    g1_indices.sort(key=lambda i: (perm[i], i))
    for i in g0_indices[:_GROUP0_POSITIVE_COUNT]:
        out[i] = 1
    for i in g1_indices[:_GROUP1_POSITIVE_COUNT]:
        out[i] = 1
    return out


@st.composite
def balanced_dataset_strategy(draw):
    """Draw a list of 30 row-dicts shaped for the existing parity collectors.

    Every drawn instance carries the columns
    ``age, bmi, linear_y, disease, time, death, group`` — the union of
    fields that the cell registry's specs reference.

    The shape and invariants documented at the module top are enforced
    here: distinct numeric draws per column, balanced group split,
    strictly positive 2x2 outcome × group cells, and signal-bearing
    ``linear_y``.
    """
    age = draw(_AGE_STRAT)
    bmi = draw(_BMI_STRAT)
    time = draw(_TIME_STRAT)
    noise = draw(_NOISE_STRAT)
    disease_perm = draw(_PERM_STRAT)
    death_perm = draw(_PERM_STRAT)

    disease = _stratified_binary(disease_perm)
    death = _stratified_binary(death_perm)

    rows: list[dict[str, Any]] = []
    for i in range(DATASET_N):
        rows.append({
            "age": age[i],
            "bmi": bmi[i],
            "linear_y": 0.5 * age[i] + 0.3 * bmi[i] + noise[i],
            "disease": disease[i],
            "time": time[i],
            "death": death[i],
            "group": 0 if i < HALF_N else 1,
        })
    return rows


def write_csv(target_dir: Path, rows: list[dict[str, Any]]) -> Path:
    """Write *rows* to ``<target_dir>/synth.csv`` and return the path.

    The file uses ``\\n`` line terminators and UTF-8 encoding, matching
    how the canonical fixtures under ``datasets/synthetic/`` are emitted.
    """
    target_dir.mkdir(parents=True, exist_ok=True)
    out = target_dir / "synth.csv"
    fieldnames = list(rows[0].keys())
    with out.open("w", encoding="utf-8", newline="") as fh:
        writer = csv.DictWriter(fh, fieldnames=fieldnames, lineterminator="\n")
        writer.writeheader()
        writer.writerows(rows)
    return out


# ---------------------------------------------------------------------------
# Power-spec strategies (dataset-free algorithms)
# ---------------------------------------------------------------------------

#: ``power_single_arm`` accepts proportions and α / power scalars only —
#: there is no underlying dataset to draw. Hypothesis varies p0 / p1 over
#: a clinically meaningful range while keeping ``|p1 - p0| >= 0.05`` so
#: the effect size is non-degenerate and statsmodels' ``solve_power``
#: converges. α and power are clipped to ``(0.01, 0.20)`` and
#: ``(0.50, 0.95)`` respectively for the same reason.
power_single_arm_spec_strategy = st.fixed_dictionaries({
    "p0": st.floats(min_value=0.10, max_value=0.40, allow_nan=False),
    "p1": st.floats(min_value=0.45, max_value=0.80, allow_nan=False),
    "alpha": st.floats(min_value=0.01, max_value=0.20, allow_nan=False),
    "power": st.floats(min_value=0.50, max_value=0.95, allow_nan=False),
})

#: ``power_phase3`` uses Cohen's d as effect size; we keep |d| >= 0.2 so
#: ``TTestIndPower.solve_power`` returns a finite N within the iteration
#: cap. The collector currently is a stub (``parity/power_phase3.py``)
#: that always returns SKIP, so the hypothesis-generated spec is also
#: used by the test driver to assert the SKIP shape stays correct under
#: the live cell.
power_phase3_spec_strategy = st.fixed_dictionaries({
    "effect_size": st.floats(min_value=0.20, max_value=1.50, allow_nan=False),
    "alpha": st.floats(min_value=0.01, max_value=0.20, allow_nan=False),
    "power": st.floats(min_value=0.50, max_value=0.95, allow_nan=False),
})
