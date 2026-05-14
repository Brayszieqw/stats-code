"""
parity/cox.py — Numerical parity module for Cox proportional hazards regression.

Stats Code CLI JSON output fields used:
  - coefficients[].term          : covariate name
  - coefficients[].beta          : log-hazard coefficient
  - coefficients[].standard_error: standard error
  - coefficients[].hazard_ratio  : exp(beta)
  - coefficients[].p_value       : Wald p-value
  - log_partial_likelihood       : log partial likelihood at convergence
  - concordance                  : Harrell's C-index (optional)
"""

from __future__ import annotations

from pathlib import Path
from typing import Any

from .adapters import ReferenceAdapter
from .common import StatsCodeInvocationError, compare_scalar, run_stats_code
from .result import Status, ToleranceConfig, ValidationResult

METHOD = "cox"
METRICS = [
    "beta",
    "stderr",
    "hazard_ratio",
    "pvalue",
    "log_partial_likelihood",
    "concordance",
]

_DEFAULT_SPEC: dict[str, Any] = {
    "duration_col": "time",
    "event_col": "death",
    "covariates": ["age", "bmi"],
}


def collect(
    dataset_path: Path,
    tol_config: ToleranceConfig,
    adapters: list[ReferenceAdapter],
    spec: dict[str, Any] | None = None,
) -> list[ValidationResult]:
    """
    Run Cox regression parity checks for *dataset_path*.
    """
    if spec is None:
        spec = _DEFAULT_SPEC

    dataset_label = dataset_path.name
    results: list[ValidationResult] = []

    duration_col = spec["duration_col"]
    event_col = spec["event_col"]
    covariates: list[str] = spec["covariates"]

    # ── 1. Call Stats Code CLI ───────────────────────────────────────────────
    try:
        sc_out = run_stats_code([
            "--json",
            "model", "cox",
            "--data", str(dataset_path.resolve()),
            "--time", duration_col,
            "--event", event_col,
            "--x", ",".join(covariates),
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

    # Parse coefficient map
    sc_coefs: dict[str, dict[str, float]] = {}
    for item in sc_out.get("coefficients", []):
        term = item.get("term", "")
        sc_coefs[term] = {
            "beta":         float(item.get("beta", float("nan"))),
            "stderr":       float(item.get("standard_error", float("nan"))),
            "hazard_ratio": float(item.get("hazard_ratio", float("nan"))),
            "pvalue":       float(item.get("p_value", float("nan"))),
        }

    sc_lpl = float(sc_out.get("log_partial_likelihood", float("nan")))
    sc_concordance_raw = sc_out.get("concordance")
    sc_concordance = float(sc_concordance_raw) if sc_concordance_raw is not None else None

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
                ("beta",         f"beta[{cov}]"),
                ("stderr",       f"stderr[{cov}]"),
                ("hazard_ratio", f"hazard_ratio[{cov}]"),
                ("pvalue",       f"pvalue[{cov}]"),
            ]:
                sc_val = sc_coefs.get(cov, {}).get(metric_key, float("nan"))
                ref_val = ref.get(ref_key, float("nan"))
                results.append(compare_scalar(
                    METHOD, f"{metric_key}[{cov}]", dataset_label,
                    adapter.name, ref_val, sc_val, tol_config,
                ))

        # Model-level: log partial likelihood
        results.append(compare_scalar(
            METHOD, "log_partial_likelihood", dataset_label,
            adapter.name,
            ref.get("log_partial_likelihood", float("nan")),
            sc_lpl,
            tol_config,
        ))

        # Concordance (optional — skip if Stats Code doesn't expose it)
        if "concordance" in ref:
            if sc_concordance is not None:
                results.append(compare_scalar(
                    METHOD, "concordance", dataset_label,
                    adapter.name, float(ref["concordance"]), sc_concordance, tol_config,
                ))
            else:
                results.append(ValidationResult(
                    method=METHOD,
                    dataset=dataset_label,
                    reference_engine=adapter.name,
                    metric="concordance",
                    tolerance=tol_config.lookup(METHOD, "concordance"),
                    status=Status.SKIP,
                    message="Stats Code does not expose 'concordance' in JSON output",
                ))

    return results
