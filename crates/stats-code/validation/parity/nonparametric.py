"""
parity/nonparametric.py — Live parity collector for nonparametric tests.

Stats Code CLI: ``nonparametric --data ... --by <group> --var <variable> [--test mann_whitney|kruskal]``

JSON output fields used:
  - test_name    : "mann_whitney_u" or "kruskal_wallis"
  - statistic    : U-statistic or H-statistic
  - p_value      : p-value

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

METHOD = "nonparametric"
METRICS = ["statistic", "p_value"]

_DEFAULT_SPEC: dict[str, Any] = {
    "by": "group",
    "var": "age",
    "test": "mann_whitney",
}


def _python_reference(dataset_path: Path, spec: dict[str, Any]) -> dict[str, float]:
    """Compute nonparametric test using scipy as reference."""
    df = pd.read_csv(dataset_path)
    by_col = spec["by"]
    var_col = spec["var"]
    test_type = spec.get("test", "mann_whitney")

    groups = sorted(df[by_col].dropna().unique())
    group_data = [
        df.loc[df[by_col] == g, var_col].dropna().values for g in groups
    ]

    if test_type == "mann_whitney" and len(groups) == 2:
        stat, p_value = sp_stats.mannwhitneyu(
            group_data[0], group_data[1], alternative="two-sided"
        )
    elif test_type == "kruskal" or len(groups) > 2:
        stat, p_value = sp_stats.kruskal(*group_data)
    else:
        stat, p_value = sp_stats.mannwhitneyu(
            group_data[0], group_data[1], alternative="two-sided"
        )

    return {
        "statistic": float(stat),
        "p_value": float(p_value),
    }


def collect(
    dataset_path: Path,
    tol_config: ToleranceConfig,
    adapters: list[Any],
    spec: dict[str, Any] | None = None,
) -> list[ValidationResult]:
    """Run nonparametric test parity checks for *dataset_path*."""
    if spec is None:
        spec = _DEFAULT_SPEC

    dataset_label = dataset_path.name
    results: list[ValidationResult] = []

    by_col = spec["by"]
    var_col = spec["var"]
    test_type = spec.get("test", "mann_whitney")

    # ── 1. Call Stats Code CLI ───────────────────────────────────────────────
    try:
        sc_out = run_stats_code([
            "--json", "nonparametric",
            "--data", str(dataset_path.resolve()),
            "--by", by_col,
            "--var", var_col,
            "--test", test_type,
        ])
    except StatsCodeInvocationError as exc:
        return [ValidationResult(
            method=METHOD, dataset=dataset_label,
            reference_engine="stats_code_cli", metric="__invoke__",
            tolerance=0.0, status=Status.ERROR, message=str(exc),
        )]

    sc_stat = float(sc_out.get("statistic", float("nan")))
    sc_p = float(sc_out.get("p_value", float("nan")))

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

    results.append(compare_scalar(
        METHOD, "statistic", dataset_label,
        ref_name, ref["statistic"], sc_stat, tol_config,
    ))
    results.append(compare_scalar(
        METHOD, "p_value", dataset_label,
        ref_name, ref["p_value"], sc_p, tol_config,
    ))

    return results
