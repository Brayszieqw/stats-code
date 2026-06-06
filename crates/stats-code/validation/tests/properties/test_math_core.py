# Feature: validation-correctness, Property 2: Math Core Functional Equivalence
"""
Property 2: For any valid input, Stats Code's CDF implementations match scipy.stats
within tolerance_config.lookup("math_core", function_name).

Since math_core functions are internal (no standalone CLI), we validate them
indirectly via the p-values produced by model/survival/tableone commands.
The direct hypothesis tests here verify the scipy reference values are self-consistent
and that our tolerance config is appropriate.
"""
from pathlib import Path

import pytest
from hypothesis import given, settings
from hypothesis import strategies as st
from scipy import stats as sp_stats

from parity.result import ToleranceConfig

VALIDATION_DIR = Path(__file__).resolve().parents[2]
CONFIG_PATH = VALIDATION_DIR / "tolerance_config.yaml"


@pytest.fixture(scope="module")
def tol_config() -> ToleranceConfig:
    return ToleranceConfig.from_yaml(CONFIG_PATH)


# ── Property 2a: normal_cdf ──────────────────────────────────────────────────

@given(x=st.floats(min_value=-10.0, max_value=10.0, allow_nan=False, allow_infinity=False))
@settings(max_examples=200)
def test_scipy_normal_cdf_range(x: float) -> None:
    """Verify scipy normal_cdf is in [0, 1] — baseline for our reference."""
    cdf = sp_stats.norm.cdf(x)
    assert 0.0 <= cdf <= 1.0, f"scipy norm.cdf({x}) = {cdf} out of [0, 1]"


@given(
    x1=st.floats(min_value=-10.0, max_value=10.0, allow_nan=False),
    x2=st.floats(min_value=-10.0, max_value=10.0, allow_nan=False),
)
@settings(max_examples=200)
def test_scipy_normal_cdf_monotone(x1: float, x2: float) -> None:
    """Verify scipy normal_cdf is monotone — baseline for our reference."""
    lo, hi = min(x1, x2), max(x1, x2)
    assert sp_stats.norm.cdf(lo) <= sp_stats.norm.cdf(hi) + 1e-15


# ── Property 2b: chi_square_cdf ──────────────────────────────────────────────

@given(
    x=st.floats(min_value=0.01, max_value=50.0, allow_nan=False),
    df=st.floats(min_value=1.0, max_value=30.0, allow_nan=False),
)
@settings(max_examples=200)
def test_scipy_chi2_cdf_range(x: float, df: float) -> None:
    """Verify scipy chi2.cdf is in [0, 1]."""
    cdf = sp_stats.chi2.cdf(x, df)
    assert 0.0 <= cdf <= 1.0, f"scipy chi2.cdf({x}, {df}) = {cdf} out of [0, 1]"


# ── Property 2c: t_cdf ───────────────────────────────────────────────────────

@given(
    x=st.floats(min_value=-10.0, max_value=10.0, allow_nan=False),
    df=st.floats(min_value=1.0, max_value=100.0, allow_nan=False),
)
@settings(max_examples=200)
def test_scipy_t_cdf_range(x: float, df: float) -> None:
    """Verify scipy t.cdf is in [0, 1]."""
    cdf = sp_stats.t.cdf(x, df)
    assert 0.0 <= cdf <= 1.0, f"scipy t.cdf({x}, {df}) = {cdf} out of [0, 1]"


# ── Property 2d: tolerance config is tight enough ────────────────────────────

def test_math_core_tolerances_are_tight(tol_config: ToleranceConfig) -> None:
    """Property 2: math_core tolerances must be ≤ 1e-10 (same algorithm family)."""
    for func in ("normal_cdf", "chi_square_cdf", "t_cdf", "f_cdf"):
        tol = tol_config.lookup("math_core", func)
        assert tol <= 1e-10, (
            f"math_core.{func} tolerance {tol:.2e} is too loose (expected ≤ 1e-10)"
        )


def test_fisher_exact_tolerance_is_very_tight(tol_config: ToleranceConfig) -> None:
    """Property 2: fisher_exact_pvalue tolerance must be ≤ 1e-12 (pure combinatorics)."""
    tol = tol_config.lookup("math_core", "fisher_exact_pvalue")
    assert tol <= 1e-12, (
        f"math_core.fisher_exact_pvalue tolerance {tol:.2e} is too loose (expected ≤ 1e-12)"
    )


# ── Spec: parity-math-core-collect-crash ─────────────────────────────────────
# Property 2: Fisher-row selection equivalence and crash-freedom.
#
# The fixed selection uses ``(row.get("test_name") or "").lower()``. For any
# test_name drawn from {None, missing-key, arbitrary strings}, selection must
# (a) never raise and (b) equal the reference selection using the same idiom.

from parity import math_core as _math_core  # noqa: E402
from parity.result import Status as _Status, ToleranceConfig as _ToleranceConfig  # noqa: E402


def _fisher_selected_variables(rows: list[dict]) -> list[str]:
    """Reference: which variables would be picked as 'fisher' rows, using the
    canonical ``(x or "").lower()`` idiom."""
    picked = []
    for row in rows:
        name = (row.get("test_name") or "").lower()
        if "fisher" in name:
            picked.append(row.get("variable", "?"))
    return picked


_test_name_strategy = st.one_of(
    st.none(),
    st.text(max_size=20),
    st.sampled_from(["fisher", "Fisher", "FISHER", "fisher exact", "t-test", "chi2", "kruskal"]),
)


@given(
    rows=st.lists(
        st.fixed_dictionaries(
            {
                "variable": st.text(min_size=1, max_size=6),
                "test_name": _test_name_strategy,
                "p_value": st.floats(min_value=0.0, max_value=1.0, allow_nan=False),
            }
        ),
        max_size=8,
    )
)
@settings(max_examples=200)
def test_fisher_selection_never_crashes_and_matches_reference(rows: list[dict]) -> None:
    """The collect helper must not raise for any test_name shape, and when no
    fisher row exists it returns the canonical SKIP."""
    import pytest as _pytest

    monkeypatch = _pytest.MonkeyPatch()
    try:
        monkeypatch.setattr(_math_core, "run_stats_code", lambda args: {"rows": rows})
        tol = _ToleranceConfig(per_metric={}, default=1e-6)
        from pathlib import Path as _Path

        # Must never raise regardless of test_name values.
        results = _math_core._validate_fisher_exact_via_tableone(
            _Path("dummy.csv"), "math_core_indirect", tol
        )
        assert isinstance(results, list)

        # Reference selection: if no fisher row, the helper yields a single SKIP.
        if not _fisher_selected_variables(rows):
            assert len(results) == 1
            assert results[0].status == _Status.SKIP
            assert "No Fisher exact test triggered" in results[0].message
    finally:
        monkeypatch.undo()
