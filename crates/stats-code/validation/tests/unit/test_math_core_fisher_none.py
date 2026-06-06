# Spec: parity-math-core-collect-crash
"""
Unit tests for the ``math_core`` Fisher-row guard against a ``test_name`` field
that is present-and-``null`` in the ``tableone`` CLI output.

Bug condition C2: ``dict.get("test_name", "")`` returns ``None`` (not the
default) when the key exists with a JSON ``null`` value, so ``None.lower()``
raises ``AttributeError`` and crashes ``math_core.collect()``.

These tests monkeypatch ``run_stats_code`` so no ``cargo`` build / CLI runs.
"""
from __future__ import annotations

from pathlib import Path

import pytest

from parity import math_core
from parity.result import Status, ToleranceConfig


def _tol() -> ToleranceConfig:
    return ToleranceConfig(per_metric={}, default=1e-6)


def _tableone_output_with_null_test_name() -> dict:
    """A tableone-shaped dict with one null-test_name row and one normal row."""
    return {
        "rows": [
            # present-and-null test_name — the crash trigger
            {
                "variable": "disease",
                "test_name": None,
                "p_value": 0.5,
                "groups": [],
            },
            # a normal non-fisher row
            {
                "variable": "age",
                "test_name": "t-test",
                "p_value": 0.1,
                "groups": [],
            },
        ]
    }


def test_fisher_guard_handles_null_test_name(monkeypatch: pytest.MonkeyPatch) -> None:
    """A present-and-null ``test_name`` must NOT raise; the row is treated as
    non-Fisher and skipped, yielding the existing SKIP result."""
    monkeypatch.setattr(
        math_core,
        "run_stats_code",
        lambda args: _tableone_output_with_null_test_name(),
    )

    results = math_core._validate_fisher_exact_via_tableone(
        Path("dummy.csv"), "math_core_indirect", _tol()
    )

    assert isinstance(results, list)
    assert len(results) == 1
    assert results[0].status == Status.SKIP
    assert "No Fisher exact test triggered" in results[0].message


def test_fisher_guard_handles_missing_test_name(monkeypatch: pytest.MonkeyPatch) -> None:
    """A row with the ``test_name`` key entirely absent must also be safe."""
    monkeypatch.setattr(
        math_core,
        "run_stats_code",
        lambda args: {"rows": [{"variable": "disease", "p_value": 0.5, "groups": []}]},
    )

    results = math_core._validate_fisher_exact_via_tableone(
        Path("dummy.csv"), "math_core_indirect", _tol()
    )

    assert len(results) == 1
    assert results[0].status == Status.SKIP
