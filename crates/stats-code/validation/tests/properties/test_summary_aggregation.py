# Feature: validation-correctness, Property 9: Summary Aggregation Fidelity
"""
Property 9: summary counts are faithful to the results list.
"""
import json
from collections import Counter

import pytest
from hypothesis import given, settings
from hypothesis import strategies as st

from parity.reporter import ReportGenerator
from parity.result import RunMetadata, Status, ValidationResult


def _make_metadata() -> RunMetadata:
    return RunMetadata(
        generated_at="2026-05-13T00:00:00+00:00",
        stats_code_commit="abc1234",
        stats_code_version="0.9.0",
        python_version="3.11.0",
        rscript_version="unavailable",
        os="Linux",
        reference_engine_versions={},
    )


_status_strategy = st.sampled_from(list(Status))
_method_strategy = st.sampled_from(["linear", "logistic", "cox"])


def _make_result(method: str, status: Status) -> ValidationResult:
    return ValidationResult(
        method=method,
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


@given(
    entries=st.lists(
        st.tuples(_method_strategy, _status_strategy),
        min_size=0,
        max_size=50,
    )
)
@settings(max_examples=200)
def test_total_comparisons_equals_len(entries: list) -> None:
    """Property 9: summary.total_comparisons == len(results)."""
    results = [_make_result(m, s) for m, s in entries]
    gen = ReportGenerator(results, _make_metadata())
    doc = json.loads(gen.render_json())
    assert doc["summary"]["total_comparisons"] == len(results)


@given(
    entries=st.lists(
        st.tuples(_method_strategy, _status_strategy),
        min_size=1,
        max_size=50,
    )
)
@settings(max_examples=200)
def test_by_method_counts_are_correct(entries: list) -> None:
    """Property 9: by_method counts match actual result distribution."""
    results = [_make_result(m, s) for m, s in entries]
    gen = ReportGenerator(results, _make_metadata())
    doc = json.loads(gen.render_json())

    by_method = doc["summary"]["by_method"]
    for method in {m for m, _ in entries}:
        expected_counts = Counter(s.value for m, s in entries if m == method)
        actual_counts = by_method.get(method, {})
        for status_val, count in expected_counts.items():
            assert actual_counts.get(status_val, 0) == count, (
                f"by_method[{method}][{status_val}]: expected {count}, "
                f"got {actual_counts.get(status_val, 0)}"
            )


@given(
    entries=st.lists(
        st.tuples(_method_strategy, _status_strategy),
        min_size=1,
        max_size=50,
    )
)
@settings(max_examples=200)
def test_overall_status_logic(entries: list) -> None:
    """Property 9: VALIDATED ↔ no FAIL/ERROR and at least one PASS."""
    results = [_make_result(m, s) for m, s in entries]
    gen = ReportGenerator(results, _make_metadata())
    doc = json.loads(gen.render_json())

    statuses = {s for _, s in entries}
    has_fail_or_error = Status.FAIL in statuses or Status.ERROR in statuses
    has_pass = Status.PASS in statuses

    overall = doc["summary"]["status"]
    if has_fail_or_error:
        assert overall == "VALIDATION FAILED", (
            f"Expected VALIDATION FAILED with {statuses}, got {overall}"
        )
    elif has_pass:
        assert overall == "VALIDATED", (
            f"Expected VALIDATED with {statuses}, got {overall}"
        )
