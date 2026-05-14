"""
parity/survival.py — Numerical parity module for Kaplan–Meier survival analysis.

Stats Code CLI: ``survival km --data ... --time ... --event ... [--by ...]``

JSON output fields used:
  - steps[].group        : group label (or "overall")
  - steps[].time         : event time
  - steps[].survival     : KM survival probability
  - steps[].standard_error: Greenwood SE
  - log_rank.chi_square  : log-rank chi-square statistic
  - log_rank.p_value     : log-rank p-value
"""

from __future__ import annotations

from pathlib import Path
from typing import Any

from .adapters import ReferenceAdapter
from .common import StatsCodeInvocationError, compare_scalar, compare_vector, run_stats_code
from .result import Status, ToleranceConfig, ValidationResult

METHOD = "survival"
METRICS = [
    "survival_probability",
    "greenwood_se",
    "median_survival",
    "logrank_chi2",
    "logrank_p",
]

_DEFAULT_SPEC: dict[str, Any] = {
    "duration_col": "time",
    "event_col": "death",
    "group_col": None,  # set to a column name to enable log-rank
}


def collect(
    dataset_path: Path,
    tol_config: ToleranceConfig,
    adapters: list[ReferenceAdapter],
    spec: dict[str, Any] | None = None,
) -> list[ValidationResult]:
    """Run Kaplan–Meier parity checks for *dataset_path*."""
    if spec is None:
        spec = _DEFAULT_SPEC

    dataset_label = dataset_path.name
    results: list[ValidationResult] = []

    duration_col = spec["duration_col"]
    event_col = spec["event_col"]
    group_col = spec.get("group_col")

    # ── 1. Call Stats Code CLI ───────────────────────────────────────────────
    cli_args = [
        "--json",
        "survival", "km",
        "--data", str(dataset_path.resolve()),
        "--time", duration_col,
        "--event", event_col,
    ]
    if group_col:
        cli_args += ["--by", group_col]

    try:
        sc_out = run_stats_code(cli_args)
    except StatsCodeInvocationError as exc:
        return [ValidationResult(
            method=METHOD, dataset=dataset_label,
            reference_engine="stats_code_cli", metric="__invoke__",
            tolerance=0.0, status=Status.ERROR, message=str(exc),
        )]

    # Extract overall (group == "overall" or first group) survival steps
    steps = sc_out.get("steps", [])
    overall_steps = [s for s in steps if s.get("group", "").lower() in ("overall", "")]
    if not overall_steps:
        overall_steps = steps  # fallback: use all steps

    sc_survival = [float(s["survival"]) for s in overall_steps]
    sc_se = [float(s["standard_error"]) for s in overall_steps]

    log_rank = sc_out.get("log_rank")
    sc_logrank_chi2 = float(log_rank["chi_square"]) if log_rank else None
    sc_logrank_p = float(log_rank["p_value"]) if log_rank else None

    # ── 2. Compare against each adapter ─────────────────────────────────────
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

        try:
            ref = adapter.fit(METHOD, dataset_path, spec)
        except Exception as exc:
            results.append(ValidationResult(
                method=METHOD, dataset=dataset_label,
                reference_engine=adapter.name, metric="__fit__",
                tolerance=0.0, status=Status.ERROR,
                message=f"adapter.fit() raised: {exc}",
            ))
            continue

        # Survival probability vector
        ref_survival = [
            ref[f"survival_probability[{i}]"]
            for i in range(len(sc_survival))
            if f"survival_probability[{i}]" in ref
        ]
        if ref_survival:
            results.extend(compare_vector(
                METHOD, "survival_probability", dataset_label,
                adapter.name, ref_survival, sc_survival[:len(ref_survival)], tol_config,
            ))

        # Greenwood SE vector
        ref_se = [
            ref[f"greenwood_se[{i}]"]
            for i in range(len(sc_se))
            if f"greenwood_se[{i}]" in ref
        ]
        if ref_se:
            results.extend(compare_vector(
                METHOD, "greenwood_se", dataset_label,
                adapter.name, ref_se, sc_se[:len(ref_se)], tol_config,
            ))

        # Median survival
        if "median_survival" in ref:
            # Stats Code doesn't expose median directly in JSON; skip if missing
            results.append(ValidationResult(
                method=METHOD, dataset=dataset_label,
                reference_engine=adapter.name, metric="median_survival",
                tolerance=tol_config.lookup(METHOD, "median_survival"),
                status=Status.SKIP,
                message="Stats Code does not expose median_survival in JSON output",
            ))

        # Log-rank
        if sc_logrank_chi2 is not None and "logrank_chi2" in ref:
            results.append(compare_scalar(
                METHOD, "logrank_chi2", dataset_label, adapter.name,
                float(ref["logrank_chi2"]), sc_logrank_chi2, tol_config,
            ))
        if sc_logrank_p is not None and "logrank_p" in ref:
            results.append(compare_scalar(
                METHOD, "logrank_p", dataset_label, adapter.name,
                float(ref["logrank_p"]), sc_logrank_p, tol_config,
            ))

    return results
