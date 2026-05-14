"""
parity/math_core.py — Numerical parity module for math core CDF functions.

Stats Code does not expose CDF functions as standalone CLI subcommands.
Instead, this module validates them *indirectly* by:

1. Running ``survival km`` with a known dataset and comparing the log-rank p-value
   (which uses chi_square_cdf internally) against scipy.
2. Running ``tableone`` and comparing chi-square / t-test p-values against scipy.
3. Running ``model logistic`` and comparing Wald p-values (which use normal_cdf)
   against scipy.

For each indirect test, the module records the CDF function being exercised in
the metric name (e.g. ``chi_square_cdf[logrank]``).

This approach is dataset-free: it uses the synthetic small_n40.csv dataset
that is always present after Task 5.1.
"""

from __future__ import annotations

import math
from pathlib import Path
from typing import Any

from .adapters import ReferenceAdapter
from .common import StatsCodeInvocationError, compare_scalar, run_stats_code
from .result import Status, ToleranceConfig, ValidationResult

METHOD = "math_core"
METRICS = [
    "normal_cdf",
    "chi_square_cdf",
    "t_cdf",
    "fisher_exact_pvalue",
]

# Path to the small synthetic dataset (relative to validation/)
_SYNTHETIC_SMALL = Path(__file__).resolve().parent.parent / "datasets" / "synthetic" / "small_n40.csv"


def _scipy_normal_cdf(x: float) -> float:
    from scipy.stats import norm
    return float(norm.cdf(x))


def _scipy_chi2_cdf(x: float, df: float) -> float:
    from scipy.stats import chi2
    return float(chi2.cdf(x, df))


def _scipy_t_cdf(x: float, df: float) -> float:
    from scipy.stats import t
    return float(t.cdf(x, df))


def _scipy_fisher_exact(table: list[list[int]]) -> float:
    from scipy.stats import fisher_exact
    _, p = fisher_exact(table)
    return float(p)


def collect(
    dataset_path: Path,  # may be __builtin__ sentinel; we use _SYNTHETIC_SMALL
    tol_config: ToleranceConfig,
    adapters: list[ReferenceAdapter],
    spec: dict[str, Any] | None = None,
) -> list[ValidationResult]:
    """
    Validate math core CDF functions indirectly via Stats Code CLI outputs.

    Strategy:
    - normal_cdf: validated via logistic Wald p-values (p = 2*(1 - normal_cdf(|z|)))
    - chi_square_cdf: validated via log-rank p-value from survival km
    - t_cdf: validated via linear regression t-test p-values
    - fisher_exact: validated via tableone categorical test on a 2x2 table
    """
    results: list[ValidationResult] = []
    dataset_label = "math_core_indirect"

    # Use the small synthetic dataset; fall back to provided path if it exists
    data_path = _SYNTHETIC_SMALL if _SYNTHETIC_SMALL.exists() else dataset_path
    if not data_path.exists() or str(data_path) == "__builtin__":
        return [ValidationResult(
            method=METHOD, dataset=dataset_label,
            reference_engine="scipy", metric="__setup__",
            tolerance=0.0, status=Status.SKIP,
            message=f"Synthetic dataset not found at {_SYNTHETIC_SMALL}; run gen_synthetic.py first",
        )]

    # ── Test 1: normal_cdf via logistic Wald p-values ───────────────────────
    results.extend(_validate_normal_cdf_via_logistic(data_path, dataset_label, tol_config))

    # ── Test 2: chi_square_cdf via survival log-rank ─────────────────────────
    results.extend(_validate_chi2_cdf_via_logrank(data_path, dataset_label, tol_config))

    # ── Test 3: t_cdf via linear regression p-values ─────────────────────────
    results.extend(_validate_t_cdf_via_linear(data_path, dataset_label, tol_config))

    # ── Test 4: fisher_exact via tableone ────────────────────────────────────
    results.extend(_validate_fisher_exact_via_tableone(data_path, dataset_label, tol_config))

    return results


def _validate_normal_cdf_via_logistic(
    data_path: Path, dataset_label: str, tol_config: ToleranceConfig
) -> list[ValidationResult]:
    """
    Logistic regression: p_value = 2*(1 - normal_cdf(|beta/stderr|)).
    We recover the implied normal_cdf value and compare with scipy.
    """
    results = []
    try:
        sc_out = run_stats_code([
            "--json", "model", "logistic",
            "--data", str(data_path.resolve()),
            "--y", "disease",
            "--x", "age,bmi",
        ])
    except StatsCodeInvocationError as exc:
        return [ValidationResult(
            method=METHOD, dataset=dataset_label,
            reference_engine="scipy", metric="normal_cdf",
            tolerance=0.0, status=Status.ERROR, message=str(exc),
        )]

    for coef in sc_out.get("coefficients", []):
        beta = float(coef.get("beta", float("nan")))
        se = float(coef.get("standard_error", float("nan")))
        sc_pvalue = float(coef.get("p_value", float("nan")))
        term = coef.get("term", "?")

        if not (math.isfinite(beta) and math.isfinite(se) and se > 0):
            continue

        z = abs(beta / se)
        # p = 2*(1 - normal_cdf(z))  →  normal_cdf(z) = 1 - p/2
        sc_implied_cdf = 1.0 - sc_pvalue / 2.0
        ref_cdf = _scipy_normal_cdf(z)

        results.append(compare_scalar(
            METHOD, f"normal_cdf[logistic/{term}]", dataset_label,
            "scipy", ref_cdf, sc_implied_cdf, tol_config,
        ))

    return results


def _validate_chi2_cdf_via_logrank(
    data_path: Path, dataset_label: str, tol_config: ToleranceConfig
) -> list[ValidationResult]:
    """
    Survival log-rank: p = 1 - chi_square_cdf(chi2, df=1).
    We recover the implied chi_square_cdf value and compare with scipy.
    """
    try:
        sc_out = run_stats_code([
            "--json", "survival", "km",
            "--data", str(data_path.resolve()),
            "--time", "time",
            "--event", "death",
            "--by", "group",
        ])
    except StatsCodeInvocationError as exc:
        return [ValidationResult(
            method=METHOD, dataset=dataset_label,
            reference_engine="scipy", metric="chi_square_cdf",
            tolerance=0.0, status=Status.ERROR, message=str(exc),
        )]

    log_rank = sc_out.get("log_rank")
    if not log_rank:
        return [ValidationResult(
            method=METHOD, dataset=dataset_label,
            reference_engine="scipy", metric="chi_square_cdf",
            tolerance=tol_config.lookup(METHOD, "chi_square_cdf"),
            status=Status.SKIP,
            message="No log_rank in survival km output (need --by group)",
        )]

    chi2_val = float(log_rank.get("chi_square", float("nan")))
    df = float(log_rank.get("degrees_freedom", 1.0))
    sc_pvalue = float(log_rank.get("p_value", float("nan")))

    if not math.isfinite(chi2_val):
        return []

    # p = 1 - chi_square_cdf(chi2, df)  →  chi_square_cdf = 1 - p
    sc_implied_cdf = 1.0 - sc_pvalue
    ref_cdf = _scipy_chi2_cdf(chi2_val, df)

    return [compare_scalar(
        METHOD, "chi_square_cdf[logrank]", dataset_label,
        "scipy", ref_cdf, sc_implied_cdf, tol_config,
    )]


def _validate_t_cdf_via_linear(
    data_path: Path, dataset_label: str, tol_config: ToleranceConfig
) -> list[ValidationResult]:
    """
    Linear regression: p_value = 2*(1 - t_cdf(|t_stat|, df=n-p)).
    We recover the implied t_cdf value and compare with scipy.
    """
    import pandas as pd

    results = []
    try:
        sc_out = run_stats_code([
            "--json", "model", "linear",
            "--data", str(data_path.resolve()),
            "--y", "linear_y",
            "--x", "age,bmi",
        ])
    except StatsCodeInvocationError as exc:
        return [ValidationResult(
            method=METHOD, dataset=dataset_label,
            reference_engine="scipy", metric="t_cdf",
            tolerance=0.0, status=Status.ERROR, message=str(exc),
        )]

    n_used = sc_out.get("n_used", 0)
    n_predictors = len(sc_out.get("coefficients", [])) - 1  # exclude intercept
    df = float(n_used - n_predictors - 1)

    for coef in sc_out.get("coefficients", []):
        t_stat = float(coef.get("t_statistic", float("nan")))
        sc_pvalue = float(coef.get("p_value", float("nan")))
        term = coef.get("term", "?")

        if not (math.isfinite(t_stat) and math.isfinite(sc_pvalue) and df > 0):
            continue

        # p = 2*(1 - t_cdf(|t|, df))  →  t_cdf = 1 - p/2
        sc_implied_cdf = 1.0 - sc_pvalue / 2.0
        ref_cdf = _scipy_t_cdf(abs(t_stat), df)

        results.append(compare_scalar(
            METHOD, f"t_cdf[linear/{term}]", dataset_label,
            "scipy", ref_cdf, sc_implied_cdf, tol_config,
        ))

    return results


def _validate_fisher_exact_via_tableone(
    data_path: Path, dataset_label: str, tol_config: ToleranceConfig
) -> list[ValidationResult]:
    """
    TableOne categorical test: when a 2x2 sparse table triggers Fisher exact,
    compare the p-value against scipy.fisher_exact.
    """
    try:
        sc_out = run_stats_code([
            "--json", "tableone",
            "--data", str(data_path.resolve()),
            "--by", "group",
            "--vars", "disease",
        ])
    except StatsCodeInvocationError as exc:
        return [ValidationResult(
            method=METHOD, dataset=dataset_label,
            reference_engine="scipy", metric="fisher_exact_pvalue",
            tolerance=0.0, status=Status.ERROR, message=str(exc),
        )]

    # Find a row that used Fisher exact
    for row in sc_out.get("rows", []):
        test_name = row.get("test_name", "")
        if "fisher" not in test_name.lower():
            continue
        sc_pvalue = row.get("p_value")
        if sc_pvalue is None:
            continue

        # Reconstruct 2x2 table from group cells
        groups_data = {gc["group"]: gc["cell"] for gc in row.get("groups", [])}
        if len(groups_data) != 2:
            continue

        g_keys = list(groups_data.keys())
        # count = events, n_non_missing - count = non-events
        def _cell_counts(cell: dict) -> tuple[int, int]:
            cnt = cell.get("count", 0) or 0
            total = cell.get("n_non_missing", 0) or 0
            return int(cnt), int(total - cnt)

        a, b = _cell_counts(groups_data[g_keys[0]])
        c, d = _cell_counts(groups_data[g_keys[1]])
        ref_p = _scipy_fisher_exact([[a, b], [c, d]])

        return [compare_scalar(
            METHOD, "fisher_exact_pvalue[tableone]", dataset_label,
            "scipy", ref_p, float(sc_pvalue), tol_config,
        )]

    return [ValidationResult(
        method=METHOD, dataset=dataset_label,
        reference_engine="scipy", metric="fisher_exact_pvalue",
        tolerance=tol_config.lookup(METHOD, "fisher_exact_pvalue"),
        status=Status.SKIP,
        message="No Fisher exact test triggered in tableone output for this dataset",
    )]
