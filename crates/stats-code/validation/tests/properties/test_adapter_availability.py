# Feature: validation-correctness, Property 6: Adapter Availability Gating
"""
Property 6: When is_available() returns False, all results have status SKIP
with a non-empty message.
"""
from pathlib import Path
from unittest.mock import patch

import pytest

from parity import linear, logistic, cox
from parity.adapters import ADAPTERS_FOR, StatsmodelsAdapter
from parity.result import Status, ToleranceConfig

VALIDATION_DIR = Path(__file__).resolve().parents[2]
CONFIG_PATH = VALIDATION_DIR / "tolerance_config.yaml"
SMALL_CSV = VALIDATION_DIR / "datasets" / "synthetic" / "small_n40.csv"


@pytest.fixture(scope="module")
def tol_config() -> ToleranceConfig:
    return ToleranceConfig.from_yaml(CONFIG_PATH)


@pytest.mark.skipif(not SMALL_CSV.exists(), reason="small_n40.csv not generated yet")
def test_unavailable_adapter_produces_only_skip(tol_config: ToleranceConfig) -> None:
    """Property 6: unavailable adapter → all results SKIP with non-empty message."""
    with patch.object(StatsmodelsAdapter, "is_available", return_value=False):
        adapters = [a for a in ADAPTERS_FOR["linear"] if isinstance(a, StatsmodelsAdapter)]
        if not adapters:
            pytest.skip("No StatsmodelsAdapter registered for linear")

        results = linear.collect(
            dataset_path=SMALL_CSV,
            tol_config=tol_config,
            adapters=adapters,
        )

    for r in results:
        assert r.status == Status.SKIP, (
            f"Expected SKIP for unavailable adapter, got {r.status} for {r.metric}"
        )
        assert r.message != "", (
            f"SKIP result for {r.metric} has empty message"
        )


@pytest.mark.skipif(not SMALL_CSV.exists(), reason="small_n40.csv not generated yet")
def test_available_adapter_produces_no_unavailability_skips(
    tol_config: ToleranceConfig,
) -> None:
    """Property 6 (converse): available adapter → no SKIP due to unavailability."""
    sm_adapter = StatsmodelsAdapter()
    if not sm_adapter.is_available():
        pytest.skip("statsmodels not installed")

    results = linear.collect(
        dataset_path=SMALL_CSV,
        tol_config=tol_config,
        adapters=[sm_adapter],
    )

    unavailability_skips = [
        r for r in results
        if r.status == Status.SKIP and "unavailable" in r.message.lower()
    ]
    assert len(unavailability_skips) == 0, (
        f"Available adapter produced unavailability SKIPs: {unavailability_skips}"
    )
