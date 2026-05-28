"""
parity/or_rr.py — Live parity collector for odds ratio / relative risk.

Stats Code CLI: ``or-rr --data ... --exposure <col> --outcome <col>``

JSON output fields used:
  - odds_ratio     : OR point estimate
  - or_ci_lower    : OR 95% CI lower
  - or_ci_upper    : OR 95% CI upper
  - relative_risk  : RR point estimate
  - rr_ci_lower    : RR 95% CI lower
  - rr_ci_upper    : RR 95% CI upper

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

METHOD = "or_rr"
METRICS = ["odds_ratio", "or_ci_lower", "or_ci_upper",
           "relative_risk", "rr_ci_lower", "rr_ci_upper"]

_DEFAULT_SPEC: dict[str, Any] = {
    "exposure": "group",
    "outcome": "disease",
}


def _python_reference(dataset_path: Path, spec: dict[str, Any]) -> dict[str, float]:
    """Compute OR and RR from a 2x2 table using manual formulae + scipy."""
    df = pd.read_csv(dataset_path)
    exposure_col = spec["exposure"]
    outcome_col = spec["outcome"]

    # Build 2x2 contingency table
    # exposure=1,outcome=1 → a; exposure=1,outcome=0 → b
    # exposure=0,outcome=1 → c; exposure=0,outcome=0 → d
    a = int(((df[exposure_col] == 1) & (df[outcome_col] == 1)).sum())
    b = int(((df[exposure_col] == 1) & (df[outcome_col] == 0)).sum())
    c = int(((df[exposure_col] == 0) & (df[outcome_col] == 1)).sum())
    d = int(((df[exposure_col] == 0) & (df[outcome_col] == 0)).sum())

    # Odds ratio
    if b == 0 or c == 0:
        or_val = float("nan")
    else:
        or_val = (a * d) / (b * c)

    # OR 95% CI (Woolf's method)
    if a > 0 and b > 0 and c > 0 and d > 0:
        log_or = np.log(or_val)
        se_log_or = np.sqrt(1.0 / a + 1.0 / b + 1.0 / c + 1.0 / d)
        or_ci_lower = np.exp(log_or - 1.96 * se_log_or)
        or_ci_upper = np.exp(log_or + 1.96 * se_log_or)
    else:
        or_ci_lower = float("nan")
        or_ci_upper = float("nan")

    # Relative risk
    n_exposed = a + b
    n_unexposed = c + d
    if n_exposed == 0 or n_unexposed == 0:
        rr_val = float("nan")
    else:
        risk_exposed = a / n_exposed
        risk_unexposed = c / n_unexposed
        if risk_unexposed == 0:
            rr_val = float("nan")
        else:
            rr_val = risk_exposed / risk_unexposed

    # RR 95% CI (log method)
    if a > 0 and n_exposed > 0 and c > 0 and n_unexposed > 0 and rr_val > 0:
        log_rr = np.log(rr_val)
        se_log_rr = np.sqrt(
            (1.0 / a - 1.0 / n_exposed) + (1.0 / c - 1.0 / n_unexposed)
        )
        rr_ci_lower = np.exp(log_rr - 1.96 * se_log_rr)
        rr_ci_upper = np.exp(log_rr + 1.96 * se_log_rr)
    else:
        rr_ci_lower = float("nan")
        rr_ci_upper = float("nan")

    return {
        "odds_ratio": float(or_val),
        "or_ci_lower": float(or_ci_lower),
        "or_ci_upper": float(or_ci_upper),
        "relative_risk": float(rr_val),
        "rr_ci_lower": float(rr_ci_lower),
        "rr_ci_upper": float(rr_ci_upper),
    }


def collect(
    dataset_path: Path,
    tol_config: ToleranceConfig,
    adapters: list[Any],
    spec: dict[str, Any] | None = None,
) -> list[ValidationResult]:
    """Run OR/RR parity checks for *dataset_path*."""
    if spec is None:
        spec = _DEFAULT_SPEC

    dataset_label = dataset_path.name
    results: list[ValidationResult] = []

    exposure_col = spec["exposure"]
    outcome_col = spec["outcome"]

    # ── 1. Call Stats Code CLI ───────────────────────────────────────────────
    try:
        sc_out = run_stats_code([
            "--json", "or-rr",
            "--data", str(dataset_path.resolve()),
            "--exposure", exposure_col,
            "--outcome", outcome_col,
        ])
    except StatsCodeInvocationError as exc:
        return [ValidationResult(
            method=METHOD, dataset=dataset_label,
            reference_engine="stats_code_cli", metric="__invoke__",
            tolerance=0.0, status=Status.ERROR, message=str(exc),
        )]

    # ── 2. Python reference ─────────────────────────────────────────────────
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
        sc_val = float(sc_out.get(metric_key, float("nan")))
        results.append(compare_scalar(
            METHOD, metric_key, dataset_label,
            ref_name, ref[metric_key], sc_val, tol_config,
        ))

    return results
