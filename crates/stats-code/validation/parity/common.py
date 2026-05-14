"""
parity/common.py — Shared utilities for the Validation Correctness Framework.

Provides:
  - StatsCodeInvocationError : raised when the CLI call fails
  - run_stats_code            : subprocess wrapper for the Stats Code CLI
  - extract_metrics           : pull metric values from CLI JSON output
  - compare_scalar            : compare one scalar metric with tolerance
  - compare_vector            : compare a vector metric element-wise
"""

from __future__ import annotations

import json
import math
import subprocess
from pathlib import Path
from typing import Any, Sequence

from .result import Status, ToleranceConfig, ValidationResult


# ---------------------------------------------------------------------------
# Errors
# ---------------------------------------------------------------------------

class StatsCodeInvocationError(RuntimeError):
    """Raised when the Stats Code CLI exits non-zero or returns invalid JSON."""


# ---------------------------------------------------------------------------
# CLI invocation
# ---------------------------------------------------------------------------

# Resolved once at import time; callers can override for testing.
_CARGO_ARGS: list[str] = ["cargo", "run", "--locked", "-q", "-p", "stats-code", "--"]


def run_stats_code(args: list[str], cwd: Path | None = None) -> dict[str, Any]:
    """
    Invoke the Stats Code CLI with *args* and return the parsed JSON output.

    The CLI is expected to be called with ``--json`` somewhere in *args* so
    that it writes a JSON object to stdout.

    Parameters
    ----------
    args:
        Arguments appended after ``cargo run --locked -q -p stats-code --``.
        Example: ``["model", "linear", "--data", "path/to/data.csv", "--json"]``
    cwd:
        Working directory for the subprocess.  Defaults to the workspace root
        (two levels above this file: ``validation/`` → ``stats-code/`` → …).

    Returns
    -------
    dict
        Parsed JSON object from stdout.

    Raises
    ------
    StatsCodeInvocationError
        If the process exits non-zero, stdout is empty, or JSON parsing fails.
    """
    if cwd is None:
        # validation/parity/common.py → validation/ → stats-code/ → crates/ → workspace root
        cwd = Path(__file__).resolve().parent.parent.parent.parent.parent

    cmd = _CARGO_ARGS + args
    try:
        # Capture as bytes to avoid Windows locale-encoding errors on stderr
        result = subprocess.run(
            cmd,
            capture_output=True,
            text=False,
            cwd=str(cwd),
            timeout=120,
        )
    except subprocess.TimeoutExpired as exc:
        raise StatsCodeInvocationError(
            f"Stats Code CLI timed out after 120 s: {' '.join(cmd)}"
        ) from exc
    except FileNotFoundError as exc:
        raise StatsCodeInvocationError(
            f"'cargo' not found — is Rust installed? ({exc})"
        ) from exc

    # Decode with replacement for any invalid bytes
    stdout_text = result.stdout.decode("utf-8", errors="replace")
    stderr_text = result.stderr.decode("utf-8", errors="replace")

    if result.returncode != 0:
        stderr_snippet = stderr_text.strip()[:500]
        raise StatsCodeInvocationError(
            f"Stats Code CLI exited {result.returncode}.\n"
            f"Command: {' '.join(cmd)}\n"
            f"stderr: {stderr_snippet}"
        )

    stdout = stdout_text.strip()
    if not stdout:
        raise StatsCodeInvocationError(
            f"Stats Code CLI produced no stdout.\nCommand: {' '.join(cmd)}"
        )

    try:
        return json.loads(stdout)
    except json.JSONDecodeError as exc:
        snippet = stdout[:200]
        raise StatsCodeInvocationError(
            f"invalid JSON from Stats Code CLI: {exc}\nOutput snippet: {snippet!r}"
        ) from exc


# ---------------------------------------------------------------------------
# Metric extraction
# ---------------------------------------------------------------------------

def extract_metrics(sc_out: dict[str, Any], spec: dict[str, Any]) -> dict[str, float]:
    """
    Extract metric values from the Stats Code CLI JSON output.

    Parameters
    ----------
    sc_out:
        Parsed JSON dict returned by ``run_stats_code``.
    spec:
        A mapping that describes which keys to extract and how.
        Supported forms::

            # flat key
            {"beta[age]": "coefficients.age.estimate"}

            # nested path (dot-separated)
            {"r_squared": "model_fit.r_squared"}

            # direct top-level key
            {"log_likelihood": "log_likelihood"}

    Returns
    -------
    dict[str, float]
        metric name → float value.  Missing keys produce a ``KeyError``
        with a descriptive message so callers can wrap it into an ERROR result.
    """
    out: dict[str, float] = {}
    for metric_name, json_path in spec.items():
        parts = json_path.split(".")
        node: Any = sc_out
        for part in parts:
            if not isinstance(node, dict) or part not in node:
                raise KeyError(
                    f"missing metric '{metric_name}': key '{part}' not found "
                    f"in path '{json_path}'"
                )
            node = node[part]
        try:
            out[metric_name] = float(node)
        except (TypeError, ValueError) as exc:
            raise ValueError(
                f"metric '{metric_name}' at path '{json_path}' is not numeric: {node!r}"
            ) from exc
    return out


# ---------------------------------------------------------------------------
# Comparators
# ---------------------------------------------------------------------------

def compare_scalar(
    method: str,
    metric: str,
    dataset: str,
    ref_name: str,
    expected: float,
    actual: float,
    tol_config: ToleranceConfig,
) -> ValidationResult:
    """
    Compare one scalar metric value against a reference.

    Non-finite values (NaN, ±Inf) in either *expected* or *actual* produce an
    ERROR result rather than PASS/FAIL, because tolerance comparison is
    undefined for non-finite numbers.

    Parameters
    ----------
    method:    Stats Code method name, e.g. ``"linear"``.
    metric:    Metric name, e.g. ``"beta[age]"``.
    dataset:   Dataset relative path, e.g. ``"synthetic/small_n40.csv"``.
    ref_name:  Reference engine name, e.g. ``"statsmodels"``.
    expected:  Value from the reference engine.
    actual:    Value from Stats Code.
    tol_config: Tolerance configuration.

    Returns
    -------
    ValidationResult
    """
    tol = tol_config.lookup(method, metric)

    if not math.isfinite(expected):
        return ValidationResult(
            method=method,
            dataset=dataset,
            reference_engine=ref_name,
            metric=metric,
            tolerance=tol,
            status=Status.ERROR,
            expected=expected,
            actual=actual,
            message=f"non-finite reference value: {expected!r}",
        )

    if not math.isfinite(actual):
        return ValidationResult(
            method=method,
            dataset=dataset,
            reference_engine=ref_name,
            metric=metric,
            tolerance=tol,
            status=Status.ERROR,
            expected=expected,
            actual=actual,
            message=f"non-finite Stats Code value: {actual!r}",
        )

    diff = abs(expected - actual)
    if diff <= tol:
        status = Status.PASS
        message = ""
    else:
        status = Status.FAIL
        message = (
            f"difference {diff:.6e} exceeds tolerance {tol:.6e} "
            f"(expected={expected!r}, actual={actual!r})"
        )

    return ValidationResult(
        method=method,
        dataset=dataset,
        reference_engine=ref_name,
        metric=metric,
        tolerance=tol,
        status=status,
        expected=expected,
        actual=actual,
        difference=diff,
        message=message,
    )


def compare_vector(
    method: str,
    metric_prefix: str,
    dataset: str,
    ref_name: str,
    expected: Sequence[float],
    actual: Sequence[float],
    tol_config: ToleranceConfig,
) -> list[ValidationResult]:
    """
    Compare a vector metric element-wise (e.g. KM survival probabilities).

    Each element produces one ``ValidationResult`` with metric name
    ``"<metric_prefix>[<index>]"``.

    If the sequences have different lengths, a single ERROR result is returned.
    """
    if len(expected) != len(actual):
        tol = tol_config.lookup(method, metric_prefix)
        return [
            ValidationResult(
                method=method,
                dataset=dataset,
                reference_engine=ref_name,
                metric=metric_prefix,
                tolerance=tol,
                status=Status.ERROR,
                message=(
                    f"vector length mismatch: expected {len(expected)}, "
                    f"got {len(actual)}"
                ),
            )
        ]

    results: list[ValidationResult] = []
    for i, (exp_val, act_val) in enumerate(zip(expected, actual)):
        metric_name = f"{metric_prefix}[{i}]"
        results.append(
            compare_scalar(method, metric_name, dataset, ref_name, exp_val, act_val, tol_config)
        )
    return results
