#!/usr/bin/env python3
"""
run_validation.py — Main entry point for the Validation Correctness Framework.

Usage examples::

    # Run everything
    python run_validation.py

    # Select methods (legacy flag — still works)
    python run_validation.py --methods linear,logistic

    # Restrict to a single Output-Level Algorithm by exact-match against
    # the Algorithm Coverage Matrix (Requirement 5.5).
    python run_validation.py --filter logistic

    # Emit the new ParityReportGenerator output (report.json + report.html)
    # under crates/stats-code/validation/reports/<run_id>/.
    python run_validation.py --emit-report

    # Select datasets (glob supported)
    python run_validation.py --datasets "datasets/synthetic/*.csv"

    # Verbose output (populates ValidationResult.details)
    python run_validation.py --verbose

    # Custom output directory for the legacy report.json + report.md
    python run_validation.py --out reports/

    # Custom tolerance config
    python run_validation.py --tolerance-config custom_tolerance.yaml

Exit codes (Requirement 5.7 — mirrors Rust ParityOutcome::exit_code):
    0  — all pass and no skipped due to reference_software_unavailable
    2  — at least one fail row (or error row) is present
    3  — ``--filter`` did not match any algorithm in the coverage matrix
    4  — tolerance config missing, unreadable, or malformed
    5  — matrix consistency failure (delegated to task 11.6 — not yet emitted here)
"""

from __future__ import annotations

import argparse
import datetime as _dt
import glob
import os
import sys
import tomllib
import uuid
from pathlib import Path
from typing import Any, Callable, Iterable, Optional

# Ensure the validation package is importable when run as a script
_HERE = Path(__file__).resolve().parent
if str(_HERE) not in sys.path:
    sys.path.insert(0, str(_HERE))

from parity.adapters import ADAPTERS_FOR
from parity.reporter import ParityReportGenerator, ReportGenerator
from parity.result import (
    ParityReportHeader,
    Status,
    ToleranceConfig,
    ValidationResult,
    collect_metadata,
    resolve_stats_code_version,
)

# ---------------------------------------------------------------------------
# Exit codes (mirrors Rust crate::parity::run_local::ParityOutcome::exit_code)
# ---------------------------------------------------------------------------

EXIT_ALL_PASS = 0
EXIT_FAIL_ROWS = 2
EXIT_UNKNOWN_FILTER = 3
EXIT_TOLERANCE_CONFIG = 4
EXIT_MATRIX_INCONSISTENT = 5

# Marker that adapters embed in ValidationResult.message when a reference
# implementation is not available on the current host (Requirement 4.10).
_REF_UNAVAILABLE_MARKER = "reference_software_unavailable"


# ---------------------------------------------------------------------------
# Parity module imports
# ---------------------------------------------------------------------------
# Each module exposes: METHOD (str), METRICS (list[str]),
#   collect(dataset_path, tol_config, adapters) -> list[ValidationResult]
#
# Modules are imported lazily inside _dispatch so that import errors for
# optional dependencies don't crash the whole runner.

def _import_linear():
    from parity import linear
    return linear

def _import_logistic():
    from parity import logistic
    return logistic

def _import_cox():
    from parity import cox
    return cox

def _import_survival():
    from parity import survival
    return survival

def _import_rate():
    from parity import rate
    return rate

def _import_power():
    from parity import power
    return power

def _import_math_core():
    from parity import math_core
    return math_core

def _import_tableone():
    from parity import tableone
    return tableone

def _import_diagnostic_roc():
    from parity import diagnostic_roc
    return diagnostic_roc


# ---------------------------------------------------------------------------
# Method dispatch table
# ---------------------------------------------------------------------------

# Maps method name → lazy importer function.
# Populated incrementally: M1 covers linear/logistic/cox; M4 adds the rest.
METHOD_IMPORTERS: dict[str, Callable] = {
    "linear":         _import_linear,
    "logistic":       _import_logistic,
    "cox":            _import_cox,
    "survival":       _import_survival,
    "rate":           _import_rate,
    "power":          _import_power,
    "math_core":      _import_math_core,
    "tableone":       _import_tableone,
    "diagnostic_roc": _import_diagnostic_roc,
}

# Methods that do NOT consume a CSV dataset (they use internal test-point grids).
DATASET_FREE_METHODS: frozenset[str] = frozenset({"math_core", "power"})

# Default dataset directories per method (relative to validation/).
DEFAULT_DATASET_DIRS: dict[str, list[str]] = {
    "linear":         ["datasets/synthetic", "datasets/public"],
    "logistic":       ["datasets/synthetic", "datasets/public"],
    "cox":            ["datasets/synthetic", "datasets/public"],
    "survival":       ["datasets/synthetic", "datasets/public"],
    "rate":           ["datasets/synthetic"],
    "tableone":       ["datasets/synthetic", "datasets/public"],
    "diagnostic_roc": ["datasets/synthetic"],
    # dataset-free methods use a sentinel value
    "math_core":      [],
    "power":          [],
}


# ---------------------------------------------------------------------------
# Coverage matrix loader (task 11.5)
# ---------------------------------------------------------------------------

class CoverageMatrixError(RuntimeError):
    """Raised when ``coverage_matrix.toml`` is missing or malformed."""


def load_coverage_matrix(validation_dir: Path | None = None) -> dict[str, Any]:
    """Load the build-mirrored ``coverage_matrix.toml`` from *validation_dir*.

    Returns the parsed TOML as a plain dict, with the embedded ``[[algorithm]]``
    list of tables surfaced under the ``"algorithm"`` key (per ``tomllib``
    semantics).

    Raises :class:`CoverageMatrixError` when the file is missing or fails to
    parse. Callers are expected to translate that into the documented
    exit code 5 (matrix inconsistency) at the CLI boundary.
    """
    if validation_dir is None:
        validation_dir = _HERE
    matrix_path = validation_dir / "coverage_matrix.toml"
    if not matrix_path.is_file():
        raise CoverageMatrixError(
            f"coverage_matrix.toml not found at {matrix_path} — was the "
            "stats-code build.rs mirror step run? (Requirement 6.1)"
        )
    try:
        with matrix_path.open("rb") as fh:
            return tomllib.load(fh)
    except tomllib.TOMLDecodeError as exc:
        raise CoverageMatrixError(
            f"coverage_matrix.toml is malformed at {matrix_path}: {exc}"
        ) from exc


def _matrix_algorithm_ids(matrix: dict[str, Any]) -> list[str]:
    """Return the list of ``algorithm.id`` values in the parsed matrix.

    The TOML uses ``[[algorithm]]`` array-of-tables, which ``tomllib`` flattens
    onto the ``"algorithm"`` key as a list of dicts.
    """
    return [str(entry["id"]) for entry in matrix.get("algorithm", [])]


# ---------------------------------------------------------------------------
# Dataset enumeration
# ---------------------------------------------------------------------------

def _enumerate_datasets(
    method: str,
    dataset_filter: list[str] | None,
    validation_dir: Path,
) -> list[Path]:
    """Return the list of dataset paths for *method*, respecting *dataset_filter*."""
    if method in DATASET_FREE_METHODS:
        # Sentinel: a single None-like path; the module's collect() ignores it.
        return [Path("__builtin__")]

    if dataset_filter:
        paths: list[Path] = []
        for pattern in dataset_filter:
            # Support both absolute and relative-to-cwd globs
            matched = glob.glob(pattern, recursive=True)
            if not matched:
                # Try relative to validation dir
                matched = glob.glob(str(validation_dir / pattern), recursive=True)
            paths.extend(Path(p) for p in matched)
        return sorted(set(paths))

    # Default: all CSVs in the method's default directories
    dirs = DEFAULT_DATASET_DIRS.get(method, ["datasets/synthetic"])
    paths = []
    for d in dirs:
        dir_path = validation_dir / d
        if dir_path.exists():
            paths.extend(sorted(dir_path.glob("*.csv")))
    return paths


# ---------------------------------------------------------------------------
# Exit-code aggregation (task 11.5 — mirrors Rust ParityOutcome::exit_code)
# ---------------------------------------------------------------------------

def _is_unavailable_skip(r: ValidationResult) -> bool:
    """Whether *r* is a ``SKIP`` row caused by a missing reference engine.

    Adapters annotate the ``message`` field with the
    ``reference_software_unavailable`` marker when the underlying engine
    cannot be located (Requirement 4.10). Per the new exit-code map a single
    such row promotes the run to exit code 2.
    """
    return (
        r.status == Status.SKIP
        and isinstance(r.message, str)
        and _REF_UNAVAILABLE_MARKER in r.message
    )


def compute_exit_code(
    results: Iterable[ValidationResult],
    *,
    filter_unknown: bool = False,
    tolerance_error: bool = False,
    matrix_inconsistent: bool = False,
) -> int:
    """Map a validation outcome onto the documented exit-code surface.

    Precedence (highest first) — matches the gate order in the Rust
    ``parity::run_local::classify_outcome`` so both implementations always
    agree on the "cause class" they surface:

    1. ``tolerance_error``     ⇒ exit 4
    2. ``filter_unknown``      ⇒ exit 3
    3. ``matrix_inconsistent`` ⇒ exit 5
    4. any ``FAIL``/``ERROR``  ⇒ exit 2
    5. any SKIP whose message  ⇒ exit 2
       contains ``reference_software_unavailable``
    6. otherwise               ⇒ exit 0
    """
    if tolerance_error:
        return EXIT_TOLERANCE_CONFIG
    if filter_unknown:
        return EXIT_UNKNOWN_FILTER
    if matrix_inconsistent:
        return EXIT_MATRIX_INCONSISTENT

    materialised = list(results)
    has_fail_or_error = any(
        r.status in (Status.FAIL, Status.ERROR) for r in materialised
    )
    if has_fail_or_error:
        return EXIT_FAIL_ROWS
    if any(_is_unavailable_skip(r) for r in materialised):
        return EXIT_FAIL_ROWS
    return EXIT_ALL_PASS


# ---------------------------------------------------------------------------
# Run-id generation for --emit-report
# ---------------------------------------------------------------------------

def _default_run_id() -> str:
    """Generate a per-process run identifier for ``reports/<run_id>/``.

    Format: ``YYYYMMDD-HHMMSS-<8 hex>``. Tests patch this helper to fix the
    run id and assert the directory layout.
    """
    now = _dt.datetime.now(_dt.timezone.utc)
    return f"{now.strftime('%Y%m%d-%H%M%S')}-{uuid.uuid4().hex[:8]}"


def _build_parity_header(
    matrix: dict[str, Any],
    metadata: Any,
) -> ParityReportHeader:
    """Build a ``ParityReportHeader`` from the parsed matrix and run metadata.

    This is the minimum viable header — wave-1 leaves ``tolerance_diff`` empty
    and records only the reference engine versions exposed by ``RunMetadata``.
    Task 12.x is responsible for plumbing PR-modified tolerance entries here.
    """
    # ``platform.platform()`` returns e.g. "Windows-10-10.0.19045-SP0".
    # Carve a coarse OS family + version, capped at the 32-character ceiling
    # spelled out in Requirement 9.2.
    raw_os = str(metadata.os)
    family = "Linux"
    if raw_os.lower().startswith("windows"):
        family = "Windows"
    elif raw_os.lower().startswith("darwin") or "macos" in raw_os.lower():
        family = "macOS"
    version = raw_os[:32]

    return ParityReportHeader(
        commit_sha=str(metadata.stats_code_commit),
        run_started_at_utc=str(metadata.generated_at),
        host_os_family=family,
        host_os_version=version,
        reference_software_versions=dict(metadata.reference_engine_versions),
        coverage_matrix=matrix,
        tolerance_diff=[],
    )


# ---------------------------------------------------------------------------
# Core orchestration
# ---------------------------------------------------------------------------

def run(
    methods: list[str] | None = None,
    datasets: list[str] | None = None,
    verbose: bool = False,
    out: Path | None = None,
    tolerance_config_path: Path | None = None,
    validation_dir: Path | None = None,
) -> list[ValidationResult]:
    """
    Run the validation suite and return all results.

    Parameters
    ----------
    methods:              Method names to run; None means all registered methods.
    datasets:             Dataset glob patterns; None means default per-method datasets.
    verbose:              If True, populate ValidationResult.details with debug info.
    out:                  Output directory for legacy reports; None skips.
    tolerance_config_path: Path to tolerance YAML; None uses the default.
    validation_dir:       Root of the validation/ directory; auto-detected if None.
    """
    if validation_dir is None:
        validation_dir = Path(__file__).resolve().parent

    # Load tolerance config
    if tolerance_config_path is None:
        tolerance_config_path = validation_dir / "tolerance_config.yaml"
    tol_config = ToleranceConfig.from_yaml(tolerance_config_path)

    # Resolve method list
    active_methods = methods if methods else list(METHOD_IMPORTERS.keys())

    all_results: list[ValidationResult] = []

    for method in active_methods:
        if method not in METHOD_IMPORTERS:
            print(f"[WARN] Unknown method '{method}', skipping.", file=sys.stderr)
            continue

        # Lazy-import the parity module
        try:
            module = METHOD_IMPORTERS[method]()
        except ImportError as exc:
            print(f"[WARN] Cannot import parity module for '{method}': {exc}", file=sys.stderr)
            continue

        # Get adapters for this method
        adapters = ADAPTERS_FOR.get(method, [])

        # Enumerate datasets
        dataset_paths = _enumerate_datasets(method, datasets, validation_dir)
        if not dataset_paths:
            print(f"[INFO] No datasets found for method '{method}', skipping.", file=sys.stderr)
            continue

        for dataset_path in dataset_paths:
            try:
                results = module.collect(
                    dataset_path=dataset_path,
                    tol_config=tol_config,
                    adapters=adapters,
                )
            except Exception as exc:
                # Catch-all: one (method, dataset) failure must not abort the run
                all_results.append(
                    ValidationResult(
                        method=method,
                        dataset=str(dataset_path),
                        reference_engine="unknown",
                        metric="__collect__",
                        tolerance=0.0,
                        status=Status.ERROR,
                        message=f"collect() raised: {exc}",
                    )
                )
                continue

            # Strip details when not in verbose mode
            if not verbose:
                for r in results:
                    r.details = {}

            all_results.extend(results)

    # Write reports if requested (legacy path — report.json + report.md)
    if out is not None:
        metadata = collect_metadata()
        # Spec: parity-math-core-collect-crash — back-fill stats_code_version
        # (collect_metadata leaves it "unknown" by default). validation_dir is
        # .../crates/stats-code/validation; the workspace root is two up.
        metadata.stats_code_version = resolve_stats_code_version(
            validation_dir.parent.parent.parent
        )
        gen = ReportGenerator(all_results, metadata)
        gen.write(out)
        print(f"[INFO] Reports written to {out}", file=sys.stderr)

    return all_results


# ---------------------------------------------------------------------------
# CLI entry point
# ---------------------------------------------------------------------------

def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Stats Code Validation Correctness Framework",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=__doc__,
    )
    parser.add_argument(
        "--methods",
        metavar="METHOD[,METHOD...]",
        help="Comma-separated list of methods to validate (default: all).",
    )
    parser.add_argument(
        "--datasets",
        metavar="GLOB[,GLOB...]",
        help="Comma-separated glob patterns for dataset CSV files.",
    )
    parser.add_argument(
        "--filter",
        metavar="ALGORITHM_ID",
        default=None,
        help=(
            "Restrict the run to a single Output-Level Algorithm by exact-match "
            "id against the Algorithm Coverage Matrix (Requirement 5.5). "
            "An unmatched value triggers exit code 3."
        ),
    )
    parser.add_argument(
        "--emit-report",
        action="store_true",
        help=(
            "Emit the new ParityReportGenerator output (report.json + report.html) "
            "under crates/stats-code/validation/reports/<run_id>/. "
            "The legacy report.md path remains gated behind --out."
        ),
    )
    parser.add_argument(
        "--verbose",
        action="store_true",
        help="Populate ValidationResult.details with intermediate values.",
    )
    parser.add_argument(
        "--out",
        metavar="DIR",
        default="reports",
        help="Output directory for legacy report.json and report.md (default: reports/).",
    )
    parser.add_argument(
        "--tolerance-config",
        metavar="YAML",
        default=None,
        help="Path to a custom tolerance_config.yaml.",
    )

    args = parser.parse_args(argv)

    methods: Optional[list[str]] = (
        [m.strip() for m in args.methods.split(",")] if args.methods else None
    )
    dataset_patterns = (
        [d.strip() for d in args.datasets.split(",")] if args.datasets else None
    )
    out_dir = Path(args.out)
    tol_path = Path(args.tolerance_config) if args.tolerance_config else None

    validation_dir = _HERE

    # ── Gate 1: tolerance config (Requirement 12.6 — exit 4) ──────────────
    # Load explicitly here (before run()) so we can map IO / parse failures
    # onto exit code 4 *before* doing any heavy lifting.
    effective_tol_path = tol_path or (validation_dir / "tolerance_config.yaml")
    try:
        ToleranceConfig.from_yaml(effective_tol_path)
    except (FileNotFoundError, OSError) as exc:
        print(
            f"[ERROR] tolerance config not found at {effective_tol_path}: {exc}",
            file=sys.stderr,
        )
        return EXIT_TOLERANCE_CONFIG
    except Exception as exc:  # YAML parse error, schema violation, etc.
        print(
            f"[ERROR] tolerance config malformed at {effective_tol_path}: {exc}",
            file=sys.stderr,
        )
        return EXIT_TOLERANCE_CONFIG

    # ── Gate 2: --filter validation (Requirement 5.7 — exit 3) ────────────
    matrix: Optional[dict[str, Any]] = None
    if args.filter is not None or args.emit_report:
        try:
            matrix = load_coverage_matrix(validation_dir)
        except CoverageMatrixError as exc:
            print(f"[ERROR] {exc}", file=sys.stderr)
            return EXIT_MATRIX_INCONSISTENT

    if args.filter is not None:
        assert matrix is not None  # populated above
        algorithm_ids = _matrix_algorithm_ids(matrix)
        if args.filter not in algorithm_ids:
            print(
                f"[ERROR] --filter {args.filter!r} did not match any algorithm "
                "in the Algorithm Coverage Matrix; no Parity Validation Report "
                "produced (Requirement 5.7)",
                file=sys.stderr,
            )
            return EXIT_UNKNOWN_FILTER
        # Restrict execution to the filtered algorithm. If --methods was also
        # provided, intersect the two so they cannot disagree silently.
        if methods is None:
            methods = [args.filter]
        else:
            methods = [m for m in methods if m == args.filter]

    # ── Run the suite ─────────────────────────────────────────────────────
    results = run(
        methods=methods,
        datasets=dataset_patterns,
        verbose=args.verbose,
        out=out_dir,
        tolerance_config_path=tol_path,
        validation_dir=validation_dir,
    )

    # ── New ParityReportGenerator output (task 11.4 / 11.5) ───────────────
    if args.emit_report:
        assert matrix is not None  # loaded above
        run_id = _default_run_id()
        report_dir = validation_dir / "reports" / run_id
        # Wave-1 emits an empty rows[] alongside the populated header; the
        # ParityRow-shaped inputs land in wave-2 once the adapter layer
        # produces them directly (tasks 11.7 / 11.8).
        metadata = collect_metadata()
        metadata.stats_code_version = resolve_stats_code_version(
            validation_dir.parent.parent.parent
        )
        header = _build_parity_header(matrix, metadata)
        gen = ParityReportGenerator(rows=[], header=header)
        gen.write(report_dir)
        print(f"[INFO] Parity report written to {report_dir}", file=sys.stderr)

    # ── Print summary to stdout ───────────────────────────────────────────
    total = len(results)
    pass_n = sum(1 for r in results if r.status == Status.PASS)
    fail_n = sum(1 for r in results if r.status == Status.FAIL)
    skip_n = sum(1 for r in results if r.status == Status.SKIP)
    error_n = sum(1 for r in results if r.status == Status.ERROR)
    print(
        f"Validation complete: {total} comparisons — "
        f"{pass_n} pass, {fail_n} fail, {skip_n} skip, {error_n} error"
    )

    return compute_exit_code(results)


if __name__ == "__main__":
    sys.exit(main())
