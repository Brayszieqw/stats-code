"""
parity/power_single_arm.py — Live parity collector for single-arm power/sample size.

Stats Code CLI: ``power single-arm --p0 <p0> --p1 <p1> --alpha <alpha> --power <power>``

JSON output fields used:
  - required_n      : required sample size (integer, ceiling)
  - achieved_power  : power achieved at computed N

Validates: Requirements 4.3, 4.8
"""

from __future__ import annotations

from pathlib import Path
from typing import Any

import numpy as np

from .adapters import ReferenceAdapter
from .common import StatsCodeInvocationError, compare_scalar, run_stats_code
from .result import Status, ToleranceConfig, ValidationResult

METHOD = "power_single_arm"
METRICS = ["required_n", "achieved_power"]

_DEFAULT_SPEC: dict[str, Any] = {
    "p0": 0.20,
    "p1": 0.35,
    "alpha": 0.05,
    "power": 0.80,
}


def _python_reference(spec: dict[str, Any]) -> dict[str, float]:
    """Compute single-arm power using statsmodels as reference.

    For a one-proportion test: H0: p = p0  vs  H1: p = p1
    """
    from statsmodels.stats.power import NormalIndPower
    from statsmodels.stats.proportion import proportion_effectsize

    p0 = float(spec["p0"])
    p1 = float(spec["p1"])
    alpha = float(spec.get("alpha", 0.05))
    power = float(spec.get("power", 0.80))

    # Effect size for one proportion (arcsine transform)
    effect = proportion_effectsize(p1, p0)

    analysis = NormalIndPower()
    n = analysis.solve_power(
        effect_size=abs(effect), alpha=alpha, power=power, ratio=0,
        alternative="two-sided",
    )
    required_n = float(np.ceil(n))

    # Achieved power at the computed N
    achieved_power = float(analysis.solve_power(
        effect_size=abs(effect), alpha=alpha, nobs1=required_n, ratio=0,
        alternative="two-sided",
    ))

    return {
        "required_n": required_n,
        "achieved_power": achieved_power,
    }


def collect(
    dataset_path: Path,
    tol_config: ToleranceConfig,
    adapters: list[Any],
    spec: dict[str, Any] | None = None,
) -> list[ValidationResult]:
    """Run single-arm power/sample size parity checks."""
    if spec is None:
        spec = _DEFAULT_SPEC

    dataset_label = "power_single_arm"
    results: list[ValidationResult] = []

    p0 = spec["p0"]
    p1 = spec["p1"]
    alpha = spec.get("alpha", 0.05)
    power_target = spec.get("power", 0.80)

    # ── 1. Call Stats Code CLI ───────────────────────────────────────────────
    try:
        sc_out = run_stats_code([
            "--json", "power", "single-arm",
            "--p0", str(p0),
            "--p1", str(p1),
            "--alpha", str(alpha),
            "--power", str(power_target),
        ])
    except StatsCodeInvocationError as exc:
        return [ValidationResult(
            method=METHOD, dataset=dataset_label,
            reference_engine="stats_code_cli", metric="__invoke__",
            tolerance=0.0, status=Status.ERROR, message=str(exc),
        )]

    sc_n = float(sc_out.get("required_n", float("nan")))
    sc_power = float(sc_out.get("achieved_power", float("nan")))

    # ── 2. Python reference (statsmodels) ───────────────────────────────────
    ref_name = "statsmodels"
    try:
        ref = _python_reference(spec)
    except Exception as exc:
        return [ValidationResult(
            method=METHOD, dataset=dataset_label,
            reference_engine=ref_name, metric="__fit__",
            tolerance=0.0, status=Status.ERROR,
            message=f"Python reference raised: {exc}",
        )]

    results.append(compare_scalar(
        METHOD, "required_n", dataset_label,
        ref_name, ref["required_n"], sc_n, tol_config,
    ))
    results.append(compare_scalar(
        METHOD, "achieved_power", dataset_label,
        ref_name, ref["achieved_power"], sc_power, tol_config,
    ))

    return results
