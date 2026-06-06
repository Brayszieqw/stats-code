# Spec: parity-math-core-collect-crash
"""
Unit tests for the legacy ``ReportGenerator`` collect-crash distinction.

A collect-crash row (status==ERROR, metric=="__collect__") must be:
  - counted separately in summary.collect_crash (subset of summary.error),
  - flagged per-row via is_collect_crash in JSON,
  - listed under a dedicated "## Collect Crashes" Markdown section,
  - absent from the generic "## Errors" section,
while numeric FAIL rows keep their numeric fields verbatim.
"""
from __future__ import annotations

import json

from parity.reporter import ReportGenerator
from parity.result import RunMetadata, Status, ValidationResult


def _metadata() -> RunMetadata:
    return RunMetadata(
        generated_at="2026-05-13T00:00:00+00:00",
        stats_code_commit="abc1234",
        stats_code_version="0.9.0",
        python_version="3.13.0",
        rscript_version="unavailable",
        os="Windows-10",
        reference_engine_versions={},
    )


def _mixed_results() -> list[ValidationResult]:
    return [
        # collect-crash row
        ValidationResult(
            method="math_core", dataset="__builtin__",
            reference_engine="unknown", metric="__collect__",
            tolerance=0.0, status=Status.ERROR,
            message="collect() raised: AttributeError",
        ),
        # numeric FAIL row with full numeric fields
        ValidationResult(
            method="linear", dataset="small_n40.csv",
            reference_engine="statsmodels", metric="beta[age]",
            tolerance=1e-8, status=Status.FAIL,
            expected=1.0, actual=1.5, difference=0.5,
            message="difference 5e-1 exceeds tolerance 1e-8",
        ),
        # generic (non-crash) ERROR row
        ValidationResult(
            method="tableone", dataset="small_n40.csv",
            reference_engine="stats_code_cli", metric="__invoke__",
            tolerance=0.0, status=Status.ERROR, message="CLI exploded",
        ),
        # a pass
        ValidationResult(
            method="linear", dataset="small_n40.csv",
            reference_engine="statsmodels", metric="r_squared",
            tolerance=1e-8, status=Status.PASS,
            expected=0.9, actual=0.9, difference=0.0,
        ),
    ]


def test_json_collect_crash_count_and_flag() -> None:
    gen = ReportGenerator(_mixed_results(), _metadata())
    doc = json.loads(gen.render_json())

    summary = doc["summary"]
    assert summary["collect_crash"] == 1
    # error count is inclusive of the collect crash (1 crash + 1 generic = 2)
    assert summary["error"] == 2
    assert summary["error"] >= summary["collect_crash"]
    assert summary["fail"] == 1

    by_metric = {r["metric"]: r for r in doc["results"]}
    assert by_metric["__collect__"]["is_collect_crash"] is True
    assert by_metric["beta[age]"]["is_collect_crash"] is False
    assert by_metric["__invoke__"]["is_collect_crash"] is False

    # numeric FAIL fields preserved verbatim
    fail_row = by_metric["beta[age]"]
    assert fail_row["expected"] == 1.0
    assert fail_row["actual"] == 1.5
    assert fail_row["difference"] == 0.5
    assert fail_row["tolerance"] == 1e-8


def test_markdown_has_dedicated_collect_crash_section() -> None:
    gen = ReportGenerator(_mixed_results(), _metadata())
    md = gen.render_markdown()

    assert "## Collect Crashes" in md
    # crash listed under its own section
    crash_idx = md.index("## Collect Crashes")
    assert "math_core" in md[crash_idx:]

    # crash must not be duplicated in the Errors section
    assert "## Errors" in md
    errors_idx = md.index("## Errors")
    # scope to just the Errors section (up to the next "## " heading)
    rest = md[errors_idx + len("## Errors"):]
    next_heading = rest.find("\n## ")
    errors_section = rest if next_heading == -1 else rest[:next_heading]
    assert "__collect__" not in errors_section
    # the generic error IS present in the Errors section
    assert "CLI exploded" in errors_section


def test_no_collect_crash_section_when_absent() -> None:
    results = [
        ValidationResult(
            method="linear", dataset="d.csv",
            reference_engine="statsmodels", metric="beta",
            tolerance=1e-8, status=Status.PASS,
            expected=1.0, actual=1.0, difference=0.0,
        )
    ]
    gen = ReportGenerator(results, _metadata())
    doc = json.loads(gen.render_json())
    assert doc["summary"]["collect_crash"] == 0
    assert "## Collect Crashes" not in gen.render_markdown()
