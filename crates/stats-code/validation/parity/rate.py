"""
parity/rate.py — Numerical parity module for incidence rate analysis.

Stats Code CLI: ``rate --data ... --event ... --person-time ...``

JSON output fields used (from rows[0] for unstratified):
  - rows[].rate_per_1000       : incidence rate per 1000 person-time
  - rows[].byar_ci_lower       : Byar CI lower bound
  - rows[].byar_ci_upper       : Byar CI upper bound
"""

from __future__ import annotations

from pathlib import Path
from typing import Any

from .adapters import ReferenceAdapter
from .common import StatsCodeInvocationError, compare_scalar, run_stats_code
from .result import Status, ToleranceConfig, ValidationResult

METHOD = "rate"
METRICS = [
    "estimate_per_1000",
    "byar_ci_lower",
    "byar_ci_upper",
]

_DEFAULT_SPEC: dict[str, Any] = {
    "events_col": "death",
    "person_time_col": "time",
    "multiplier": 1000.0,
    "alpha": 0.05,
}


def collect(
    dataset_path: Path,
    tol_config: ToleranceConfig,
    adapters: list[ReferenceAdapter],
    spec: dict[str, Any] | None = None,
) -> list[ValidationResult]:
    """Run incidence rate parity checks for *dataset_path*."""
    if spec is None:
        spec = _DEFAULT_SPEC

    dataset_label = dataset_path.name
    results: list[ValidationResult] = []

    events_col = spec["events_col"]
    person_time_col = spec["person_time_col"]

    # ── 1. Call Stats Code CLI ───────────────────────────────────────────────
    try:
        sc_out = run_stats_code([
            "--json",
            "rate",
            "--data", str(dataset_path.resolve()),
            "--event", events_col,
            "--person-time", person_time_col,
        ])
    except StatsCodeInvocationError as exc:
        return [ValidationResult(
            method=METHOD, dataset=dataset_label,
            reference_engine="stats_code_cli", metric="__invoke__",
            tolerance=0.0, status=Status.ERROR, message=str(exc),
        )]

    # Use the first (overall / unstratified) row
    rows = sc_out.get("rows", [])
    if not rows:
        return [ValidationResult(
            method=METHOD, dataset=dataset_label,
            reference_engine="stats_code_cli", metric="__parse__",
            tolerance=0.0, status=Status.ERROR,
            message="No rows in rate JSON output",
        )]

    row = rows[0]
    sc_rate = float(row.get("rate_per_1000", float("nan")))
    sc_lower = float(row.get("byar_ci_lower", float("nan")))
    sc_upper = float(row.get("byar_ci_upper", float("nan")))

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

        for metric_key, sc_val, ref_key in [
            ("estimate_per_1000", sc_rate,  "estimate_per_1000"),
            ("byar_ci_lower",     sc_lower, "byar_ci_lower"),
            ("byar_ci_upper",     sc_upper, "byar_ci_upper"),
        ]:
            ref_val = ref.get(ref_key, float("nan"))
            results.append(compare_scalar(
                METHOD, metric_key, dataset_label,
                adapter.name, ref_val, sc_val, tol_config,
            ))

    return results
