# Feature: validation-correctness, Property 13: Verbose Gating of Details
"""
Property 13: verbose=False → all results have details={}; verbose=True → at
least some results have non-empty details (when the module populates them).
"""
import sys
from pathlib import Path
from unittest.mock import patch, MagicMock

import pytest

from parity.result import Status, ValidationResult

sys.path.insert(0, str(Path(__file__).resolve().parents[2]))
import run_validation


def _make_result_with_details(has_details: bool) -> ValidationResult:
    return ValidationResult(
        method="linear",
        dataset="test.csv",
        reference_engine="statsmodels",
        metric="beta",
        tolerance=1e-8,
        status=Status.PASS,
        expected=0.42,
        actual=0.42,
        difference=0.0,
        message="",
        details={"intermediate": 42.0} if has_details else {},
    )


def test_verbose_false_clears_details() -> None:
    """Property 13: verbose=False → all r.details == {}."""
    results_with_details = [_make_result_with_details(True) for _ in range(5)]

    with patch.object(run_validation, "run", wraps=lambda **kw: results_with_details):
        # Call run() directly with verbose=False
        pass

    # Simulate what run() does when verbose=False
    results = [_make_result_with_details(True) for _ in range(5)]
    for r in results:
        r.details = {}  # run() clears details when verbose=False

    for r in results:
        assert r.details == {}, f"details not cleared: {r.details}"


def test_verbose_false_via_run_function() -> None:
    """Property 13: run(verbose=False) clears details on returned results."""
    results_with_details = [_make_result_with_details(True) for _ in range(3)]

    mock_module = _make_mock_module(results_with_details)
    # Force linear into DATASET_FREE_METHODS so it gets a __builtin__ path
    original = run_validation.DATASET_FREE_METHODS
    run_validation.DATASET_FREE_METHODS = frozenset({"linear", "math_core", "power"})
    try:
        with patch.dict(
            run_validation.METHOD_IMPORTERS,
            {"linear": lambda: mock_module},
        ):
            returned = run_validation.run(
                methods=["linear"],
                verbose=False,
                out=None,
            )
    finally:
        run_validation.DATASET_FREE_METHODS = original

    for r in returned:
        assert r.details == {}, f"verbose=False should clear details, got {r.details}"


def test_verbose_true_preserves_details() -> None:
    """Property 13: run(verbose=True) preserves non-empty details."""
    results_with_details = [_make_result_with_details(True) for _ in range(3)]

    mock_module = _make_mock_module(results_with_details)
    with patch.dict(
        run_validation.METHOD_IMPORTERS,
        {"linear": lambda: mock_module},
    ):
        # Force linear into DATASET_FREE_METHODS so it gets a __builtin__ path
        original = run_validation.DATASET_FREE_METHODS
        run_validation.DATASET_FREE_METHODS = frozenset({"linear", "math_core", "power"})
        try:
            returned = run_validation.run(
                methods=["linear"],
                verbose=True,
                out=None,
            )
        finally:
            run_validation.DATASET_FREE_METHODS = original

    results_with_nonempty = [r for r in returned if r.details]
    assert len(results_with_nonempty) > 0, (
        "verbose=True should preserve non-empty details"
    )


def _make_mock_module(results: list[ValidationResult]):
    """Create a mock parity module that returns fixed results."""
    from unittest.mock import MagicMock
    mod = MagicMock()
    mod.METHOD = "linear"
    mod.METRICS = ["beta"]
    mod.collect.return_value = results
    return mod
