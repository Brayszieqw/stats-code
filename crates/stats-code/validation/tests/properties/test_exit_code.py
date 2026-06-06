# Feature: validation-correctness, Property 10: Exit Code Consistency
"""
Property 10: run_validation.main() exits 0 iff results are all-pass + no
``reference_software_unavailable`` SKIPs; otherwise it exits with the
documented non-zero code from the Rust parity subcommand exit-code map
(2 / 3 / 4 / 5). Per task 11.5 of the parity-and-multilang-sidecar spec
the FAIL / ERROR branch maps to exit code 2 (Requirement 5.7).
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
    """Property 10: exit code 2 ↔ ∃ FAIL or ERROR in results.

    The Rust parity subcommand maps fail rows onto exit code 2
    (`ParityOutcome::FailRows`); this Python entry point mirrors that map
    so CI gets the same cause class either way.
    """
    results = [_make_result(s) for s in statuses]
    has_failure = any(s in (Status.FAIL, Status.ERROR) for s in statuses)

    # Patch run() to return our controlled results
    with patch.object(run_validation, "run", return_value=results):
        exit_code = run_validation.main(["--out", "/tmp/test_exit_code_out"])

    if has_failure:
        assert exit_code == 2, (
            f"Expected exit code 2 with statuses {statuses}, got {exit_code}"
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


# ── Spec: parity-math-core-collect-crash ─────────────────────────────────────
# Property 6: a single collect-crash row (status==ERROR, metric=="__collect__")
# yields exit 2, and compute_exit_code is unaffected by the reporter changes.

def _collect_crash_row() -> ValidationResult:
    return ValidationResult(
        method="math_core",
        dataset="__builtin__",
        reference_engine="unknown",
        metric="__collect__",
        tolerance=0.0,
        status=Status.ERROR,
        message="collect() raised: boom",
    )


def test_single_collect_crash_yields_exit_2() -> None:
    """Property 6: one collect-crash row + only PASS/SKIP otherwise → exit 2."""
    results = [_collect_crash_row(), _make_result(Status.PASS), _make_result(Status.SKIP)]
    assert run_validation.compute_exit_code(results) == 2


def test_compute_exit_code_ignores_collect_crash_metric_marker() -> None:
    """Property 6: compute_exit_code treats the crash purely via Status.ERROR;
    the __collect__ marker does not change the mapping (a clean run is 0)."""
    clean = [_make_result(Status.PASS), _make_result(Status.SKIP)]
    assert run_validation.compute_exit_code(clean) == 0
    # adding a collect-crash row flips it to 2 (same as any ERROR row)
    assert run_validation.compute_exit_code(clean + [_collect_crash_row()]) == 2
