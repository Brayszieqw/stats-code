"""
parity/correlation.py — Live parity collector for correlation analysis.

Stats Code CLI: ``correlation --data ... --x <var1> --y <var2> [--method pearson|spearman]``

JSON output fields used:
  - method       : "pearson" or "spearman"
  - statistic    : correlation coefficient (r or rho)
  - p_value      : two-sided p-value for H0: rho == 0

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

METHOD = "correlation"
METRICS = ["statistic", "p_value"]

_DEFAULT_SPEC: dict[str, Any] = {
    "x": "age",
    "y": "bmi",
    "method": "pearson",
}


def _python_reference(dataset_path: Path, spec: dict[str, Any]) -> dict[str, float]:
    """Compute correlation using scipy as reference."""
    df = pd.read_csv(dataset_path)
    x_col = spec["x"]
    y_col = spec["y"]
    corr_method = spec.get("method", "pearson")

    x = df[x_col].dropna().values
    y = df[y_col].dropna().values

    # Align on non-missing pairs
    mask = ~(np.isnan(x) | np.isnan(y))
    x, y = x[mask], y[mask]

    if corr_method == "spearman":
        stat, p_value = sp_stats.spearmanr(x, y)
    else:
        stat, p_value = sp_stats.pearsonr(x, y)

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
    """Run correlation parity checks for *dataset_path*."""
    if spec is None:
        spec = _DEFAULT_SPEC

    dataset_label = dataset_path.name
    results: list[ValidationResult] = []

    x_col = spec["x"]
    y_col = spec["y"]
    corr_method = spec.get("method", "pearson")

    # ── 1. Call Stats Code CLI ───────────────────────────────────────────────
    try:
        sc_out = run_stats_code([
            "--json", "correlation",
            "--data", str(dataset_path.resolve()),
            "--x", x_col,
            "--y", y_col,
            "--method", corr_method,
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
