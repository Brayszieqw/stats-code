"""
parity/anova.py — Live parity collector for one-way ANOVA.

Stats Code CLI: ``anova --data ... --by <group> --var <variable>``

JSON output fields used:
  - f_statistic  : F-test statistic
  - p_value      : p-value from the F-distribution
  - df_between   : degrees of freedom between groups
  - df_within    : degrees of freedom within groups
  - ss_between   : sum of squares between groups
  - ss_within    : sum of squares within groups

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

METHOD = "anova"
METRICS = ["f_statistic", "p_value"]

_DEFAULT_SPEC: dict[str, Any] = {
    "by": "group",
    "var": "age",
}


def _python_reference(dataset_path: Path, spec: dict[str, Any]) -> dict[str, float]:
    """Compute one-way ANOVA F-test using scipy as reference."""
    df = pd.read_csv(dataset_path)
    by_col = spec["by"]
    var_col = spec["var"]

    groups = sorted(df[by_col].dropna().unique())
    if len(groups) < 2:
        raise ValueError(f"Need at least 2 groups in '{by_col}', found {len(groups)}")

    group_data = [
        df.loc[df[by_col] == g, var_col].dropna().values for g in groups
    ]

    f_stat, p_value = sp_stats.f_oneway(*group_data)

    return {
        "f_statistic": float(f_stat),
        "p_value": float(p_value),
    }


def collect(
    dataset_path: Path,
    tol_config: ToleranceConfig,
    adapters: list[Any],
    spec: dict[str, Any] | None = None,
) -> list[ValidationResult]:
    """Run one-way ANOVA parity checks for *dataset_path*."""
    if spec is None:
        spec = _DEFAULT_SPEC

    dataset_label = dataset_path.name
    results: list[ValidationResult] = []

    by_col = spec["by"]
    var_col = spec["var"]

    # ── 1. Call Stats Code CLI ───────────────────────────────────────────────
    try:
        sc_out = run_stats_code([
            "--json", "anova",
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

    sc_f = float(sc_out.get("f_statistic", float("nan")))
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
        METHOD, "f_statistic", dataset_label,
        ref_name, ref["f_statistic"], sc_f, tol_config,
    ))
    results.append(compare_scalar(
        METHOD, "p_value", dataset_label,
        ref_name, ref["p_value"], sc_p, tol_config,
    ))

    return results
