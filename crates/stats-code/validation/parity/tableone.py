"""
parity/tableone.py — Numerical parity module for Table One descriptive statistics.

Stats Code CLI: ``tableone --data ... --by <group_col> --vars <var1,var2,...>``

JSON output fields used (from rows[]):
  - rows[].variable          : variable name
  - rows[].overall.mean      : overall mean
  - rows[].overall.sd        : overall SD
  - rows[].overall.median    : overall median
  - rows[].overall.q1        : overall Q1
  - rows[].overall.q3        : overall Q3
  - rows[].overall.count     : count (categorical)
  - rows[].overall.percent   : percent (categorical)
  - rows[].p_value           : test p-value
  - rows[].test_name         : test name (t-test, chi2, kruskal, fisher)
"""

from __future__ import annotations

from pathlib import Path
from typing import Any

import numpy as np
import pandas as pd

from .adapters import ReferenceAdapter
from .common import StatsCodeInvocationError, compare_scalar, run_stats_code
from .result import Status, ToleranceConfig, ValidationResult

METHOD = "tableone"
METRICS = [
    "mean", "std", "median", "q1", "q3",
    "count", "proportion",
    "t_stat", "chi2_stat", "kw_stat", "pvalue",
]

_DEFAULT_SPEC: dict[str, Any] = {
    "by": "group",
    "vars": ["age", "bmi", "disease"],
}


def _ref_stats(df: pd.DataFrame, var: str, by: str) -> dict[str, float]:
    """Compute reference statistics for one variable using pandas/scipy."""
    from scipy import stats as sp_stats

    results: dict[str, float] = {}
    col = df[var].dropna()

    # Overall descriptive stats
    results["mean"] = float(col.mean())
    results["std"] = float(col.std(ddof=1))
    results["median"] = float(col.median())
    results["q1"] = float(col.quantile(0.25))
    results["q3"] = float(col.quantile(0.75))

    # Categorical: count and proportion of value == 1 (binary)
    if set(col.unique()).issubset({0, 1, 0.0, 1.0}):
        results["count"] = float(int(col.sum()))
        results["proportion"] = float(col.mean())

    # Group-level test
    groups = df[by].dropna().unique()
    group_data = [df.loc[df[by] == g, var].dropna().values for g in groups]

    if len(groups) == 2:
        # t-test for continuous
        if not set(col.unique()).issubset({0, 1, 0.0, 1.0}):
            t_stat, _ = sp_stats.ttest_ind(group_data[0], group_data[1], equal_var=False)
            results["t_stat"] = float(t_stat)
        # chi-square for binary
        else:
            contingency = pd.crosstab(df[var], df[by])
            if contingency.shape == (2, 2):
                chi2, p, _, _ = sp_stats.chi2_contingency(contingency, correction=False)
                results["chi2_stat"] = float(chi2)
                results["pvalue_chi2"] = float(p)

    if len(groups) >= 2:
        # Kruskal-Wallis
        if len(group_data) >= 2 and all(len(g) > 0 for g in group_data):
            try:
                kw_stat, kw_p = sp_stats.kruskal(*group_data)
                results["kw_stat"] = float(kw_stat)
                results["pvalue_kw"] = float(kw_p)
            except Exception:
                pass

    return results


def collect(
    dataset_path: Path,
    tol_config: ToleranceConfig,
    adapters: list[ReferenceAdapter],
    spec: dict[str, Any] | None = None,
) -> list[ValidationResult]:
    """Run Table One parity checks for *dataset_path*."""
    if spec is None:
        spec = _DEFAULT_SPEC

    dataset_label = dataset_path.name
    results: list[ValidationResult] = []

    by_col = spec["by"]
    vars_list: list[str] = spec["vars"]
    vars_str = ",".join(vars_list)

    # ── 1. Call Stats Code CLI ───────────────────────────────────────────────
    try:
        sc_out = run_stats_code([
            "--json", "tableone",
            "--data", str(dataset_path.resolve()),
            "--by", by_col,
            "--vars", vars_str,
        ])
    except StatsCodeInvocationError as exc:
        return [ValidationResult(
            method=METHOD, dataset=dataset_label,
            reference_engine="stats_code_cli", metric="__invoke__",
            tolerance=0.0, status=Status.ERROR, message=str(exc),
        )]

    # Build lookup: variable → row
    sc_rows: dict[str, dict] = {}
    for row in sc_out.get("rows", []):
        var = row.get("variable", "")
        if var not in sc_rows:
            sc_rows[var] = row

    # ── 2. Compare against each adapter ─────────────────────────────────────
    df = pd.read_csv(dataset_path)

    for adapter in adapters:
        if not adapter.is_available():
            for metric in METRICS:
                results.append(ValidationResult(
                    method=METHOD, dataset=dataset_label,
                    reference_engine=adapter.name, metric=metric,
                    tolerance=tol_config.lookup(METHOD, metric),
                    status=Status.SKIP, message=f"{adapter.name} unavailable",
                ))
            continue

        for var in vars_list:
            if var not in df.columns:
                continue
            if var not in sc_rows:
                continue

            row = sc_rows[var]
            overall = row.get("overall", {})

            # Compute reference stats
            try:
                ref = _ref_stats(df, var, by_col)
            except Exception as exc:
                results.append(ValidationResult(
                    method=METHOD, dataset=dataset_label,
                    reference_engine=adapter.name, metric=f"__ref_stats[{var}]__",
                    tolerance=0.0, status=Status.ERROR,
                    message=f"_ref_stats raised: {exc}",
                ))
                continue

            # Continuous descriptive stats
            for metric_key, sc_key in [
                ("mean",   "mean"),
                ("std",    "sd"),
                ("median", "median"),
                ("q1",     "q1"),
                ("q3",     "q3"),
            ]:
                sc_val_raw = overall.get(sc_key)
                if sc_val_raw is None or metric_key not in ref:
                    continue
                results.append(compare_scalar(
                    METHOD, f"{metric_key}[{var}]", dataset_label,
                    adapter.name, ref[metric_key], float(sc_val_raw), tol_config,
                ))

            # Categorical: count and proportion
            for metric_key, sc_key, ref_key in [
                ("count",      "count",   "count"),
                ("proportion", "percent", "proportion"),
            ]:
                sc_val_raw = overall.get(sc_key)
                if sc_val_raw is None or ref_key not in ref:
                    continue
                # percent in Stats Code is 0–100; proportion in ref is 0–1
                sc_val = float(sc_val_raw) / 100.0 if sc_key == "percent" else float(sc_val_raw)
                results.append(compare_scalar(
                    METHOD, f"{metric_key}[{var}]", dataset_label,
                    adapter.name, ref[ref_key], sc_val, tol_config,
                ))

            # Test statistics and p-value
            sc_pvalue = row.get("p_value")
            test_name = (row.get("test_name") or "").lower()

            if sc_pvalue is not None:
                # Match test type to reference stat
                if "t" in test_name and "t_stat" in ref:
                    results.append(compare_scalar(
                        METHOD, f"t_stat[{var}]", dataset_label,
                        adapter.name, ref["t_stat"],
                        float(row.get("t_stat", float("nan"))), tol_config,
                    ))
                if "chi" in test_name and "chi2_stat" in ref:
                    results.append(compare_scalar(
                        METHOD, f"chi2_stat[{var}]", dataset_label,
                        adapter.name, ref["chi2_stat"],
                        float(row.get("chi2_stat", float("nan"))), tol_config,
                    ))
                if "kruskal" in test_name and "kw_stat" in ref:
                    results.append(compare_scalar(
                        METHOD, f"kw_stat[{var}]", dataset_label,
                        adapter.name, ref["kw_stat"],
                        float(row.get("kw_stat", float("nan"))), tol_config,
                    ))

                # p-value comparison (use kw_p as reference for kruskal, chi2_p for chi2)
                ref_p_key = "pvalue_kw" if "kruskal" in test_name else "pvalue_chi2"
                if ref_p_key in ref:
                    results.append(compare_scalar(
                        METHOD, f"pvalue[{var}]", dataset_label,
                        adapter.name, ref[ref_p_key], float(sc_pvalue), tol_config,
                    ))

    return results
