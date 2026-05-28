"""Unit tests for the ``run_validation.py`` CLI entry point (task 11.5).

Validates Requirements 3.1, 4.5, 5.1, 5.5, 5.7, 12.6.

Covers:
  - Exit code 0 for an all-pass run with no ``reference_software_unavailable``
    SKIPs.
  - Exit code 2 when at least one row is FAIL or ERROR (or a SKIP carrying
    the ``reference_software_unavailable`` marker — Requirement 4.10).
  - Exit code 3 when ``--filter`` does not match any algorithm in the
    Algorithm Coverage Matrix (Requirement 5.5).
  - Exit code 4 when ``tolerance_config.yaml`` is missing or malformed
    (Requirement 12.6).
  - ``--emit-report`` writes ``report.json`` + ``report.html`` under
    ``reports/<run_id>/`` (Requirement 3.1).
"""

from __future__ import annotations

import json
import sys
from pathlib import Path
from unittest.mock import patch

import pytest

# Make ``run_validation`` and the ``parity`` package importable when this
# test file is executed directly via ``python -m pytest`` from any cwd.
_VALIDATION_DIR = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(_VALIDATION_DIR))

import run_validation  # noqa: E402
from parity.result import Status, ValidationResult  # noqa: E402


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def _make_result(
    status: Status,
    *,
    method: str = "linear",
    message: str = "",
) -> ValidationResult:
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
        message=message,
    )


# ---------------------------------------------------------------------------
# compute_exit_code helper (pure function — Requirement 5.7)
# ---------------------------------------------------------------------------

class TestComputeExitCode:
    """Tests for the pure ``compute_exit_code`` helper.

    The CLI uses this helper to decide its exit code; testing it directly
    keeps the contract observable without spinning up the full ``main()``.
    """

    def test_all_pass_no_skip_returns_zero(self) -> None:
        results = [_make_result(Status.PASS) for _ in range(5)]
        assert run_validation.compute_exit_code(results) == 0

    def test_pass_plus_neutral_skip_returns_zero(self) -> None:
        # SKIP rows that do *not* carry the reference_software_unavailable
        # marker are neutral (e.g. "no test cases for this method").
        results = [
            _make_result(Status.PASS),
            _make_result(Status.SKIP, message="no_test_data"),
        ]
        assert run_validation.compute_exit_code(results) == 0

    def test_fail_row_returns_two(self) -> None:
        results = [_make_result(Status.PASS), _make_result(Status.FAIL)]
        assert run_validation.compute_exit_code(results) == 2

    def test_error_row_returns_two(self) -> None:
        results = [_make_result(Status.ERROR, message="adapter blew up")]
        assert run_validation.compute_exit_code(results) == 2

    def test_unavailable_skip_returns_two(self) -> None:
        # Per Requirement 4.10 a SKIP triggered by an unavailable reference
        # engine must promote the run to a non-zero exit code.
        results = [
            _make_result(Status.PASS),
            _make_result(
                Status.SKIP, message="reference_software_unavailable: Rscript"
            ),
        ]
        assert run_validation.compute_exit_code(results) == 2

    def test_filter_unknown_takes_precedence(self) -> None:
        # Even if results contain failures, the filter-unknown gate fires
        # earlier in the cause-class hierarchy (Requirement 5.7).
        results = [_make_result(Status.FAIL)]
        assert (
            run_validation.compute_exit_code(results, filter_unknown=True) == 3
        )

    def test_tolerance_error_takes_precedence_over_filter(self) -> None:
        results: list[ValidationResult] = []
        assert (
            run_validation.compute_exit_code(
                results, filter_unknown=True, tolerance_error=True
            )
            == 4
        )

    def test_matrix_inconsistent_returns_five(self) -> None:
        results: list[ValidationResult] = []
        assert (
            run_validation.compute_exit_code(results, matrix_inconsistent=True)
            == 5
        )


# ---------------------------------------------------------------------------
# Exit code 0 — all pass + no unavailable SKIP
# ---------------------------------------------------------------------------

def test_main_exits_zero_for_all_pass(tmp_path: Path) -> None:
    """Requirement 5.7 → exit code 0 when every row is PASS."""
    results = [_make_result(Status.PASS) for _ in range(3)]
    with patch.object(run_validation, "run", return_value=results):
        code = run_validation.main(["--out", str(tmp_path)])
    assert code == 0


# ---------------------------------------------------------------------------
# Exit code 2 — fail / error / unavailable-skip rows present
# ---------------------------------------------------------------------------

def test_main_exits_two_for_fail_row(tmp_path: Path) -> None:
    """Requirement 5.7 → at least one FAIL row promotes the run to exit 2."""
    results = [_make_result(Status.PASS), _make_result(Status.FAIL)]
    with patch.object(run_validation, "run", return_value=results):
        code = run_validation.main(["--out", str(tmp_path)])
    assert code == 2


def test_main_exits_two_for_unavailable_skip(tmp_path: Path) -> None:
    """Requirement 4.10 → a SKIP with reference_software_unavailable
    promotes the run to exit 2."""
    results = [
        _make_result(Status.PASS),
        _make_result(
            Status.SKIP,
            message="reference_software_unavailable: SAS not installed",
        ),
    ]
    with patch.object(run_validation, "run", return_value=results):
        code = run_validation.main(["--out", str(tmp_path)])
    assert code == 2


# ---------------------------------------------------------------------------
# Exit code 3 — --filter does not match any algorithm in the matrix
# ---------------------------------------------------------------------------

def test_main_exits_three_for_unknown_filter(
    tmp_path: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    """Requirement 5.5 / 5.7 → unmatched ``--filter`` value exits with 3
    and surfaces the offending value to stderr."""
    # ``run`` must not be invoked when the filter gate trips; patch it to
    # raise so a leak would be loud.
    with patch.object(run_validation, "run", side_effect=AssertionError("run() should not be called")):
        code = run_validation.main([
            "--filter", "definitely_not_an_algorithm",
            "--out", str(tmp_path),
        ])
    assert code == 3

    err = capsys.readouterr().err
    assert "definitely_not_an_algorithm" in err


def test_main_accepts_known_filter(tmp_path: Path) -> None:
    """A ``--filter`` value that matches an algorithm in the matrix must
    pass the gate and continue to the suite. ``logistic`` is in the
    embedded coverage matrix."""
    results = [_make_result(Status.PASS, method="logistic")]
    with patch.object(run_validation, "run", return_value=results) as mock_run:
        code = run_validation.main([
            "--filter", "logistic",
            "--out", str(tmp_path),
        ])
    assert code == 0
    # The filter is intersected onto the methods list so the suite only
    # runs the matching algorithm.
    assert mock_run.call_args.kwargs["methods"] == ["logistic"]


# ---------------------------------------------------------------------------
# Exit code 4 — tolerance config missing or malformed
# ---------------------------------------------------------------------------

def test_main_exits_four_for_missing_tolerance_config(
    tmp_path: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    """Requirement 12.6 → missing tolerance config exits with 4 and does
    not invoke the suite."""
    missing_yaml = tmp_path / "no_such_file.yaml"
    with patch.object(
        run_validation, "run", side_effect=AssertionError("run() should not be called")
    ):
        code = run_validation.main([
            "--tolerance-config", str(missing_yaml),
            "--out", str(tmp_path),
        ])
    assert code == 4
    err = capsys.readouterr().err
    assert "tolerance config" in err.lower()


def test_main_exits_four_for_malformed_tolerance_config(tmp_path: Path) -> None:
    """Requirement 12.6 → malformed tolerance YAML exits with 4."""
    bad_yaml = tmp_path / "bad.yaml"
    # Not a YAML mapping — ``ToleranceConfig.from_yaml`` raises ValueError.
    bad_yaml.write_text("- just\n- a\n- list\n", encoding="utf-8")
    with patch.object(
        run_validation, "run", side_effect=AssertionError("run() should not be called")
    ):
        code = run_validation.main([
            "--tolerance-config", str(bad_yaml),
            "--out", str(tmp_path),
        ])
    assert code == 4


# ---------------------------------------------------------------------------
# --emit-report writes report.json + report.html under reports/<run_id>/
# ---------------------------------------------------------------------------

def test_emit_report_writes_to_run_id_subdir(tmp_path: Path) -> None:
    """Requirement 3.1 → ``--emit-report`` materialises ``report.json``
    and ``report.html`` under ``reports/<run_id>/``."""
    fixed_run_id = "20991231-235959-deadbeef"

    # Patch the run id helper so the test can assert on the exact path,
    # and patch ``run`` to bypass the heavy adapter machinery (we only
    # care about the report writer here).
    results = [_make_result(Status.PASS)]
    with patch.object(run_validation, "_default_run_id", return_value=fixed_run_id), \
         patch.object(run_validation, "run", return_value=results):
        # Point ``--out`` at a tmp dir so the legacy reports also land in
        # an isolated location instead of the repo's reports/ folder.
        code = run_validation.main([
            "--emit-report",
            "--out", str(tmp_path / "legacy_out"),
        ])

    assert code == 0

    # The Parity report lives under the repo's validation/reports tree —
    # see ``run_validation._HERE``. Compute the expected path the same way
    # the production code does.
    report_dir = (
        Path(run_validation.__file__).resolve().parent
        / "reports"
        / fixed_run_id
    )
    try:
        json_path = report_dir / "report.json"
        html_path = report_dir / "report.html"
        assert json_path.exists(), f"missing: {json_path}"
        assert html_path.exists(), f"missing: {html_path}"

        # Sanity-check the JSON parses and exposes the expected schema.
        doc = json.loads(json_path.read_text(encoding="utf-8"))
        assert "schema_version" in doc
        assert "header" in doc
        assert "rows" in doc

        # Header must echo the embedded coverage matrix (Requirement 3.7).
        cov = doc["header"]["coverage_matrix"]
        assert "algorithm" in cov
        algorithm_ids = {entry["id"] for entry in cov["algorithm"]}
        assert "logistic" in algorithm_ids
    finally:
        # Clean up the run-specific directory so repeated runs do not
        # accumulate stale fixtures under crates/stats-code/validation/reports/.
        for f in report_dir.glob("*"):
            f.unlink()
        if report_dir.exists():
            report_dir.rmdir()


def test_emit_report_skipped_when_flag_absent(tmp_path: Path) -> None:
    """Without ``--emit-report`` the new ParityReportGenerator output must
    not be written — only the legacy ``--out`` reports land on disk."""
    fixed_run_id = "20991231-235959-cafebabe"

    results = [_make_result(Status.PASS)]
    with patch.object(run_validation, "_default_run_id", return_value=fixed_run_id), \
         patch.object(run_validation, "run", return_value=results):
        code = run_validation.main(["--out", str(tmp_path / "legacy_out")])

    assert code == 0

    report_dir = (
        Path(run_validation.__file__).resolve().parent
        / "reports"
        / fixed_run_id
    )
    assert not report_dir.exists(), (
        f"emit-report flag was absent; {report_dir} should not be created"
    )


# ---------------------------------------------------------------------------
# Backwards compatibility — existing CLI flags still work
# ---------------------------------------------------------------------------

def test_existing_methods_flag_still_accepted(tmp_path: Path) -> None:
    """Task 11.5 must not break ``--methods``, ``--datasets``, etc."""
    results: list[ValidationResult] = []
    with patch.object(run_validation, "run", return_value=results) as mock_run:
        code = run_validation.main([
            "--methods", "linear,logistic",
            "--datasets", "datasets/synthetic/*.csv",
            "--out", str(tmp_path),
        ])
    assert code == 0
    kwargs = mock_run.call_args.kwargs
    assert kwargs["methods"] == ["linear", "logistic"]
    assert kwargs["datasets"] == ["datasets/synthetic/*.csv"]


# ---------------------------------------------------------------------------
# Coverage matrix loader (Requirement 6.1)
# ---------------------------------------------------------------------------

def test_load_coverage_matrix_returns_algorithm_list() -> None:
    """The loader must return the parsed TOML with the array-of-tables
    surfaced under ``algorithm``."""
    matrix = run_validation.load_coverage_matrix()
    assert isinstance(matrix, dict)
    assert "algorithm" in matrix
    ids = {entry["id"] for entry in matrix["algorithm"]}
    # Spot-check a handful of algorithms that must be present in the matrix.
    assert {"logistic", "cox", "tableone"}.issubset(ids)


def test_load_coverage_matrix_raises_on_missing_file(tmp_path: Path) -> None:
    """A directory without ``coverage_matrix.toml`` must raise the
    structured error so the CLI can surface exit code 5."""
    with pytest.raises(run_validation.CoverageMatrixError):
        run_validation.load_coverage_matrix(tmp_path)
