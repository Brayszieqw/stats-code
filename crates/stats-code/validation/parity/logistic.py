"""
parity/logistic.py — Numerical parity module for logistic regression.

Stats Code CLI JSON output fields used:
  - coefficients[].term          : "Intercept" | covariate name
  - coefficients[].beta          : log-odds coefficient
  - coefficients[].standard_error: standard error
  - coefficients[].p_value       : Wald p-value
  - coefficients[].odds_ratio    : exp(beta)
  - log_likelihood               : log-likelihood at convergence
  (c_statistic and nagelkerke_r2 are computed by the adapter from predictions)
"""

from __future__ import annotations

from pathlib import Path
from typing import Any

from .adapters import ReferenceAdapter
from .common import StatsCodeInvocationError, compare_scalar, run_stats_code
from .result import Status, ToleranceConfig, ValidationResult

METHOD = "logistic"
METRICS = [
    "beta",
    "stderr",
    "wald",
    "pvalue",
    "odds_ratio",
    "log_likelihood",
    "c_statistic",
    "nagelkerke_r2",
]

_DEFAULT_SPEC: dict[str, Any] = {
    "outcome": "disease",
    "covariates": ["age", "bmi"],
}


def collect(
    dataset_path: Path,
    tol_config: ToleranceConfig,
    adapters: list[ReferenceAdapter],
    spec: dict[str, Any] | None = None,
) -> list[ValidationResult]:
    """
    Run logistic regression parity checks for *dataset_path*.
    """
    if spec is None:
        spec = _DEFAULT_SPEC

    dataset_label = dataset_path.name
    results: list[ValidationResult] = []

    outcome = spec["outcome"]
    covariates: list[str] = spec["covariates"]
    cov_str = ",".join(covariates)

    # ── 1. Call Stats Code CLI ───────────────────────────────────────────────
    try:
        sc_out = run_stats_code([
            "--json",
            "model", "logistic",
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

    # Parse coefficient map
    sc_coefs: dict[str, dict[str, float]] = {}
    for item in sc_out.get("coefficients", []):
        term = item.get("term", "")
        sc_coefs[term] = {
            "beta":       float(item.get("beta", float("nan"))),
            "stderr":     float(item.get("standard_error", float("nan"))),
            # Stats Code uses p_value; wald stat = beta / stderr
            "wald":       (
                float(item.get("beta", float("nan"))) /
                float(item.get("standard_error", 1.0))
                if float(item.get("standard_error", 0.0)) != 0.0
                else float("nan")
            ),
            "pvalue":     float(item.get("p_value", float("nan"))),
            "odds_ratio": float(item.get("odds_ratio", float("nan"))),
        }

    sc_ll = float(sc_out.get("log_likelihood", float("nan")))

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
                ("beta",       f"beta[{cov}]"),
                ("stderr",     f"stderr[{cov}]"),
                ("wald",       f"wald[{cov}]"),
                ("pvalue",     f"pvalue[{cov}]"),
                ("odds_ratio", f"odds_ratio[{cov}]"),
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
            ("beta",       "beta[const]"),
            ("stderr",     "stderr[const]"),
            ("wald",       "wald[const]"),
            ("pvalue",     "pvalue[const]"),
            ("odds_ratio", "odds_ratio[const]"),
        ]:
            sc_val = sc_coefs.get(intercept_term, {}).get(metric_key, float("nan"))
            ref_val = ref.get(ref_key, float("nan"))
            results.append(compare_scalar(
                METHOD, f"{metric_key}[Intercept]", dataset_label,
                adapter.name, ref_val, sc_val, tol_config,
            ))

        # Model-level metrics
        results.append(compare_scalar(
            METHOD, "log_likelihood", dataset_label,
            adapter.name, ref.get("log_likelihood", float("nan")), sc_ll, tol_config,
        ))

        # Optional: c_statistic and nagelkerke_r2 (adapter may not provide these)
        for metric_key in ("c_statistic", "nagelkerke_r2"):
            if metric_key in ref:
                # Stats Code may not expose these directly; skip if missing
                sc_val_raw = sc_out.get(metric_key)
                if sc_val_raw is not None:
                    results.append(compare_scalar(
                        METHOD, metric_key, dataset_label,
                        adapter.name, float(ref[metric_key]), float(sc_val_raw), tol_config,
                    ))
                else:
                    results.append(ValidationResult(
                        method=METHOD,
                        dataset=dataset_label,
                        reference_engine=adapter.name,
                        metric=metric_key,
                        tolerance=tol_config.lookup(METHOD, metric_key),
                        status=Status.SKIP,
                        message=f"Stats Code does not expose '{metric_key}' in JSON output",
                    ))

    return results
