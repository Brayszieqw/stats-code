"""Unit tests for ParityReportGenerator (task 11.4).

Validates Requirements 3.1, 3.2, 3.3, 3.5, 3.6, 3.7, 12.4, 12.5.

Covered cases:
  - happy path: 2 rows render to valid JSON and HTML
  - numeric formatting: at least 12 significant digits via ``f"{x:.12e}"``
  - skipped row renders ``n/a`` for missing numeric values
  - header fields are all present in the rendered output
  - determinism: same input ⇒ byte-identical JSON
  - written files land under ``<run_id>/report.json`` and ``report.html``
"""

from __future__ import annotations

import json
import re
from pathlib import Path

import pytest

from parity.reporter import (
    NA_LITERAL,
    PARITY_REPORT_SCHEMA_VERSION,
    ParityReportGenerator,
    _fmt_numeric,
)
from parity.result import (
    ParityReportHeader,
    ParityRow,
    ParityVerdict,
    ReferenceImplDescriptor,
    SkippedReason,
)


# ---------------------------------------------------------------------------
# Fixtures
# ---------------------------------------------------------------------------


def _pass_row() -> ParityRow:
    """A typical PASS row, populated with values typical for a logistic beta."""
    return ParityRow(
        algorithm_id="logistic",
        algorithm_display_name="Logistic Regression",
        software="R",
        reference_impl=ReferenceImplDescriptor(
            name="stats::glm", pkg="stats", version="4.4.1"
        ),
        case_id="synthetic_n100_seed42",
        metric="beta[age]",
        stats_engine_value=0.123456789012,
        reference_value_or_na=0.123456789013,
        absolute_difference=1.0e-12,
        relative_difference=8.1e-12,
        active_absolute_tolerance=1e-9,
        active_relative_tolerance=1e-6,
        verdict=ParityVerdict.PASS,
        skipped_reason=None,
    )


def _skipped_row() -> ParityRow:
    """A SKIPPED row with all numeric fields None — exercises ``n/a`` rendering."""
    return ParityRow(
        algorithm_id="cox",
        algorithm_display_name="Cox Proportional Hazards",
        software="SAS",
        reference_impl=ReferenceImplDescriptor(
            name="PROC PHREG", pkg=None, version="9.4M8"
        ),
        case_id="recorded_phreg_case_01",
        metric="beta[treatment]",
        stats_engine_value=None,
        reference_value_or_na=None,
        absolute_difference=None,
        relative_difference=None,
        active_absolute_tolerance=1e-7,
        active_relative_tolerance=1e-4,
        verdict=ParityVerdict.SKIPPED,
        skipped_reason=SkippedReason.REFERENCE_SOFTWARE_UNAVAILABLE,
    )


def _header() -> ParityReportHeader:
    """A header populated with realistic fields for all required sections."""
    return ParityReportHeader(
        commit_sha="0123456789abcdef0123456789abcdef01234567",
        run_started_at_utc="2025-04-26T20:39:31Z",
        host_os_family="Windows",
        host_os_version="10.0.19045",
        # Insertion order intentionally NOT sorted — the reporter must sort
        # for determinism.
        reference_software_versions={"R": "4.4.1", "Python": "3.11.9"},
        coverage_matrix={
            "schema_version": 1,
            "release_version": "0.1.0-test",
            "algorithms": {
                "logistic": {"R": "live", "SAS": "recorded", "Python": "live", "SPSS": "none"},
                "cox": {"R": "live", "SAS": "recorded", "Python": "live", "SPSS": "none"},
            },
        },
        tolerance_diff=[
            {
                "algorithm": "logistic.beta",
                "previous": "1.0e-5",
                "new": "1.0e-6",
                "pr_id": "PR-1234",
            }
        ],
    )


# ---------------------------------------------------------------------------
# _fmt_numeric (Requirement 3.3, 12.5)
# ---------------------------------------------------------------------------


def test_fmt_numeric_renders_12_significant_digits():
    """``f"{x:.12e}"`` yields one digit before the decimal point + 12 after,
    so the rendered string has 13 sig digits — clears the ≥12 floor."""
    rendered = _fmt_numeric(0.123456789012345)

    # Strip the exponent, count digits in the mantissa.
    mantissa = rendered.split("e")[0]
    digits = re.sub(r"[^0-9]", "", mantissa)
    assert len(digits) >= 12, (
        f"expected ≥12 sig digits in mantissa, got {len(digits)} in {rendered!r}"
    )

    # Sanity: the format keeps the magnitude correct.
    assert float(rendered) == pytest.approx(0.123456789012345, rel=1e-13)


def test_fmt_numeric_renders_none_as_na_literal():
    assert _fmt_numeric(None) == NA_LITERAL == "n/a"


def test_fmt_numeric_handles_zero_negative_and_large_values():
    assert _fmt_numeric(0.0) == "0.000000000000e+00"
    # negative
    assert _fmt_numeric(-1.5).startswith("-1.500000000000")
    # large
    rendered = _fmt_numeric(1.5e10 + 1.0)
    assert "e+10" in rendered


# ---------------------------------------------------------------------------
# Happy path: 2 rows render to valid JSON + HTML (Requirement 3.1, 3.2)
# ---------------------------------------------------------------------------


def test_happy_path_two_rows_render_to_valid_json():
    gen = ParityReportGenerator(rows=[_pass_row(), _skipped_row()], header=_header())

    json_str = gen.render_json()
    doc = json.loads(json_str)

    assert doc["schema_version"] == PARITY_REPORT_SCHEMA_VERSION
    assert "header" in doc
    assert "rows" in doc
    assert len(doc["rows"]) == 2

    # Row 1 (PASS) — verify the 14 fields plus reference_impl substructure.
    row1 = doc["rows"][0]
    expected_top_level_keys = {
        "algorithm_id",
        "algorithm_display_name",
        "software",
        "reference_impl",
        "case_id",
        "metric",
        "stats_engine_value",
        "reference_value_or_na",
        "absolute_difference",
        "relative_difference",
        "active_absolute_tolerance",
        "active_relative_tolerance",
        "verdict",
        "skipped_reason",
    }
    assert set(row1.keys()) == expected_top_level_keys
    assert row1["algorithm_id"] == "logistic"
    assert row1["software"] == "R"
    assert row1["verdict"] == "pass"
    assert row1["reference_impl"] == {
        "name": "stats::glm",
        "pkg": "stats",
        "version": "4.4.1",
    }


def test_happy_path_two_rows_render_to_valid_html():
    gen = ParityReportGenerator(rows=[_pass_row(), _skipped_row()], header=_header())

    html_str = gen.render_html()

    # Basic shape — well-formed enough that downstream tools / browsers can render.
    assert html_str.startswith("<!DOCTYPE html>")
    assert html_str.rstrip().endswith("</html>")
    assert "<table>" in html_str
    assert "</table>" in html_str

    # Both rows show up in the body.
    assert "Logistic Regression" in html_str
    assert "Cox Proportional Hazards" in html_str

    # Verdict CSS classes exist on the rendered cells.
    assert "verdict-pass" in html_str
    assert "verdict-skipped" in html_str


# ---------------------------------------------------------------------------
# Numeric formatting (Requirement 3.3, 12.5)
# ---------------------------------------------------------------------------


def test_numeric_fields_are_rendered_with_12_significant_digits_in_json():
    gen = ParityReportGenerator(rows=[_pass_row()], header=_header())
    doc = json.loads(gen.render_json())
    row = doc["rows"][0]

    numeric_keys = (
        "stats_engine_value",
        "reference_value_or_na",
        "absolute_difference",
        "relative_difference",
        "active_absolute_tolerance",
        "active_relative_tolerance",
    )
    for key in numeric_keys:
        rendered = row[key]
        assert rendered != NA_LITERAL, f"{key}: PASS row should not be n/a"
        # ``e`` exponent token confirms the .12e format was used.
        assert "e" in rendered, f"{key} = {rendered!r} missing exponent"
        mantissa = rendered.split("e")[0]
        digits = re.sub(r"[^0-9]", "", mantissa)
        assert len(digits) >= 12, (
            f"{key} = {rendered!r} has {len(digits)} sig digits, need ≥12"
        )


# ---------------------------------------------------------------------------
# Skipped row → n/a (Requirement 3.3)
# ---------------------------------------------------------------------------


def test_skipped_row_renders_na_for_missing_numeric_fields():
    gen = ParityReportGenerator(rows=[_skipped_row()], header=_header())
    doc = json.loads(gen.render_json())
    row = doc["rows"][0]

    # Numeric fields that ParityRow stored as None must render as "n/a".
    for key in (
        "stats_engine_value",
        "reference_value_or_na",
        "absolute_difference",
        "relative_difference",
    ):
        assert row[key] == NA_LITERAL, f"{key} should be n/a, got {row[key]!r}"

    # Tolerances are still present numerically — they come from
    # tolerance_config.yaml and are not None on a skipped row.
    assert row["active_absolute_tolerance"] != NA_LITERAL
    assert row["active_relative_tolerance"] != NA_LITERAL

    # Skipped reason carries through.
    assert row["verdict"] == "skipped"
    assert row["skipped_reason"] == "reference_software_unavailable"


def test_skipped_row_renders_na_in_html():
    gen = ParityReportGenerator(rows=[_skipped_row()], header=_header())
    html_str = gen.render_html()
    # The PASS-row mantissa would appear here too if rows leaked; restrict
    # to a single skipped row so any "n/a" came from this row alone.
    assert "n/a" in html_str
    assert "reference_software_unavailable" in html_str


# ---------------------------------------------------------------------------
# Header fields all present (Requirements 3.6, 3.7, 12.4)
# ---------------------------------------------------------------------------


def test_header_fields_all_present_in_json():
    gen = ParityReportGenerator(rows=[_pass_row()], header=_header())
    doc = json.loads(gen.render_json())
    h = doc["header"]

    expected_header_keys = {
        "commit_sha",
        "run_started_at_utc",
        "host_os_family",
        "host_os_version",
        "reference_software_versions",
        "coverage_matrix",
        "tolerance_diff",
    }
    assert set(h.keys()) == expected_header_keys

    assert h["commit_sha"] == "0123456789abcdef0123456789abcdef01234567"
    assert h["run_started_at_utc"] == "2025-04-26T20:39:31Z"
    assert h["host_os_family"] == "Windows"
    assert h["host_os_version"] == "10.0.19045"
    assert len(h["host_os_version"]) <= 32  # Requirement 9.2 guarantee from caller

    # reference_software_versions is sorted by key — determinism requirement.
    assert list(h["reference_software_versions"].keys()) == sorted(
        h["reference_software_versions"].keys()
    )
    assert h["reference_software_versions"] == {"Python": "3.11.9", "R": "4.4.1"}

    # coverage_matrix preserves the `none` marker (Requirement 3.7).
    cov = h["coverage_matrix"]
    assert cov["algorithms"]["logistic"]["SPSS"] == "none"
    assert cov["algorithms"]["cox"]["SPSS"] == "none"

    # tolerance_diff entries flow through verbatim (Requirement 12.4).
    assert h["tolerance_diff"] == [
        {
            "algorithm": "logistic.beta",
            "previous": "1.0e-5",
            "new": "1.0e-6",
            "pr_id": "PR-1234",
        }
    ]


def test_header_fields_all_present_in_html():
    gen = ParityReportGenerator(rows=[_pass_row()], header=_header())
    html_str = gen.render_html()

    # Literal, escape-safe strings show up directly.
    assert "0123456789abcdef0123456789abcdef01234567" in html_str
    assert "2025-04-26T20:39:31Z" in html_str
    assert "Windows" in html_str
    assert "10.0.19045" in html_str
    assert "PR-1234" in html_str
    # Coverage matrix embedded as JSON in <pre> — the reporter HTML-escapes
    # the JSON text, so quotes appear as ``&quot;``. The ``none`` marker
    # itself flows through verbatim per Requirement 3.7.
    assert "&quot;none&quot;" in html_str


# ---------------------------------------------------------------------------
# Determinism (Requirement 3.5 — release-asset stability)
# ---------------------------------------------------------------------------


def test_determinism_same_input_yields_byte_identical_json():
    rows = [_pass_row(), _skipped_row()]
    header = _header()

    out1 = ParityReportGenerator(rows=list(rows), header=header).render_json().encode("utf-8")
    out2 = ParityReportGenerator(rows=list(rows), header=header).render_json().encode("utf-8")
    assert out1 == out2

    # Even when the dict insertion order on the input differs, output is stable
    # because the reporter sorts reference_software_versions internally.
    header_shuffled = ParityReportHeader(
        commit_sha=header.commit_sha,
        run_started_at_utc=header.run_started_at_utc,
        host_os_family=header.host_os_family,
        host_os_version=header.host_os_version,
        reference_software_versions={
            # different insertion order
            "Python": "3.11.9",
            "R": "4.4.1",
        },
        coverage_matrix=header.coverage_matrix,
        tolerance_diff=header.tolerance_diff,
    )
    out3 = (
        ParityReportGenerator(rows=list(rows), header=header_shuffled)
        .render_json()
        .encode("utf-8")
    )
    assert out1 == out3


def test_render_json_uses_lf_line_endings():
    """JSON output must be LF-only — release assets must not pick up CRLF
    when generated on Windows runners (Requirement 3.5 stability)."""
    gen = ParityReportGenerator(rows=[_pass_row()], header=_header())
    payload = gen.render_json()
    assert "\r" not in payload


# ---------------------------------------------------------------------------
# Disk write (Requirement 3.1)
# ---------------------------------------------------------------------------


def test_write_emits_report_json_and_report_html_in_run_id_dir(tmp_path: Path):
    out_dir = tmp_path / "abc123-runid"  # mirrors validation/reports/<run_id>/
    gen = ParityReportGenerator(rows=[_pass_row(), _skipped_row()], header=_header())
    gen.write(out_dir)

    json_path = out_dir / "report.json"
    html_path = out_dir / "report.html"
    assert json_path.exists()
    assert html_path.exists()

    # Round-trip the JSON file to confirm it parses.
    doc = json.loads(json_path.read_text(encoding="utf-8"))
    assert doc["schema_version"] == PARITY_REPORT_SCHEMA_VERSION
    assert len(doc["rows"]) == 2

    # HTML loads as UTF-8 text and is non-trivial.
    html_text = html_path.read_text(encoding="utf-8")
    assert "<html" in html_text
    assert "Stats Code Parity Validation Report" in html_text

    # Bytes-on-disk are LF-only — Windows runners must not insert CRLF.
    json_bytes = json_path.read_bytes()
    html_bytes = html_path.read_bytes()
    assert b"\r" not in json_bytes
    assert b"\r" not in html_bytes


def test_write_creates_intermediate_directories(tmp_path: Path):
    """The reporter must mkdir parents — runs may target a fresh
    ``reports/<run_id>/`` path that does not exist yet."""
    out_dir = tmp_path / "deep" / "nested" / "run_id"
    gen = ParityReportGenerator(rows=[_pass_row()], header=_header())
    gen.write(out_dir)
    assert (out_dir / "report.json").exists()
    assert (out_dir / "report.html").exists()


# ---------------------------------------------------------------------------
# Sanity: the legacy ReportGenerator surface is still present
# (we must not break existing consumers — task instruction)
# ---------------------------------------------------------------------------


def test_legacy_report_generator_is_still_importable():
    """Smoke-test that the legacy ``ReportGenerator`` (ValidationResult-based)
    is still exported alongside the new ``ParityReportGenerator``."""
    from parity.reporter import ReportGenerator  # noqa: F401
