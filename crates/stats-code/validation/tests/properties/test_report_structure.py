# Feature: validation-correctness, Property 8: Report Structural Completeness
"""
Property 8: ReportGenerator.write() produces report.json and report.md with
required structure regardless of input results.
"""
import json
import tempfile
from pathlib import Path

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
        reference_engine_versions={"statsmodels": "0.14.1", "scipy": "1.11.4"},
    )


def _make_result(status: Status) -> ValidationResult:
    return ValidationResult(
        method="linear",
        dataset="test.csv",
        reference_engine="statsmodels",
        metric="beta[age]",
        tolerance=1e-8,
        status=status,
        expected=0.42,
        actual=0.42 if status == Status.PASS else 0.50,
        difference=0.0 if status == Status.PASS else 0.08,
        message="" if status == Status.PASS else "difference exceeds tolerance",
    )


def test_write_produces_both_files() -> None:
    """Property 8a: write() produces report.json and report.md."""
    results = [_make_result(Status.PASS), _make_result(Status.SKIP)]
    gen = ReportGenerator(results, _make_metadata())

    with tempfile.TemporaryDirectory() as tmp:
        out = Path(tmp)
        gen.write(out)
        assert (out / "report.json").exists(), "report.json not created"
        assert (out / "report.md").exists(), "report.md not created"


def test_json_is_valid_and_has_required_keys() -> None:
    """Property 8b: report.json is valid JSON with required metadata keys."""
    results = [_make_result(Status.PASS)]
    gen = ReportGenerator(results, _make_metadata())
    json_str = gen.render_json()

    doc = json.loads(json_str)
    assert "schema_version" in doc
    assert "metadata" in doc
    assert "summary" in doc
    assert "results" in doc

    meta = doc["metadata"]
    for key in ("generated_at", "stats_code_commit", "python_version", "os", "reference_engines"):
        assert key in meta, f"metadata missing key '{key}'"
        assert meta[key] not in (None, ""), f"metadata['{key}'] is empty"


def test_markdown_has_required_sections() -> None:
    """Property 8c: report.md has top-level header, Summary, and Failures sections."""
    results = [_make_result(Status.FAIL), _make_result(Status.PASS)]
    gen = ReportGenerator(results, _make_metadata())
    md = gen.render_markdown()

    assert "# Stats Code Validation Report" in md, "Missing top-level header"
    assert "## Summary" in md, "Missing Summary section"
    assert "## Failures" in md, "Missing Failures section"


def test_failures_section_lists_fail_results() -> None:
    """Property 8c: Failures section lists every FAIL result."""
    results = [_make_result(Status.FAIL)]
    gen = ReportGenerator(results, _make_metadata())
    md = gen.render_markdown()

    # The FAIL result's metric should appear in the Failures section
    assert "beta[age]" in md


def test_empty_results_produces_valid_report() -> None:
    """Property 8: empty results list produces valid (but minimal) report."""
    gen = ReportGenerator([], _make_metadata())

    json_str = gen.render_json()
    doc = json.loads(json_str)
    assert doc["summary"]["total_comparisons"] == 0
    assert doc["summary"]["status"] == "NO COMPARISONS"

    md = gen.render_markdown()
    assert "# Stats Code Validation Report" in md
