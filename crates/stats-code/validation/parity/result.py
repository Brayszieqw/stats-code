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
