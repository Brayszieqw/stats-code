"""Unit tests for the Reference-Software adapters in ``parity.adapters``.

Covers task 11.2 of the parity-and-multilang-sidecar spec.
Validates Requirements 4.3, 4.4, 4.9.

The Reference-Software adapters answer "is this Reference Software runnable
on the current host?" for each of the four columns of the Algorithm Coverage
Matrix:

  - ``R``       — Live, probed via ``shutil.which("Rscript")``.
  - ``Python``  — Live, probed via ``importlib.import_module`` for the three
                  required reference libraries (statsmodels, scipy, lifelines).
  - ``SAS``     — Recorded; always available because fixtures are bundled
                  under ``validation/known_values/sas/``.
  - ``SPSS``    — Recorded; same pattern as SAS.

These tests mock ``shutil.which`` / ``importlib.import_module`` rather than
relying on the actual host environment so the suite stays deterministic on
CI runners that may or may not have Rscript on PATH.
"""

from __future__ import annotations

import json
from pathlib import Path
from unittest.mock import patch

import pytest

from parity.adapters import (
    PythonAdapter,
    RecordedSasAdapter,
    RecordedSpssAdapter,
    ReferenceSoftwareAdapter,
    RscriptAdapter,
    get_adapter,
)


# ---------------------------------------------------------------------------
# Recorded adapters: always available (Requirement 4.4)
# ---------------------------------------------------------------------------

def test_recorded_sas_adapter_is_always_available():
    """Requirement 4.4: SAS goes through Known-Values, no live SAS needed."""
    assert RecordedSasAdapter().is_available() is True


def test_recorded_spss_adapter_is_always_available():
    """Requirement 4.4: SPSS goes through Known-Values, no live SPSS needed."""
    assert RecordedSpssAdapter().is_available() is True


def test_recorded_sas_adapter_advertises_correct_name():
    assert RecordedSasAdapter().name == "SAS"


def test_recorded_spss_adapter_advertises_correct_name():
    assert RecordedSpssAdapter().name == "SPSS"


def test_recorded_sas_run_case_loads_fixture_payload(tmp_path: Path):
    """run_case loads the JSON payload under known_values/sas/<algo>/<case>.json.

    The adapter takes its fixture root from ``__file__`` of ``parity.adapters``.
    To exercise the loader without polluting the real ``known_values/`` tree,
    we monkey-patch the resolved directory on the instance.
    """
    fixture_root = tmp_path / "sas" / "logistic"
    fixture_root.mkdir(parents=True)
    payload = {
        "input": {"n": 100},
        "expected": {"beta[age]": 0.123456789012},
        "software": "SAS",
        "version": "9.4M8",
    }
    (fixture_root / "synthetic_n100.json").write_text(
        json.dumps(payload), encoding="utf-8"
    )

    adapter = RecordedSasAdapter()
    adapter._known_values_dir = tmp_path / "sas"

    loaded = adapter.run_case("logistic", Path("synthetic_n100"))
    assert loaded == payload


def test_recorded_sas_run_case_raises_for_missing_fixture(tmp_path: Path):
    adapter = RecordedSasAdapter()
    adapter._known_values_dir = tmp_path / "sas"

    with pytest.raises(FileNotFoundError):
        adapter.run_case("logistic", Path("does_not_exist"))


# ---------------------------------------------------------------------------
# RscriptAdapter: matches shutil.which("Rscript") semantics (Requirement 4.3)
# ---------------------------------------------------------------------------

def test_rscript_adapter_available_when_which_returns_path():
    """is_available is True iff shutil.which finds Rscript on PATH."""
    with patch("parity.adapters.shutil.which", return_value="/usr/bin/Rscript"):
        adapter = RscriptAdapter()
        assert adapter.is_available() is True


def test_rscript_adapter_unavailable_when_which_returns_none():
    """is_available is False when Rscript is not on PATH (Requirement 4.9)."""
    with patch("parity.adapters.shutil.which", return_value=None):
        adapter = RscriptAdapter()
        assert adapter.is_available() is False


def test_rscript_adapter_caches_availability_after_first_probe():
    """Probing once avoids redundant PATH scans across many parity rows."""
    with patch(
        "parity.adapters.shutil.which", return_value="/usr/bin/Rscript"
    ) as mock_which:
        adapter = RscriptAdapter()
        assert adapter.is_available() is True
        assert adapter.is_available() is True
        assert adapter.is_available() is True
        # The cache means we only probe PATH once per adapter instance.
        assert mock_which.call_count == 1


def test_rscript_adapter_advertises_correct_name():
    assert RscriptAdapter().name == "R"


# ---------------------------------------------------------------------------
# PythonAdapter: probes required libraries via importlib (Requirement 4.3)
# ---------------------------------------------------------------------------

def test_python_adapter_available_in_test_environment():
    """The validation env pins statsmodels / scipy / lifelines, so the
    adapter must be available when the test suite itself runs."""
    assert PythonAdapter().is_available() is True


def test_python_adapter_advertises_correct_name():
    assert PythonAdapter().name == "Python"


def test_python_adapter_unavailable_when_required_lib_missing():
    """If any required reference library cannot be imported, the host is
    not a valid Live Python reference and the orchestrator must short-
    circuit to verdict=skipped per Requirement 4.9."""

    def fake_import(name: str):
        if name == "lifelines":
            raise ImportError(f"No module named {name!r}")
        # Simulate the other libs being importable without actually invoking
        # importlib (would re-enter the patched function).
        return object()

    with patch("parity.adapters.importlib.import_module", side_effect=fake_import):
        adapter = PythonAdapter()
        assert adapter.is_available() is False


def test_python_adapter_caches_availability_after_first_probe():
    """Same caching contract as RscriptAdapter — one probe per instance."""

    call_count = {"n": 0}

    def fake_import(name: str):
        call_count["n"] += 1
        return object()

    with patch("parity.adapters.importlib.import_module", side_effect=fake_import):
        adapter = PythonAdapter()
        assert adapter.is_available() is True
        first = call_count["n"]
        assert adapter.is_available() is True
        assert adapter.is_available() is True
        # No additional imports after the first probe completes.
        assert call_count["n"] == first


# ---------------------------------------------------------------------------
# get_adapter factory
# ---------------------------------------------------------------------------

@pytest.mark.parametrize(
    "software, expected_cls",
    [
        ("R", RscriptAdapter),
        ("Python", PythonAdapter),
        ("SAS", RecordedSasAdapter),
        ("SPSS", RecordedSpssAdapter),
    ],
)
def test_get_adapter_returns_correct_class_per_software(software, expected_cls):
    """The factory dispatches each Reference Software identifier to its
    matching adapter class (Requirements 4.3, 4.4)."""
    adapter = get_adapter(software)
    assert isinstance(adapter, expected_cls)
    assert adapter.name == software


def test_get_adapter_returns_fresh_instance_per_call():
    """A fresh instance per call avoids sharing availability caches across
    orchestration runs that may have mutated PATH or installed deps."""
    a1 = get_adapter("R")
    a2 = get_adapter("R")
    assert a1 is not a2


def test_get_adapter_raises_value_error_for_unknown_software():
    """Unknown identifiers must raise ValueError, not silently fall through."""
    with pytest.raises(ValueError, match="unknown software"):
        get_adapter("Unknown")


def test_get_adapter_is_case_sensitive():
    """The matrix uses canonical casing (R / Python / SAS / SPSS); any
    other casing is treated as unknown so the factory cannot mask a typo."""
    with pytest.raises(ValueError):
        get_adapter("r")
    with pytest.raises(ValueError):
        get_adapter("python")


# ---------------------------------------------------------------------------
# Protocol conformance
# ---------------------------------------------------------------------------

@pytest.mark.parametrize(
    "adapter",
    [
        RscriptAdapter(),
        PythonAdapter(),
        RecordedSasAdapter(),
        RecordedSpssAdapter(),
    ],
)
def test_every_adapter_satisfies_the_protocol(adapter):
    """Each concrete adapter must satisfy the runtime-checkable Protocol so
    the orchestrator can treat them uniformly."""
    assert isinstance(adapter, ReferenceSoftwareAdapter)
