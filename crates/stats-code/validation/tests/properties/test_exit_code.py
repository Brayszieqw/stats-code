# Feature: validation-correctness, Property 10: Exit Code Consistency
"""
Property 10: run_validation.main() returns 1 iff any result has FAIL or ERROR.
"""
import sys
from pathlib import Path
from unittest.mock import patch

import pytest
from hypothesis import given, settings
from hypothesis import strategies as st

from parity.result import Status, ValidationResult

# Add validation dir to path for run_validation import
sys.path.insert(0, str(Path(__file__).resolve().parents[2]))
import run_validation


def _make_result(status: Status) -> ValidationResult:
    return ValidationResult(
        method="linear",
        dataset="test.csv",
        reference_engine="statsmodels",
        metric="beta",
        tolerance=1e-8,
        status=status,
        expected=0.42,
        actual=0.42,
        difference=0.0,
        message="",
    )


_status_strategy = st.sampled_from(list(Status))


@given(statuses=st.lists(_status_strategy, min_size=1, max_size=30))
@settings(max_examples=200)
def test_exit_code_matches_fail_or_error(statuses: list) -> None:
    """Property 10: exit code 1 ↔ ∃ FAIL or ERROR in results."""
    results = [_make_result(s) for s in statuses]
    has_failure = any(s in (Status.FAIL, Status.ERROR) for s in statuses)

    # Patch run() to return our controlled results
    with patch.object(run_validation, "run", return_value=results):
        exit_code = run_validation.main(["--out", "/tmp/test_exit_code_out"])

    if has_failure:
        assert exit_code == 1, (
            f"Expected exit code 1 with statuses {statuses}, got {exit_code}"
        )
    else:
        assert exit_code == 0, (
            f"Expected exit code 0 with statuses {statuses}, got {exit_code}"
        )


def test_all_skip_returns_zero() -> None:
    """Property 10: all SKIP → exit code 0."""
    results = [_make_result(Status.SKIP) for _ in range(5)]
    with patch.object(run_validation, "run", return_value=results):
        exit_code = run_validation.main(["--out", "/tmp/test_exit_code_skip"])
    assert exit_code == 0


def test_mix_pass_skip_returns_zero() -> None:
    """Property 10: PASS + SKIP → exit code 0."""
    results = [_make_result(Status.PASS), _make_result(Status.SKIP)]
    with patch.object(run_validation, "run", return_value=results):
        exit_code = run_validation.main(["--out", "/tmp/test_exit_code_pass_skip"])
    assert exit_code == 0
