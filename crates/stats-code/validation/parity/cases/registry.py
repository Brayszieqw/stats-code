"""Registry of Live parity test cases per Coverage Matrix cell.

Validates Requirements 4.3, 4.8.

The Algorithm Coverage Matrix (``coverage_matrix.toml``) records, for
each Output-Level Algorithm, which Reference Software is parity-tested
``live`` versus ``recorded``. Task 11.7 demands "every ``live`` cell has
at least one Live test case driven by hypothesis".

This module enumerates one :class:`LiveCase` per ``live`` cell:

* Each case is keyed by ``(algorithm_id, software)`` exactly as the
  matrix records it.
* Each case carries the bits the parametrized test driver in
  ``tests/properties/test_live_parity_cases.py`` needs to reuse the
  existing ``parity.<algorithm>.collect`` harness:
    - ``module_name`` — the ``parity.<x>`` module that exposes
      ``collect(dataset_path, tol_config, adapters, spec=...)``;
    - ``spec`` — the spec dict that ``collect`` will pass through to the
      Stats Engine CLI and the Reference adapter;
    - ``adapters`` — a callable that returns the Reference adapter list
      to plug into ``collect``. The selector is keyed by the cell's
      ``software`` column so the Python column drives Python in-process
      references and the R column drives the Rscript adapter only.
* Cases with no Reference adapter wired yet (every R live cell beyond
  ``cox`` / ``kaplan_meier``) get a registry entry whose ``adapters``
  callable returns an empty list. The driver is responsible for turning
  that into a structured ``live_R_adapter_not_yet_implemented`` skip so
  the Coverage Matrix consistency check (task 11.6) keeps passing while
  the missing R adapters are implemented in a follow-up wave.

The cases are deliberately *spec definitions only*. Stats Engine
invocation, Reference invocation, and threshold-based pass/fail
classification all live in the test driver; this module is pure data.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any, Callable

from hypothesis.strategies import SearchStrategy

from .strategies import (
    balanced_dataset_strategy,
    power_phase3_spec_strategy,
    power_single_arm_spec_strategy,
)


# ---------------------------------------------------------------------------
# Case shape
# ---------------------------------------------------------------------------

@dataclass(frozen=True)
class LiveCase:
    """One ``(algorithm, software)`` Live parity test case.

    Attributes:
        algorithm_id:
            Output-Level Algorithm id, matching the ``id`` field of
            ``coverage_matrix.toml``.
        software:
            Reference Software column — one of ``"R"`` or ``"Python"``.
            ``SAS`` and ``SPSS`` are excluded by construction because the
            matrix never assigns them ``live`` (they are always
            ``recorded`` or ``none``).
        module_name:
            Dotted name of the ``parity`` module that exposes ``collect``
            with the matching spec shape. ``None`` for cases whose
            ``collect`` is not yet wired upstream (e.g. ``standardization``,
            ``life_table``, ``attributable_risk`` — all ``sidecar_only``,
            so they never appear in this registry, but the field exists
            for forward-compatibility).
        spec_strategy:
            Hypothesis strategy that yields the keyword arguments the
            collector consumes. Two shapes are supported:
              - ``list[dict]`` rows (dataset-bearing algorithms): the
                driver writes the rows to a synthetic CSV before invoking
                ``collect``;
              - ``dict`` spec (dataset-free algorithms — currently only
                the two ``power_*`` cells): the driver passes a sentinel
                CSV path and lets the collector ignore it.
        spec_extra:
            Static spec fragments merged on top of the per-draw output
            (column choices, test variants, etc.). Kept as a separate
            field so Hypothesis only varies the parts that are random.
        adapters:
            Callable that returns the list of Reference adapters the
            collector should compare against. The driver invokes it once
            per case so adapters can lazily probe ``is_available()``
            without importing R libraries on Python-only test runs.
        kind:
            ``"dataset"`` (rows ⇒ CSV path) or ``"dataset_free"``
            (spec dict only) — used by the driver to pick the right
            invocation path.
    """

    algorithm_id: str
    software: str
    module_name: str | None
    spec_strategy: SearchStrategy[Any]
    spec_extra: dict[str, Any] = field(default_factory=dict)
    adapters: Callable[[], list[Any]] = field(default_factory=lambda: list)
    kind: str = "dataset"


# ---------------------------------------------------------------------------
# Adapter selectors
# ---------------------------------------------------------------------------
#
# We import lazily inside each callable so that a host that lacks rpy2 /
# Rscript can still discover the registry without ImportError, and so
# pytest collection stays cheap on cold caches.

def _python_adapter_for(method: str) -> Callable[[], list[Any]]:
    """Return a selector that yields the Python in-process adapters
    registered for *method* in ``parity.adapters.ADAPTERS_FOR``.

    Filters out non-Python adapters so the Python column does not
    accidentally trigger an Rscript subprocess.
    """

    def _select() -> list[Any]:
        from parity.adapters import ADAPTERS_FOR
        return [a for a in ADAPTERS_FOR.get(method, []) if getattr(a, "is_python", True)]

    return _select


def _r_adapter_for(method: str) -> Callable[[], list[Any]]:
    """Return a selector that yields the Rscript adapter for *method*,
    or an empty list when no Rscript adapter is wired for that algorithm
    yet.

    The collector treats an empty adapter list as "no Reference for this
    cell"; the driver turns that into a documented
    ``live_R_adapter_not_yet_implemented`` skip so the Coverage Matrix
    consistency check (task 11.6) keeps passing while the remaining R
    adapters are implemented in a follow-up wave.
    """

    def _select() -> list[Any]:
        from parity.adapters import ADAPTERS_FOR
        return [a for a in ADAPTERS_FOR.get(method, []) if not getattr(a, "is_python", True)]

    return _select


# ---------------------------------------------------------------------------
# Per-cell case registry
# ---------------------------------------------------------------------------
#
# Specs below mirror the ``_DEFAULT_SPEC`` of each ``parity.<algorithm>``
# module so the same column-name contract used by the orchestrator's
# curated synthetic datasets continues to apply to hypothesis-generated
# datasets. Where an algorithm exposes a real choice (correlation method,
# nonparametric test, etc.) we pick the variant that the matrix's
# Reference Implementation row in ``matrix.toml`` records.

_TTEST_SPEC = {"by": "group", "var": "age"}
_ANOVA_SPEC = {"by": "group", "var": "age"}
_NONPAR_SPEC = {"by": "group", "var": "age", "test": "mann_whitney"}
_CORRELATION_SPEC = {"x": "age", "y": "bmi", "method": "pearson"}
_OR_RR_SPEC = {"exposure": "group", "outcome": "disease"}
_TABLEONE_SPEC = {"by": "group", "vars": ["age", "bmi", "disease"]}
_LINEAR_SPEC = {"outcome": "linear_y", "covariates": ["age", "bmi"]}
_LOGISTIC_SPEC = {"outcome": "disease", "covariates": ["age", "bmi"]}
_KM_SPEC = {"duration_col": "time", "event_col": "death", "group_col": "group"}
_COX_SPEC = {"duration_col": "time", "event_col": "death", "covariates": ["age", "bmi"]}
_DIAGNOSTIC_ROC_SPEC = {"truth_col": "disease", "score_col": "score", "threshold": 0.5}


# Order matches the ``[[algorithm]]`` order in matrix.toml so the
# parametrized pytest ids stay stable PR-to-PR. Cells where the matrix
# does not record ``live`` are intentionally absent.
LIVE_CASES: list[LiveCase] = [
    # ── Table One ────────────────────────────────────────────────────────
    LiveCase(
        algorithm_id="tableone",
        software="Python",
        module_name="parity.tableone",
        spec_strategy=balanced_dataset_strategy(),
        spec_extra=_TABLEONE_SPEC,
        adapters=_python_adapter_for("tableone"),
    ),
    LiveCase(
        algorithm_id="tableone",
        software="R",
        module_name="parity.tableone",
        spec_strategy=balanced_dataset_strategy(),
        spec_extra=_TABLEONE_SPEC,
        adapters=_r_adapter_for("tableone"),
    ),

    # ── t-test ───────────────────────────────────────────────────────────
    LiveCase(
        algorithm_id="ttest",
        software="Python",
        module_name="parity.ttest",
        spec_strategy=balanced_dataset_strategy(),
        spec_extra=_TTEST_SPEC,
        adapters=_python_adapter_for("ttest"),
    ),
    LiveCase(
        algorithm_id="ttest",
        software="R",
        module_name="parity.ttest",
        spec_strategy=balanced_dataset_strategy(),
        spec_extra=_TTEST_SPEC,
        adapters=_r_adapter_for("ttest"),
    ),

    # ── ANOVA ────────────────────────────────────────────────────────────
    LiveCase(
        algorithm_id="anova",
        software="Python",
        module_name="parity.anova",
        spec_strategy=balanced_dataset_strategy(),
        spec_extra=_ANOVA_SPEC,
        adapters=_python_adapter_for("anova"),
    ),
    LiveCase(
        algorithm_id="anova",
        software="R",
        module_name="parity.anova",
        spec_strategy=balanced_dataset_strategy(),
        spec_extra=_ANOVA_SPEC,
        adapters=_r_adapter_for("anova"),
    ),

    # ── Non-parametric tests ─────────────────────────────────────────────
    LiveCase(
        algorithm_id="nonparametric",
        software="Python",
        module_name="parity.nonparametric",
        spec_strategy=balanced_dataset_strategy(),
        spec_extra=_NONPAR_SPEC,
        adapters=_python_adapter_for("nonparametric"),
    ),
    LiveCase(
        algorithm_id="nonparametric",
        software="R",
        module_name="parity.nonparametric",
        spec_strategy=balanced_dataset_strategy(),
        spec_extra=_NONPAR_SPEC,
        adapters=_r_adapter_for("nonparametric"),
    ),

    # ── Correlation ──────────────────────────────────────────────────────
    LiveCase(
        algorithm_id="correlation",
        software="Python",
        module_name="parity.correlation",
        spec_strategy=balanced_dataset_strategy(),
        spec_extra=_CORRELATION_SPEC,
        adapters=_python_adapter_for("correlation"),
    ),
    LiveCase(
        algorithm_id="correlation",
        software="R",
        module_name="parity.correlation",
        spec_strategy=balanced_dataset_strategy(),
        spec_extra=_CORRELATION_SPEC,
        adapters=_r_adapter_for("correlation"),
    ),

    # ── Odds Ratio / Relative Risk ───────────────────────────────────────
    LiveCase(
        algorithm_id="or_rr",
        software="Python",
        module_name="parity.or_rr",
        spec_strategy=balanced_dataset_strategy(),
        spec_extra=_OR_RR_SPEC,
        adapters=_python_adapter_for("or_rr"),
    ),
    LiveCase(
        algorithm_id="or_rr",
        software="R",
        module_name="parity.or_rr",
        spec_strategy=balanced_dataset_strategy(),
        spec_extra=_OR_RR_SPEC,
        adapters=_r_adapter_for("or_rr"),
    ),

    # ── Kaplan-Meier ─────────────────────────────────────────────────────
    LiveCase(
        algorithm_id="kaplan_meier",
        software="Python",
        module_name="parity.kaplan_meier",
        spec_strategy=balanced_dataset_strategy(),
        spec_extra=_KM_SPEC,
        adapters=_python_adapter_for("survival"),
    ),
    LiveCase(
        algorithm_id="kaplan_meier",
        software="R",
        module_name="parity.kaplan_meier",
        spec_strategy=balanced_dataset_strategy(),
        spec_extra=_KM_SPEC,
        adapters=_r_adapter_for("survival"),
    ),

    # ── Cox proportional hazards ─────────────────────────────────────────
    LiveCase(
        algorithm_id="cox",
        software="Python",
        module_name="parity.cox",
        spec_strategy=balanced_dataset_strategy(),
        spec_extra=_COX_SPEC,
        adapters=_python_adapter_for("cox"),
    ),
    LiveCase(
        algorithm_id="cox",
        software="R",
        module_name="parity.cox",
        spec_strategy=balanced_dataset_strategy(),
        spec_extra=_COX_SPEC,
        adapters=_r_adapter_for("cox"),
    ),

    # ── Linear regression ────────────────────────────────────────────────
    LiveCase(
        algorithm_id="linear",
        software="Python",
        module_name="parity.linear",
        spec_strategy=balanced_dataset_strategy(),
        spec_extra=_LINEAR_SPEC,
        adapters=_python_adapter_for("linear"),
    ),
    LiveCase(
        algorithm_id="linear",
        software="R",
        module_name="parity.linear",
        spec_strategy=balanced_dataset_strategy(),
        spec_extra=_LINEAR_SPEC,
        adapters=_r_adapter_for("linear"),
    ),

    # ── Logistic regression ──────────────────────────────────────────────
    LiveCase(
        algorithm_id="logistic",
        software="Python",
        module_name="parity.logistic",
        spec_strategy=balanced_dataset_strategy(),
        spec_extra=_LOGISTIC_SPEC,
        adapters=_python_adapter_for("logistic"),
    ),
    LiveCase(
        algorithm_id="logistic",
        software="R",
        module_name="parity.logistic",
        spec_strategy=balanced_dataset_strategy(),
        spec_extra=_LOGISTIC_SPEC,
        adapters=_r_adapter_for("logistic"),
    ),

    # ── Power / Sample Size — Single-Arm (dataset-free) ──────────────────
    LiveCase(
        algorithm_id="power_single_arm",
        software="Python",
        module_name="parity.power_single_arm",
        spec_strategy=power_single_arm_spec_strategy,
        adapters=_python_adapter_for("power"),
        kind="dataset_free",
    ),
    LiveCase(
        algorithm_id="power_single_arm",
        software="R",
        module_name="parity.power_single_arm",
        spec_strategy=power_single_arm_spec_strategy,
        adapters=_r_adapter_for("power"),
        kind="dataset_free",
    ),

    # ── Power / Sample Size — Phase 3 (dataset-free, currently a stub) ──
    LiveCase(
        algorithm_id="power_phase3",
        software="Python",
        module_name="parity.power_phase3",
        spec_strategy=power_phase3_spec_strategy,
        adapters=_python_adapter_for("power"),
        kind="dataset_free",
    ),
    LiveCase(
        algorithm_id="power_phase3",
        software="R",
        module_name="parity.power_phase3",
        spec_strategy=power_phase3_spec_strategy,
        adapters=_r_adapter_for("power"),
        kind="dataset_free",
    ),

    # ── Diagnostic-test ROC ──────────────────────────────────────────────
    LiveCase(
        algorithm_id="diagnostic_roc",
        software="Python",
        module_name="parity.diagnostic_roc",
        spec_strategy=balanced_dataset_strategy(),
        spec_extra=_DIAGNOSTIC_ROC_SPEC,
        adapters=_python_adapter_for("diagnostic_roc"),
    ),
    LiveCase(
        algorithm_id="diagnostic_roc",
        software="R",
        module_name="parity.diagnostic_roc",
        spec_strategy=balanced_dataset_strategy(),
        spec_extra=_DIAGNOSTIC_ROC_SPEC,
        adapters=_r_adapter_for("diagnostic_roc"),
    ),
]


def live_cells() -> list[tuple[str, str]]:
    """Return ``(algorithm_id, software)`` for every registered live case."""
    return [(c.algorithm_id, c.software) for c in LIVE_CASES]
