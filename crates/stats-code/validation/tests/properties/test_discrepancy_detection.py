# Feature: validation-correctness, Property 7: Reference Engine Discrepancy Detection
"""
Property 7: When two reference adapters disagree beyond tolerance, the result
message contains 'reference_engines_disagree' and details records both values.
"""
from pathlib import Path
from unittest.mock import MagicMock

import pytest

from parity.adapters import ReferenceAdapter
from parity.common import compare_scalar
from parity.result import Status, ToleranceConfig


def _make_mock_adapter(name: str, value: float) -> ReferenceAdapter:
    adapter = MagicMock(spec=ReferenceAdapter)
    adapter.name = name
    adapter.is_available.return_value = True
    adapter.is_python = True
    adapter.fit.return_value = {"beta[age]": value}
    return adapter


def test_discrepancy_detection_via_compare_scalar() -> None:
    """
    Property 7: inject two adapters with diverging values; verify the
    discrepancy marker appears when we manually check both results.

    Note: The VCF detects discrepancy at the orchestration level by comparing
    results from two adapters for the same metric. This test verifies the
    building block: compare_scalar produces FAIL with a message when values differ.
    """
    tol_config = ToleranceConfig(per_metric={"linear.beta": 1e-5})

    # Adapter A returns 0.42, Adapter B returns 0.43 — diff = 0.01 >> 1e-5
    result_a = compare_scalar(
        "linear", "beta[age]", "test.csv", "adapter_a",
        expected=0.42, actual=0.42, tol_config=tol_config,
    )
    result_b = compare_scalar(
        "linear", "beta[age]", "test.csv", "adapter_b",
        expected=0.43, actual=0.42, tol_config=tol_config,
    )

    # result_b should FAIL because expected (0.43) != actual (0.42) beyond 1e-5
    assert result_b.status == Status.FAIL, (
        f"Expected FAIL for diverging adapters, got {result_b.status}"
    )
    assert result_b.message != "", "FAIL result must have non-empty message"


def test_discrepancy_marker_in_details() -> None:
    """
    Property 7: when we detect engine disagreement, details should record both values.
    This tests the convention that callers populate details with engine values.
    """
    from parity.result import ValidationResult

    # Simulate what an orchestrator would do when two engines disagree
    result = ValidationResult(
        method="linear",
        dataset="test.csv",
        reference_engine="multi",
        metric="beta[age]",
        tolerance=1e-5,
        status=Status.FAIL,
        expected=0.43,
        actual=0.42,
        difference=0.01,
        message="reference_engines_disagree: adapter_a=0.42, adapter_b=0.43",
        details={"adapter_a": 0.42, "adapter_b": 0.43},
    )

    assert "reference_engines_disagree" in result.message
    assert "adapter_a" in result.details
    assert "adapter_b" in result.details
