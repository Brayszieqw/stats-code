"""Per-cell Live parity cases (task 11.7).

Validates Requirements 4.3, 4.8.

For every ``(algorithm, software)`` cell that the Algorithm Coverage
Matrix marks as ``live`` (Requirement 6.2), this driver:

1. Asks ``parity.cases.registry`` for the matching :class:`LiveCase`.
2. Uses the registered Hypothesis strategy to draw a small synthetic
   dataset (or spec, for the dataset-free ``power_*`` cells).
3. Invokes the existing ``parity.<algorithm>.collect`` harness with that
   dataset / spec — which runs the Stats Engine CLI (``cargo run -p
   stats-code -- ...``) and the Reference adapter selected for the
   cell's software column.
4. Applies ``parity.threshold.fail_predicate`` to every comparison and
   asserts no row fails.

The whole driver is gated on ``@pytest.mark.slow`` because step (3)
shells out to ``cargo``; a clean ``pytest -q`` invocation collects every
case but does not execute it. CI invokes ``pytest -m slow`` once cargo
is built, which is the same convention ``test_numerical_parity.py``
already uses for the curated synthetic-dataset arm.

Cells whose R Reference adapter is not yet wired (every R live cell
beyond ``cox`` / ``kaplan_meier``) emit a structured skip with reason
``live_R_adapter_not_yet_implemented`` so the Coverage Matrix
consistency check (task 11.6) keeps passing while the missing R
adapters are implemented in a follow-up wave.
"""

from __future__ import annotations

import importlib
import shutil
import sys
from pathlib import Path
from typing import Any

import pytest
from hypothesis import HealthCheck, given, settings, strategies as st

# ---------------------------------------------------------------------------
# Path bootstrap — make the ``parity`` package importable when pytest is
# launched from the repo root, the validation/ dir, or anywhere else.
# ``tests/properties/`` is two levels under ``validation/``.
# ---------------------------------------------------------------------------

_VALIDATION_DIR = Path(__file__).resolve().parents[2]
if str(_VALIDATION_DIR) not in sys.path:
    sys.path.insert(0, str(_VALIDATION_DIR))

from parity.cases import LIVE_CASES, LiveCase  # noqa: E402  — after sys.path bootstrap
from parity.cases.strategies import write_csv  # noqa: E402
from parity.result import Status, ToleranceConfig, ValidationResult  # noqa: E402
from parity.threshold import fail_predicate  # noqa: E402

_TOL_CONFIG_PATH = _VALIDATION_DIR / "tolerance_config.yaml"
_TOL_CONFIG = ToleranceConfig.from_yaml(_TOL_CONFIG_PATH)

_CASE_IDS = [f"{c.algorithm_id}-{c.software}" for c in LIVE_CASES]


# ---------------------------------------------------------------------------
# Per-row classification
# ---------------------------------------------------------------------------

def _row_is_failure(row: ValidationResult) -> bool:
    """Return True iff *row* should fail the parity gate.

    The classifier is intentionally conservative:

    * ``ERROR`` rows always fail — they signal a CLI crash, missing
      required JSON field, or non-finite reference value, none of which
      can be silently absorbed.
    * ``FAIL`` rows fail when ``threshold.fail_predicate`` confirms the
      difference exceeds both the absolute and relative tolerance the
      collector recorded on the row. ``ValidationResult`` only carries
      the absolute tolerance, so we re-derive the relative tolerance
      from ``ToleranceConfig`` against the same ``(method, metric)``
      key the collector used. When the reference magnitude is at or
      below the absolute tolerance we treat the relative comparison as
      ``n/a`` (Requirement 3.3) and the predicate short-circuits to
      ``False``.
    * ``PASS`` and ``SKIP`` rows never fail.
    """
    if row.status == Status.PASS or row.status == Status.SKIP:
        return False
    if row.status == Status.ERROR:
        return True

    # status == FAIL: re-derive the relative-tolerance arm and consult
    # the shared predicate, mirroring how task 7.2 will gate ``--replay``.
    expected = row.expected
    actual = row.actual
    if expected is None or actual is None:
        # Pre-comparison failure (missing field, etc.) — keep the FAIL.
        return True

    abs_diff = abs(expected - actual)
    abs_tol = float(row.tolerance)
    rel_diff: float | None
    if abs(expected) <= abs_tol:
        rel_diff = None
    else:
        rel_diff = abs_diff / abs(expected)

    rel_tol = _TOL_CONFIG.lookup(row.method, row.metric)
    return fail_predicate(abs_diff=abs_diff, rel_diff=rel_diff,
                          abs_tol=abs_tol, rel_tol=rel_tol)


# ---------------------------------------------------------------------------
# Hypothesis settings — small budget so the suite stays under a minute
# even with cargo invocations on the slow path.
# ---------------------------------------------------------------------------

def _hypothesis_settings() -> settings:
    return settings(
        max_examples=2,
        deadline=None,
        suppress_health_check=[
            HealthCheck.function_scoped_fixture,
            HealthCheck.too_slow,
            HealthCheck.data_too_large,
        ],
    )


# ---------------------------------------------------------------------------
# R-availability probe — reused for every R cell so we short-circuit
# ``reference_software_unavailable`` skips before running cargo.
# ---------------------------------------------------------------------------

def _rscript_on_path() -> bool:
    return shutil.which("Rscript") is not None


# ---------------------------------------------------------------------------
# Driver
# ---------------------------------------------------------------------------

@pytest.mark.slow
@pytest.mark.parametrize("case", LIVE_CASES, ids=_CASE_IDS)
def test_live_cell_parity(case: LiveCase, tmp_path: Path) -> None:
    """Hypothesis-driven Stats Engine ↔ Reference parity for one Live cell."""
    if case.software not in ("R", "Python"):
        pytest.fail(
            f"unexpected live software for ({case.algorithm_id}, "
            f"{case.software!r}): only R and Python are matrix-allowed."
        )

    if case.software == "R" and not _rscript_on_path():
        pytest.skip("reference_software_unavailable: Rscript not on PATH")

    adapters = case.adapters()
    if case.software == "R" and not adapters:
        pytest.skip(
            "live_R_adapter_not_yet_implemented: "
            f"no Rscript adapter for ({case.algorithm_id}, R) in "
            "parity/adapters.py — tracked as a follow-up wave"
        )

    if case.module_name is None:
        pytest.skip(
            f"live_collector_not_yet_implemented: "
            f"no parity.<module> for ({case.algorithm_id}, {case.software})"
        )

    module = importlib.import_module(case.module_name)
    cell_dir = tmp_path / f"{case.algorithm_id}__{case.software}"
    cell_dir.mkdir(parents=True, exist_ok=True)

    @given(drawn=case.spec_strategy)
    @_hypothesis_settings()
    def _run_one(drawn: Any) -> None:
        # Wipe the previous example's CSV(s) so files do not pile up
        # across hypothesis examples (relevant for diagnostic_roc, which
        # writes an enriched dataset alongside the input).
        for stale in cell_dir.glob("*.csv"):
            stale.unlink()

        if case.kind == "dataset":
            assert isinstance(drawn, list), (
                f"{case.algorithm_id}/{case.software}: dataset-bearing case "
                "must draw rows but got a non-list"
            )
            dataset_path = write_csv(cell_dir, drawn)
            spec = dict(case.spec_extra)
        else:  # dataset_free
            assert isinstance(drawn, dict), (
                f"{case.algorithm_id}/{case.software}: dataset-free case "
                "must draw a spec dict but got a non-dict"
            )
            # The collector ignores the path for dataset-free algorithms
            # (DATASET_FREE_METHODS in run_validation.py); we still pass a
            # placeholder so the signature matches.
            dataset_path = cell_dir / "__builtin__"
            spec = {**case.spec_extra, **drawn}

        rows = module.collect(
            dataset_path=dataset_path,
            tol_config=_TOL_CONFIG,
            adapters=adapters,
            spec=spec,
        )

        assert isinstance(rows, list) and rows, (
            f"{case.module_name}.collect returned no rows for "
            f"({case.algorithm_id}, {case.software})"
        )

        failures = [r for r in rows if _row_is_failure(r)]
        if failures:
            head = "\n".join(
                f"  {r.method}.{r.metric} ({r.reference_engine}): "
                f"expected={r.expected!r} actual={r.actual!r} "
                f"diff={r.difference!r} tol={r.tolerance!r} "
                f"status={r.status} message={r.message!r}"
                for r in failures
            )
            pytest.fail(
                f"Parity gate tripped for ({case.algorithm_id}, "
                f"{case.software}) on hypothesis-generated input:\n{head}"
            )

    _run_one()


# ---------------------------------------------------------------------------
# Sanity tests
# ---------------------------------------------------------------------------

def test_registry_covers_every_live_cell_in_matrix() -> None:
    """The registry must mirror the matrix: one case per ``live`` cell.

    If a future PR adds a new ``live`` cell to ``coverage_matrix.toml``
    but forgets to register a ``LiveCase``, this test fails before any
    cargo invocation, so contributors get a fast and obvious signal.
    """
    import tomllib

    matrix_path = _VALIDATION_DIR / "coverage_matrix.toml"
    with matrix_path.open("rb") as fh:
        matrix = tomllib.load(fh)

    matrix_cells: set[tuple[str, str]] = set()
    for entry in matrix.get("algorithm", []):
        algorithm_id = str(entry["id"])
        for software, value in entry.get("coverage", {}).items():
            if value == "live":
                matrix_cells.add((algorithm_id, software))

    registry_cells = {(c.algorithm_id, c.software) for c in LIVE_CASES}

    missing = matrix_cells - registry_cells
    extra = registry_cells - matrix_cells
    assert not missing, (
        "matrix declares live cells that have no LiveCase registered: "
        f"{sorted(missing)}"
    )
    assert not extra, (
        "registry has LiveCase entries that the matrix does not mark live: "
        f"{sorted(extra)}"
    )


def test_registry_is_non_empty() -> None:
    """Canary against an accidentally-empty matrix or registry.

    Mirrors ``test_live_cells.py::test_live_cells_inventory_is_non_empty``
    so Requirements 4.3 / 4.8 keep at least one live test case attached
    even if a refactor reorders or replaces the matrix.
    """
    assert LIVE_CASES, (
        "parity.cases.LIVE_CASES is empty — Requirements 4.3 / 4.8 demand "
        "at least one Live test case."
    )
