"""
parity/result.py — Core data structures for the Validation Correctness Framework.

Defines:
  - Status          : pass / fail / skip / error enum
  - ValidationResult: one comparison outcome
  - ToleranceConfig : per-metric tolerance lookup with YAML loading
  - RunMetadata     : provenance information for a validation run
"""

from __future__ import annotations

import platform
import subprocess
import sys
from dataclasses import dataclass, field
from enum import Enum
from pathlib import Path
from typing import Any, Optional


# ---------------------------------------------------------------------------
# Status
# ---------------------------------------------------------------------------

class Status(str, Enum):
    PASS = "pass"
    FAIL = "fail"
    SKIP = "skip"
    ERROR = "error"  # non-numeric error: CLI crash, missing dependency, etc.


# ---------------------------------------------------------------------------
# ValidationResult
# ---------------------------------------------------------------------------

@dataclass
class ValidationResult:
    """One scalar (or vector-element) comparison between Stats Code and a reference."""

    method: str            # "linear" | "logistic" | "cox" | ...
    dataset: str           # relative path, e.g. "synthetic/small_n40.csv"
    reference_engine: str  # "statsmodels" | "lifelines" | "Rscript/survival" | "known_value"
    metric: str            # "beta[age]" | "r_squared" | "log_likelihood" | ...
    tolerance: float       # tolerance used for this comparison
    status: Status

    expected: Optional[float] = None
    actual: Optional[float] = None
    difference: Optional[float] = None
    message: str = ""
    # verbose mode populates this with intermediate values; cleared when verbose=False
    details: dict[str, Any] = field(default_factory=dict)


# ---------------------------------------------------------------------------
# ToleranceConfig
# ---------------------------------------------------------------------------

@dataclass(frozen=True)
class ToleranceConfig:
    """
    Tolerance lookup table loaded from tolerance_config.yaml.

    Lookup priority:
      1. "method.metric"  key  (most specific)
      2. "method"         key  (method-level fallback)
      3. default          value (global fallback, default 1e-6)
    """

    # key: "method.metric" or "method", value: tolerance
    per_metric: dict[str, float]
    default: float = 1e-6

    def lookup(self, method: str, metric: str) -> float:
        """Return the tolerance for (method, metric), falling back as described above.

        Lookup priority:
          1. "method.metric"       (exact, e.g. "logistic.wald[age]")
          2. "method.base_metric"  (strip [index], e.g. "logistic.wald")
          3. "method"              (method-level fallback)
          4. default
        """
        import re

        specific_key = f"{method}.{metric}"
        if specific_key in self.per_metric:
            return self.per_metric[specific_key]

        # Strip array index: "wald[age]" → "wald", "beta[0]" → "beta"
        base_metric = re.sub(r"\[.*\]$", "", metric)
        base_key = f"{method}.{base_metric}"
        if base_key in self.per_metric:
            return self.per_metric[base_key]

        if method in self.per_metric:
            return self.per_metric[method]
        return self.default

    @classmethod
    def from_yaml(cls, path: Path) -> "ToleranceConfig":
        """Load tolerance configuration from a YAML file.

        Expected YAML structure::

            default: 1e-6
            per_metric:
              linear.beta: 1.0e-8
              logistic.beta: 1.0e-5
              ...
        """
        import yaml  # deferred import — only needed when loading config

        with open(path, "r", encoding="utf-8") as fh:
            raw = yaml.safe_load(fh)

        if not isinstance(raw, dict):
            raise ValueError(f"tolerance_config.yaml must be a YAML mapping, got {type(raw)}")

        default = float(raw.get("default", 1e-6))
        per_metric_raw = raw.get("per_metric", {}) or {}
        per_metric = {str(k): float(v) for k, v in per_metric_raw.items()}

        return cls(per_metric=per_metric, default=default)


# ---------------------------------------------------------------------------
# RunMetadata
# ---------------------------------------------------------------------------

@dataclass
class RunMetadata:
    """Provenance information captured at the start of a validation run."""

    generated_at: str                          # ISO-8601 UTC timestamp
    stats_code_commit: str                     # git rev-parse HEAD (or "unknown")
    stats_code_version: str                    # from Cargo.toml or binary --version
    python_version: str                        # sys.version_info string
    rscript_version: str                       # "unavailable" if Rscript not found
    os: str                                    # platform.platform()
    reference_engine_versions: dict[str, str]  # library name → version string


def _git_commit() -> str:
    """Return the current HEAD commit hash, or 'unknown' if git is unavailable."""
    try:
        result = subprocess.run(
            ["git", "rev-parse", "HEAD"],
            capture_output=True,
            text=True,
            timeout=5,
        )
        if result.returncode == 0:
            return result.stdout.strip()
    except Exception:
        pass
    return "unknown"


def _rscript_version() -> str:
    """Return the Rscript version string, or 'unavailable'."""
    import shutil

    if shutil.which("Rscript") is None:
        return "unavailable"
    try:
        result = subprocess.run(
            ["Rscript", "--version"],
            capture_output=True,
            text=True,
            timeout=10,
        )
        # Rscript prints version to stderr on some platforms
        output = (result.stdout + result.stderr).strip()
        return output if output else "unavailable"
    except Exception:
        return "unavailable"


def _lib_version(name: str) -> str:
    """Return the installed version of a Python package, or 'unknown'."""
    try:
        import importlib.metadata
        return importlib.metadata.version(name)
    except Exception:
        return "unknown"


def _version_from_cargo_toml(workspace_root: Path) -> Optional[str]:
    """Read the Stats Code version from ``crates/stats-code/Cargo.toml``.

    Handles both a direct ``version = "x.y.z"`` and Cargo workspace
    inheritance (``version.workspace = true``), in which case the version is
    read from the workspace root ``[workspace.package].version``. Returns
    None on any failure (missing file, parse error, unresolved version).
    """
    import tomllib

    crate_toml = workspace_root / "crates" / "stats-code" / "Cargo.toml"
    try:
        with crate_toml.open("rb") as fh:
            crate = tomllib.load(fh)
    except (OSError, tomllib.TOMLDecodeError):
        return None

    package = crate.get("package", {})
    version = package.get("version")

    # Direct string version.
    if isinstance(version, str) and version:
        return version

    # Workspace inheritance: {"workspace": true} → read workspace root.
    if isinstance(version, dict) and version.get("workspace") is True:
        root_toml = workspace_root / "Cargo.toml"
        try:
            with root_toml.open("rb") as fh:
                root = tomllib.load(fh)
        except (OSError, tomllib.TOMLDecodeError):
            return None
        ws_version = root.get("workspace", {}).get("package", {}).get("version")
        if isinstance(ws_version, str) and ws_version:
            return ws_version

    return None


def _version_from_cli_probe(workspace_root: Path) -> Optional[str]:
    """Resolve the version via a ``stats-code --version`` CLI probe.

    Reuses the cargo-run invocation contract. A non-zero exit, timeout, or
    unparseable output returns None (caller degrades to ``"unknown"``).
    Never raises.
    """
    try:
        result = subprocess.run(
            ["cargo", "run", "--locked", "-q", "-p", "stats-code", "--", "--version"],
            capture_output=True,
            text=True,
            cwd=str(workspace_root),
            timeout=120,
        )
    except Exception:
        return None

    if result.returncode != 0:
        return None

    # Expect output like "stats-code 0.1.0"; take the last whitespace token.
    output = (result.stdout or "").strip()
    if not output:
        return None
    token = output.split()[-1]
    # A plausible version token contains a digit.
    return token if any(ch.isdigit() for ch in token) else None


def resolve_stats_code_version(workspace_root: Optional[Path] = None) -> str:
    """Resolve the Stats Code version, or ``"unknown"``.

    Resolution order (each step falls through on failure):
      1. ``crates/stats-code/Cargo.toml`` ``[package].version`` (handles
         workspace inheritance) — fast, deterministic, no subprocess.
      2. ``stats-code --version`` CLI probe.
      3. ``"unknown"``.

    This function catches all exceptions internally and never raises, so it
    cannot abort metadata collection (Requirement 4.2 / 4.4). It does not
    spawn R / SAS / SPSS / Python reference software (Requirement 4.3).
    """
    if workspace_root is None:
        # result.py → parity/ → validation/ → stats-code/ → crates/ → root
        workspace_root = Path(__file__).resolve().parents[4]

    try:
        from_cargo = _version_from_cargo_toml(workspace_root)
        if from_cargo:
            return from_cargo
        from_probe = _version_from_cli_probe(workspace_root)
        if from_probe:
            return from_probe
    except Exception:
        pass
    return "unknown"


def collect_metadata() -> RunMetadata:
    """Collect all provenance fields for the current run."""
    from datetime import datetime, timezone

    ref_libs = ["numpy", "pandas", "scipy", "statsmodels", "lifelines", "scikit-learn"]
    ref_versions = {lib: _lib_version(lib) for lib in ref_libs}

    return RunMetadata(
        generated_at=datetime.now(timezone.utc).isoformat(),
        stats_code_commit=_git_commit(),
        stats_code_version="unknown",  # populated by run_validation.py after CLI probe
        python_version=f"{sys.version_info.major}.{sys.version_info.minor}.{sys.version_info.micro}",
        rscript_version=_rscript_version(),
        os=platform.platform(),
        reference_engine_versions=ref_versions,
    )


# ---------------------------------------------------------------------------
# Parity report row (Requirements 3.2 / 3.3 / 12.5)
#
# The structures below are the canonical Python mirror of the Rust
# `ParityReportRow` defined in design.md. They live alongside the legacy
# `ValidationResult` because the new parity reporter (task 11.4) consumes
# `ParityRow`, while older method collectors still emit `ValidationResult`.
# Both must keep working until the migration is complete.
# ---------------------------------------------------------------------------


class ParityVerdict(str, Enum):
    """Verdict for one parity comparison row.

    The set is exactly {pass, fail, skipped} per Requirement 3.3.
    """

    PASS = "pass"
    FAIL = "fail"
    SKIPPED = "skipped"


class SkippedReason(str, Enum):
    """Reason a parity row was emitted with verdict == skipped.

    `REFERENCE_SOFTWARE_UNAVAILABLE` triggers a non-zero CI exit code per
    Requirement 4.10. `UNCOVERED_CELL` is used when the Algorithm Coverage
    Matrix records the cell as `none` and the run still has to materialize a
    row (so the report stays a complete grid).
    """

    REFERENCE_SOFTWARE_UNAVAILABLE = "reference_software_unavailable"
    UNCOVERED_CELL = "uncovered_cell"


@dataclass(frozen=True)
class ReferenceImplDescriptor:
    """Identity of the reference implementation used for one parity row.

    Mirrors the Rust `ReferenceImpl` struct in `coverage_matrix/mod.rs`.
    `pkg` is None for SAS / SPSS PROC-style references, where the procedure
    name lives in `name` and there is no separate package container.
    """

    name: str
    pkg: Optional[str]
    version: str


@dataclass(frozen=True)
class ParityRow:
    """One row of the Parity Validation Report.

    Field semantics:
      - `stats_engine_value`   may be None when the row is skipped
        (no Stats Engine call was made because the reference was unavailable).
      - `reference_value_or_na` is None whenever the row has no reference
        numeric (skipped rows or rows where the reference adapter returned
        a non-numeric outcome). The reporter renders None as the literal
        string `n/a` per Requirement 3.3.
      - `absolute_difference`  is None iff `reference_value_or_na` is None.
      - `relative_difference`  is None iff |reference_value_or_na| is at or
        below `active_absolute_tolerance`, or `reference_value_or_na` is
        None. The reporter renders None as the literal string `n/a`.
      - `active_*_tolerance`   are the thresholds actually applied, after
        the per-algorithm lookup against `tolerance_config.yaml`.
      - `verdict`              is one of pass / fail / skipped exactly.
      - `skipped_reason`       is None unless `verdict == ParityVerdict.SKIPPED`.

    The dataclass is frozen so rows are hashable and may be safely
    deduplicated or used as dict keys by the reporter.

    Numeric fields are stored as plain `float` here. Rendering with at
    least 12 significant digits is the reporter's job (task 11.4) and is
    not enforced at construction time.
    """

    algorithm_id: str
    algorithm_display_name: str
    software: str  # one of "R" | "SAS" | "Python" | "SPSS"
    reference_impl: ReferenceImplDescriptor
    case_id: str
    metric: str
    stats_engine_value: Optional[float]
    reference_value_or_na: Optional[float]
    absolute_difference: Optional[float]
    relative_difference: Optional[float]
    active_absolute_tolerance: float
    active_relative_tolerance: float
    verdict: ParityVerdict
    skipped_reason: Optional[SkippedReason] = None


# ---------------------------------------------------------------------------
# Parity report header (Requirements 3.6 / 3.7 / 12.4)
#
# Mirrors the Rust `ParityReportHeader` defined in design.md. Lives next to
# `ParityRow` because the new `ParityReportGenerator` (task 11.4) consumes
# both. Kept as a frozen dataclass so the reporter cannot mutate the header
# mid-render and so two structurally equal headers compare equal.
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class ParityReportHeader:
    """Header section of the Parity Validation Report.

    Field semantics (Requirements 3.6, 3.7, 12.4):
      - ``commit_sha``                — Stats Code commit SHA producing the run.
      - ``run_started_at_utc``        — ISO-8601 UTC timestamp string.
      - ``host_os_family``            — One of ``"Windows" | "Linux" | "macOS"``.
        Validation is the caller's responsibility; the reporter renders the
        value verbatim. Stored as a string (not an enum) to match the JSON
        contract `report.json` exposes to downstream tools.
      - ``host_os_version``           — ≤ 32 characters, no host name / user
        name / home directory (Requirement 9.2 — the exporter handles that).
      - ``reference_software_versions`` — Mapping ``software -> version``
        recording every Reference Software actually invoked in this run.
        Stored ordered by caller; the reporter sorts by key when serialising
        so output is deterministic across dict-insertion orders.
      - ``coverage_matrix``           — The Algorithm Coverage Matrix DTO
        embedded as JSON in the report (Requirement 3.7). The reporter
        treats this as opaque and emits it verbatim; preserving the ``none``
        marker is the matrix builder's job.
      - ``tolerance_diff``            — List of tolerance entries modified
        by the current PR (Requirement 12.4). Each entry is a dict with at
        minimum ``algorithm``, ``previous``, ``new``, ``pr_id`` keys. List
        order is preserved exactly as provided (the PR diff order is the
        reader-friendly order).

    The dataclass is frozen so the reporter cannot rebind a header field
    mid-render. Frozen is the *attribute-binding* barrier here — the dict
    and list members are by their nature mutable in place; rendering code
    treats them as read-only.
    """

    commit_sha: str
    run_started_at_utc: str
    host_os_family: str
    host_os_version: str
    reference_software_versions: dict[str, str]
    coverage_matrix: dict[str, Any]
    tolerance_diff: list[dict[str, Any]]


def compute_differences(
    stats: float,
    reference: Optional[float],
    abs_tol: float,
) -> tuple[Optional[float], Optional[float]]:
    """Compute (absolute_difference, relative_difference) for one parity cell.

    Rules (Requirements 3.3 and 12.5):
      - If `reference is None`, both differences are None (no comparison).
      - Else `absolute_difference = abs(stats - reference)`.
      - If `abs(reference) <= abs_tol`, `relative_difference` is None
        ("n/a"); the reference magnitude is too small to make a relative
        comparison meaningful.
      - Else `relative_difference = absolute_difference / abs(reference)`.

    `abs_tol` MUST be non-negative; callers are expected to source it from
    `ToleranceConfig.lookup(...)` which already enforces that invariant.
    """

    if reference is None:
        return (None, None)

    absolute_difference = abs(stats - reference)
    if abs(reference) <= abs_tol:
        return (absolute_difference, None)
    return (absolute_difference, absolute_difference / abs(reference))
