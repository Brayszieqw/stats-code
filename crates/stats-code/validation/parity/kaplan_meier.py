"""
parity/kaplan_meier.py — Live parity collector for Kaplan–Meier survival estimates.

Stats Code CLI: ``survival km --data ... --time <col> --event <col> [--group <col>]``

JSON output fields used:
  - median_survival       : median survival time
  - survival_probabilities: list of {time, survival, se}
  - logrank_chi2          : log-rank test statistic (when group present)
  - logrank_p             : log-rank p-value (when group present)

Validates: Requirements 4.3, 4.8
"""

from __future__ import annotations

from pathlib import Path
from typing import Any

import numpy as np
import pandas as pd

from .adapters import ReferenceAdapter
from .common import StatsCodeInvocationError, compare_scalar, run_stats_code
from .result import Status, ToleranceConfig, ValidationResult

METHOD = "kaplan_meier"
METRICS = ["median_survival", "logrank_chi2", "logrank_p"]

_DEFAULT_SPEC: dict[str, Any] = {
    "duration_col": "time",
    "event_col": "death",
    "group_col": "group",
}


def _python_reference(dataset_path: Path, spec: dict[str, Any]) -> dict[str, float]:
    """Compute Kaplan-Meier estimates using lifelines as reference."""
    from lifelines import KaplanMeierFitter
    from lifelines.statistics import logrank_test

    df = pd.read_csv(dataset_path)
    duration_col = spec["duration_col"]
    event_col = spec["event_col"]
    group_col = spec.get("group_col")

    kmf = KaplanMeierFitter()
    kmf.fit(df[duration_col], event_observed=df[event_col])

    results: dict[str, float] = {}
    results["median_survival"] = float(kmf.median_survival_time_)

    # Log-rank test if group column is present
    if group_col and group_col in df.columns:
        groups = sorted(df[group_col].dropna().unique())
        if len(groups) == 2:
            g0, g1 = groups
            mask0 = df[group_col] == g0
            mask1 = df[group_col] == g1
            lr = logrank_test(
                df.loc[mask0, duration_col], df.loc[mask1, duration_col],
                event_observed_A=df.loc[mask0, event_col],
                event_observed_B=df.loc[mask1, event_col],
            )
            results["logrank_chi2"] = float(lr.test_statistic)
            results["logrank_p"] = float(lr.p_value)

    return results


def collect(
    dataset_path: Path,
    tol_config: ToleranceConfig,
    adapters: list[Any],
    spec: dict[str, Any] | None = None,
) -> list[ValidationResult]:
    """Run Kaplan-Meier parity checks for *dataset_path*."""
    if spec is None:
        spec = _DEFAULT_SPEC

    dataset_label = dataset_path.name
    results: list[ValidationResult] = []

    duration_col = spec["duration_col"]
    event_col = spec["event_col"]
    group_col = spec.get("group_col")

    # ── 1. Call Stats Code CLI ───────────────────────────────────────────────
    cli_args = [
        "--json", "survival", "km",
        "--data", str(dataset_path.resolve()),
        "--time", duration_col,
        "--event", event_col,
    ]
    if group_col:
        cli_args.extend(["--group", group_col])

    try:
        sc_out = run_stats_code(cli_args)
    except StatsCodeInvocationError as exc:
        return [ValidationResult(
            method=METHOD, dataset=dataset_label,
            reference_engine="stats_code_cli", metric="__invoke__",
            tolerance=0.0, status=Status.ERROR, message=str(exc),
        )]

    sc_median = float(sc_out.get("median_survival", float("nan")))
    sc_logrank_chi2 = sc_out.get("logrank_chi2")
    sc_logrank_p = sc_out.get("logrank_p")

    # ── 2. Python reference (lifelines) ─────────────────────────────────────
    ref_name = "lifelines"
    try:
        ref = _python_reference(dataset_path, spec)
    except Exception as exc:
        return [ValidationResult(
            method=METHOD, dataset=dataset_label,
            reference_engine=ref_name, metric="__fit__",
            tolerance=0.0, status=Status.ERROR,
            message=f"Python reference raised: {exc}",
        )]

    # Median survival
    results.append(compare_scalar(
        METHOD, "median_survival", dataset_label,
        ref_name, ref["median_survival"], sc_median, tol_config,
    ))

    # Log-rank (only if both sides have it)
    if "logrank_chi2" in ref and sc_logrank_chi2 is not None:
        results.append(compare_scalar(
            METHOD, "logrank_chi2", dataset_label,
            ref_name, ref["logrank_chi2"], float(sc_logrank_chi2), tol_config,
        ))
    if "logrank_p" in ref and sc_logrank_p is not None:
        results.append(compare_scalar(
            METHOD, "logrank_p", dataset_label,
            ref_name, ref["logrank_p"], float(sc_logrank_p), tol_config,
        ))

    return results
