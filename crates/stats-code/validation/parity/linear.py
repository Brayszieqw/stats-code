"""
parity/linear.py — Numerical parity module for linear regression.

Stats Code CLI JSON output fields used:
  - coefficients[].term          : "Intercept" | covariate name
  - coefficients[].beta          : coefficient estimate
  - coefficients[].standard_error: standard error
  - coefficients[].t_statistic   : t-statistic
  - coefficients[].p_value       : p-value
  - r_squared                    : R²
  - adjusted_r_squared           : adjusted R²
  - f_statistic                  : F-statistic (optional)
  - f_p_value                    : F-test p-value (optional)
"""

from __future__ import annotations

from pathlib import Path
from typing import Any

from .adapters import ReferenceAdapter
from .common import StatsCodeInvocationError, compare_scalar, run_stats_code
from .result import Status, ToleranceConfig, ValidationResult

METHOD = "linear"
METRICS = [
    "beta",
    "stderr",
    "t_stat",
    "pvalue",
    "r_squared",
    "adj_r_squared",
    "f_stat",
]

# Default covariates and outcome used when spec is not provided.
# These match the synthetic dataset column names from gen_synthetic.py.
_DEFAULT_SPEC: dict[str, Any] = {
    "outcome": "linear_y",
    "covariates": ["age", "bmi"],
}


def collect(
    dataset_path: Path,
    tol_config: ToleranceConfig,
    adapters: list[ReferenceAdapter],
    spec: dict[str, Any] | None = None,
) -> list[ValidationResult]:
    """
    Run linear regression parity checks for *dataset_path*.

    Parameters
    ----------
    dataset_path: Path to the CSV dataset.
    tol_config:   Tolerance configuration.
    adapters:     Reference engine adapters to compare against.
    spec:         Optional override for outcome/covariates.
                  Defaults to ``{"outcome": "linear_y", "covariates": ["age", "bmi"]}``.

    Returns
    -------
    list[ValidationResult]
    """
    if spec is None:
        spec = _DEFAULT_SPEC

    dataset_label = dataset_path.name
    results: list[ValidationResult] = []

    # ── 1. Call Stats Code CLI ───────────────────────────────────────────────
    outcome = spec["outcome"]
    covariates: list[str] = spec["covariates"]
    cov_str = ",".join(covariates)

    try:
        sc_out = run_stats_code([
            "--json",
            "model", "linear",
            "--data", str(dataset_path.resolve()),
            "--y", outcome,
            "--x", cov_str,
        ])
    except StatsCodeInvocationError as exc:
        return [ValidationResult(
            method=METHOD,
            dataset=dataset_label,
            reference_engine="stats_code_cli",
            metric="__invoke__",
            tolerance=0.0,
            status=Status.ERROR,
            message=str(exc),
        )]

    # Parse coefficient map: term → {beta, standard_error, t_statistic, p_value}
    sc_coefs: dict[str, dict[str, float]] = {}
    for item in sc_out.get("coefficients", []):
        term = item.get("term", "")
        sc_coefs[term] = {
            "beta":     float(item.get("beta", float("nan"))),
            "stderr":   float(item.get("standard_error", float("nan"))),
            "t_stat":   float(item.get("t_statistic", float("nan"))),
            "pvalue":   float(item.get("p_value", float("nan"))),
        }

    sc_r2     = float(sc_out.get("r_squared", float("nan")))
    sc_adj_r2 = float(sc_out.get("adjusted_r_squared", float("nan")))
    sc_f      = sc_out.get("f_statistic")
    sc_f_p    = sc_out.get("f_p_value")

    # ── 2. Compare against each adapter ─────────────────────────────────────
    for adapter in adapters:
        if not adapter.is_available():
            for metric in METRICS:
                results.append(ValidationResult(
                    method=METHOD,
                    dataset=dataset_label,
                    reference_engine=adapter.name,
                    metric=metric,
                    tolerance=tol_config.lookup(METHOD, metric),
                    status=Status.SKIP,
                    message=f"{adapter.name} unavailable",
                ))
            continue

        try:
            ref = adapter.fit(METHOD, dataset_path, spec)
        except Exception as exc:
            results.append(ValidationResult(
                method=METHOD,
                dataset=dataset_label,
                reference_engine=adapter.name,
                metric="__fit__",
                tolerance=0.0,
                status=Status.ERROR,
                message=f"adapter.fit() raised: {exc}",
            ))
            continue

        # Per-covariate metrics
        for cov in covariates:
            for metric_key, ref_key in [
                ("beta",   f"beta[{cov}]"),
                ("stderr", f"stderr[{cov}]"),
                ("t_stat", f"t_stat[{cov}]"),
                ("pvalue", f"pvalue[{cov}]"),
            ]:
                sc_val = sc_coefs.get(cov, {}).get(metric_key, float("nan"))
                ref_val = ref.get(ref_key, float("nan"))
                results.append(compare_scalar(
                    METHOD, f"{metric_key}[{cov}]", dataset_label,
                    adapter.name, ref_val, sc_val, tol_config,
                ))

        # Intercept
        intercept_term = "Intercept"
        for metric_key, ref_key in [
            ("beta",   "beta[const]"),
            ("stderr", "stderr[const]"),
            ("t_stat", "t_stat[const]"),
            ("pvalue", "pvalue[const]"),
        ]:
            sc_val = sc_coefs.get(intercept_term, {}).get(metric_key, float("nan"))
            ref_val = ref.get(ref_key, float("nan"))
            results.append(compare_scalar(
                METHOD, f"{metric_key}[Intercept]", dataset_label,
                adapter.name, ref_val, sc_val, tol_config,
            ))

        # Model-level metrics
        for metric_key, sc_val, ref_key in [
            ("r_squared",     sc_r2,     "r_squared"),
            ("adj_r_squared", sc_adj_r2, "adj_r_squared"),
        ]:
            ref_val = ref.get(ref_key, float("nan"))
            results.append(compare_scalar(
                METHOD, metric_key, dataset_label,
                adapter.name, ref_val, sc_val, tol_config,
            ))

        # F-statistic (optional)
        if sc_f is not None and "f_stat" in ref:
            results.append(compare_scalar(
                METHOD, "f_stat", dataset_label,
                adapter.name, float(ref["f_stat"]), float(sc_f), tol_config,
            ))
        if sc_f_p is not None and "f_pvalue" in ref:
            results.append(compare_scalar(
                METHOD, "f_pvalue", dataset_label,
                adapter.name, float(ref["f_pvalue"]), float(sc_f_p), tol_config,
            ))

    return results
