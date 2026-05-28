"""parity/power_phase3.py — Live parity collector for power_phase3.

Wave-1 stub: returns a single SKIP result with reason
"live_collector_not_yet_implemented" so the test surface exists for
the consistency check (task 11.6) but no real comparison runs yet.
The full Live test surface lands in a follow-up wave alongside the
hypothesis-based synthetic data generators.
"""

from __future__ import annotations

from pathlib import Path
from typing import Any

from .result import Status, ToleranceConfig, ValidationResult

METHOD = "power_phase3"
METRICS: list[str] = []


def collect(
    dataset_path: Path,
    tol_config: ToleranceConfig,
    adapters: list[Any],
) -> list[ValidationResult]:
    return [
        ValidationResult(
            method=METHOD,
            dataset=str(dataset_path),
            reference_engine="not-yet-wired",
            metric=METHOD,
            tolerance=0.0,
            status=Status.SKIP,
            message="live_collector_not_yet_implemented",
        )
    ]
