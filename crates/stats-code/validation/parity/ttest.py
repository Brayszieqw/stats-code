"""
parity/ttest.py — Live parity collector for two-sample t-test.

Stats Code CLI: ``ttest --data ... --by <group> --var <variable>``

JSON output fields used:
  - t_statistic  : t-test statistic (Welch's by default)
  - p_value      : two-sided p-value
  - mean_group_0 : mean of group 0
  - mean_group_1 : mean of group 1
  - mean_diff    : difference in means (group_1 - group_0)
  - ci_lower     : 95% CI lower bound for mean difference
  - ci_upper     : 95% CI upper bound for mean difference

Validates: Requirements 4.3, 4.8
"""

from __future__ import annotations

from pathlib import Path
from typing import Any

import numpy as np
import pandas as pd
from scipy import stats as sp_stats

from .adapters import ReferenceAdapter
from .common import StatsCodeInvocationError, compare_scalar, run_stats_code
from .result import Status, ToleranceConfig, ValidationResult

METHOD = "ttest"
METRICS = ["t_statistic", "p_value", "mean_diff", "ci_lower", "ci_upper"]

_DEFAULT_SPEC: dict[str, Any] = {
    "by": "group",
    "var": "age",
}


def _python_reference(dataset_path: Path, spec: dict[str, Any]) -> dict[str, float]:
    """Compute Welch's two-sample t-test using scipy as reference."""
    df = pd.read_csv(dataset_path)
    by_col = spec["by"]
    var_col = spec["var"]

    groups = sorted(df[by_col].dropna().unique())
    if len(groups) < 2:
        raise ValueError(f"Need at least 2 groups in '{by_col}', found {len(groups)}")

    g0 = df.loc[df[by_col] == groups[0], var_col].dropna().values
    g1 = df.loc[df[by_col] == groups[1], var_col].dropna().values

    # Welch's t-test (unequal variance)
    t_stat, p_value = sp_stats.ttest_ind(g0, g1, equal_var=False)

    mean_diff = float(np.mean(g1)) - float(np.mean(g0))

    # 95% CI for mean difference (Welch-Satterthwaite)
    n0, n1 = len(g0), len(g1)
    s0, s1 = float(np.std(g0, ddof=1)), float(np.std(g1, ddof=1))
    se = np.sqrt(s0**2 / n0 + s1**2 / n1)
    # degrees of freedom (Welch-Satterthwaite)
    nu_num = (s0**2 / n0 + s1**2 / n1) ** 2
    nu_den = (s0**2 / n0) ** 2 / (n0 - 1) + (s1**2 / n1) ** 2 / (n1 - 1)
    nu = nu_num / nu_den if nu_den > 0 else n0 + n1 - 2
    t_crit = sp_stats.t.ppf(0.975, nu)
    ci_lower = mean_diff - t_crit * se
    ci_upper = mean_diff + t_crit * se

    return {
        "t_statistic": float(t_stat),
        "p_value": float(p_value),
        "mean_diff": mean_diff,
        "ci_lower": float(ci_lower),
        "ci_upper": float(ci_upper),
    }


def collect(
    dataset_path: Path,
    tol_config: ToleranceConfig,
    adapters: list[Any],
    spec: dict[str, Any] | None = None,
) -> list[ValidationResult]:
    """Run two-sample t-test parity checks for *dataset_path*."""
    if spec is None:
        spec = _DEFAULT_SPEC

    dataset_label = dataset_path.name
    results: list[ValidationResult] = []

    by_col = spec["by"]
    var_col = spec["var"]

    # ── 1. Call Stats Code CLI ───────────────────────────────────────────────
    try:
        sc_out = run_stats_code([
            "--json", "ttest",
            "--data", str(dataset_path.resolve()),
            "--by", by_col,
            "--var", var_col,
        ])
    except StatsCodeInvocationError as exc:
        return [ValidationResult(
            method=METHOD, dataset=dataset_label,
            reference_engine="stats_code_cli", metric="__invoke__",
            tolerance=0.0, status=Status.ERROR, message=str(exc),
        )]

    sc_t = float(sc_out.get("t_statistic", float("nan")))
    sc_p = float(sc_out.get("p_value", float("nan")))
    sc_mean_diff = float(sc_out.get("mean_diff", float("nan")))
    sc_ci_lower = float(sc_out.get("ci_lower", float("nan")))
    sc_ci_upper = float(sc_out.get("ci_upper", float("nan")))

    # ── 2. Python reference (scipy) ─────────────────────────────────────────
    ref_name = "scipy"
    try:
        ref = _python_reference(dataset_path, spec)
    except Exception as exc:
        return [ValidationResult(
            method=METHOD, dataset=dataset_label,
            reference_engine=ref_name, metric="__fit__",
            tolerance=0.0, status=Status.ERROR,
            message=f"Python reference raised: {exc}",
        )]

    for metric_key in METRICS:
        results.append(compare_scalar(
            METHOD, metric_key, dataset_label,
            ref_name, ref[metric_key],
            {"t_statistic": sc_t, "p_value": sc_p, "mean_diff": sc_mean_diff,
             "ci_lower": sc_ci_lower, "ci_upper": sc_ci_upper}[metric_key],
            tol_config,
        ))

    return results
