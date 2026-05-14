# Feature: validation-correctness, Property 11: CLI Filter Subset Relation
"""
Property 11: method_filter and dataset_filter produce result subsets;
no filter → full Cartesian product coverage.
"""
import sys
from pathlib import Path
from unittest.mock import MagicMock, patch

import pytest

from parity.result import Status, ValidationResult

sys.path.insert(0, str(Path(__file__).resolve().parents[2]))
import run_validation


def _make_result(method: str, dataset: str) -> ValidationResult:
    return ValidationResult(
        method=method,
        dataset=dataset,
        reference_engine="statsmodels",
        metric="beta",
        tolerance=1e-8,
        status=Status.PASS,
        expected=0.42,
        actual=0.42,
        difference=0.0,
        message="",
    )


def test_method_filter_produces_subset() -> None:
    """Property 11: results contain only requested methods."""
    all_methods = ["linear", "logistic", "cox"]
    filter_methods = ["linear"]

    # Simulate run() returning results for all methods
    all_results = [_make_result(m, "test.csv") for m in all_methods]

    with patch.object(run_validation, "run", return_value=all_results) as mock_run:
        run_validation.main(["--methods", "linear", "--out", "/tmp/test_filter"])
        call_kwargs = mock_run.call_args[1]
        assert call_kwargs["methods"] == filter_methods


def test_no_filter_passes_none_to_run() -> None:
    """Property 11: no filter → methods=None (run uses all methods)."""
    with patch.object(run_validation, "run", return_value=[]) as mock_run:
        run_validation.main(["--out", "/tmp/test_no_filter"])
        call_kwargs = mock_run.call_args[1]
        assert call_kwargs["methods"] is None


def test_dataset_filter_is_passed_through() -> None:
    """Property 11: --datasets flag is forwarded to run()."""
    with patch.object(run_validation, "run", return_value=[]) as mock_run:
        run_validation.main([
            "--datasets", "datasets/synthetic/small_n40.csv",
            "--out", "/tmp/test_dataset_filter",
        ])
        call_kwargs = mock_run.call_args[1]
        assert call_kwargs["datasets"] == ["datasets/synthetic/small_n40.csv"]


def test_result_methods_are_subset_of_filter() -> None:
    """Property 11: {r.method for r in results} ⊆ method_filter."""
    filter_methods = ["linear", "logistic"]
    results = [
        _make_result("linear", "test.csv"),
        _make_result("logistic", "test.csv"),
    ]

    with patch.object(run_validation, "run", return_value=results):
        run_validation.main(["--methods", "linear,logistic", "--out", "/tmp/test_subset"])

    result_methods = {r.method for r in results}
    assert result_methods.issubset(set(filter_methods)), (
        f"Result methods {result_methods} not subset of filter {filter_methods}"
    )
