#!/usr/bin/env python3
"""
run_validation.py — Main entry point for the Validation Correctness Framework.

Usage examples::

    # Run everything
    python run_validation.py

    # Select methods
    python run_validation.py --methods linear,logistic

    # Select datasets (glob supported)
    python run_validation.py --datasets "datasets/synthetic/*.csv"

    # Verbose output (populates ValidationResult.details)
    python run_validation.py --verbose

    # Custom output directory
    python run_validation.py --out reports/

    # Custom tolerance config
    python run_validation.py --tolerance-config custom_tolerance.yaml

Exit codes:
    0  — all comparisons PASS or SKIP
    1  — at least one FAIL or ERROR
"""

from __future__ import annotations

import argparse
import glob
import sys
from pathlib import Path
from typing import Callable

# Ensure the validation package is importable when run as a script
_HERE = Path(__file__).resolve().parent
if str(_HERE) not in sys.path:
    sys.path.insert(0, str(_HERE))

from parity.adapters import ADAPTERS_FOR
from parity.reporter import ReportGenerator
from parity.result import Status, ToleranceConfig, ValidationResult, collect_metadata

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
    out:                  Output directory for reports; None skips report writing.
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

    # Write reports if requested
    if out is not None:
        metadata = collect_metadata()
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
        "--verbose",
        action="store_true",
        help="Populate ValidationResult.details with intermediate values.",
    )
    parser.add_argument(
        "--out",
        metavar="DIR",
        default="reports",
        help="Output directory for report.json and report.md (default: reports/).",
    )
    parser.add_argument(
        "--tolerance-config",
        metavar="YAML",
        default=None,
        help="Path to a custom tolerance_config.yaml.",
    )

    args = parser.parse_args(argv)

    methods = [m.strip() for m in args.methods.split(",")] if args.methods else None
    dataset_patterns = (
        [d.strip() for d in args.datasets.split(",")] if args.datasets else None
    )
    out_dir = Path(args.out)
    tol_path = Path(args.tolerance_config) if args.tolerance_config else None

    results = run(
        methods=methods,
        datasets=dataset_patterns,
        verbose=args.verbose,
        out=out_dir,
        tolerance_config_path=tol_path,
    )

    # Exit code: 1 if any FAIL or ERROR, else 0
    has_failure = any(r.status in (Status.FAIL, Status.ERROR) for r in results)

    # Print summary to stdout
    total = len(results)
    pass_n = sum(1 for r in results if r.status == Status.PASS)
    fail_n = sum(1 for r in results if r.status == Status.FAIL)
    skip_n = sum(1 for r in results if r.status == Status.SKIP)
    error_n = sum(1 for r in results if r.status == Status.ERROR)
    print(
        f"Validation complete: {total} comparisons — "
        f"{pass_n} pass, {fail_n} fail, {skip_n} skip, {error_n} error"
    )

    return 1 if has_failure else 0


if __name__ == "__main__":
    sys.exit(main())
