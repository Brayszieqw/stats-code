"""
parity/power.py — Numerical parity module for sample size / power calculations.

Stats Code CLI subcommands:
  power two-means   --mean1 ... --mean2 ... --sd ... --power ... --alpha ...
  power two-proportions --p1 ... --p2 ... --power ... --alpha ...
  power one-proportion  --proportion ... --margin ... --alpha ...

JSON output fields used:
  - total_n    : total required sample size
  - power      : achieved power (may be absent for one-proportion)
  - effect_size: Cohen's d or h (optional)

This module uses built-in test-point grids rather than CSV datasets.
"""

from __future__ import annotations

from pathlib import Path
from typing import Any

from .adapters import ReferenceAdapter
from .common import StatsCodeInvocationError, compare_scalar, run_stats_code
from .result import Status, ToleranceConfig, ValidationResult

METHOD = "power"
METRICS = ["required_n", "achieved_power"]

# Built-in test-point grid: each entry is a (subcommand, cli_args, spec_for_adapter)
_TEST_POINTS: list[dict[str, Any]] = [
    {
        "label": "two_means_d0.5_power0.8",
        "subcommand": ["power", "two-means"],
        "cli_extra": ["--mean1", "0", "--mean2", "2", "--sd", "4",
                      "--power", "0.8", "--alpha", "0.05"],
        "adapter_spec": {
            "power_type": "two_means",
            "mean_diff": 2.0,
            "std": 4.0,
            "power": 0.8,
            "alpha": 0.05,
        },
    },
    {
        "label": "two_means_d1.0_power0.9",
        "subcommand": ["power", "two-means"],
        "cli_extra": ["--mean1", "0", "--mean2", "4", "--sd", "4",
                      "--power", "0.9", "--alpha", "0.05"],
        "adapter_spec": {
            "power_type": "two_means",
            "mean_diff": 4.0,
            "std": 4.0,
            "power": 0.9,
            "alpha": 0.05,
        },
    },
    {
        "label": "two_proportions_0.3_vs_0.5",
        "subcommand": ["power", "two-proportions"],
        "cli_extra": ["--p1", "0.3", "--p2", "0.5",
                      "--power", "0.8", "--alpha", "0.05"],
        "adapter_spec": {
            "power_type": "two_proportions",
            "p1": 0.3,
            "p2": 0.5,
            "power": 0.8,
            "alpha": 0.05,
        },
    },
]


def collect(
    dataset_path: Path,  # ignored for power (dataset-free method)
    tol_config: ToleranceConfig,
    adapters: list[ReferenceAdapter],
    spec: dict[str, Any] | None = None,
) -> list[ValidationResult]:
    """Run power/sample-size parity checks using built-in test-point grid."""
    results: list[ValidationResult] = []

    for point in _TEST_POINTS:
        label = point["label"]
        cli_args = ["--json"] + point["subcommand"] + point["cli_extra"]
        adapter_spec = point["adapter_spec"]

        # ── 1. Call Stats Code CLI ───────────────────────────────────────────
        try:
            sc_out = run_stats_code(cli_args)
        except StatsCodeInvocationError as exc:
            results.append(ValidationResult(
                method=METHOD, dataset=label,
                reference_engine="stats_code_cli", metric="__invoke__",
                tolerance=0.0, status=Status.ERROR, message=str(exc),
            ))
            continue

        sc_total_n = float(sc_out.get("total_n", float("nan")))
        sc_power_raw = sc_out.get("power")
        sc_power = float(sc_power_raw) if sc_power_raw is not None else None

        # ── 2. Compare against each adapter ─────────────────────────────────
        for adapter in adapters:
            if not adapter.is_available():
                for metric in METRICS:
                    results.append(ValidationResult(
                        method=METHOD, dataset=label,
                        reference_engine=adapter.name, metric=metric,
                        tolerance=tol_config.lookup(METHOD, metric),
                        status=Status.SKIP, message=f"{adapter.name} unavailable",
                    ))
                continue

            try:
                ref = adapter.fit(METHOD, dataset_path, adapter_spec)
            except Exception as exc:
                results.append(ValidationResult(
                    method=METHOD, dataset=label,
                    reference_engine=adapter.name, metric="__fit__",
                    tolerance=0.0, status=Status.ERROR,
                    message=f"adapter.fit() raised: {exc}",
                ))
                continue

            # required_n: integer comparison (tolerance = 0)
            ref_n = ref.get("required_n", float("nan"))
            results.append(compare_scalar(
                METHOD, "required_n", label,
                adapter.name, ref_n, sc_total_n, tol_config,
            ))

            # achieved_power (optional)
            if sc_power is not None and "achieved_power" in ref:
                results.append(compare_scalar(
                    METHOD, "achieved_power", label,
                    adapter.name, float(ref["achieved_power"]), sc_power, tol_config,
                ))

    return results
