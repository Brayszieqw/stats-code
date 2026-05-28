"""Algorithm Coverage Matrix consistency tests (task 11.6).

Validates Requirements 4.7, 4.8, 6.5, 6.6.

For every (algorithm, software) cell in the Algorithm Coverage Matrix:

* ``live``         ⇒ a Live test surface MUST exist for that algorithm.
                     Wave-1 heuristic (task 11.7 lands the full surface):
                     ``parity/<algorithm_id>.py`` exists OR the algorithm_id
                     appears in ``parity/__init__.py``'s declared known-method
                     set (currently empty).
* ``recorded``     ⇒ a Known-Values directory at
                     ``validation/known_values/<software>/<algorithm_id>/``
                     MUST exist and contain at least one ``*.json`` fixture.
* ``sidecar_only`` ⇒ a sidecar template at
                     ``crates/stats-code/src/sidecar/templates/<software>/<algorithm_id>.tmpl.txt``
                     MUST exist.
* ``none``         ⇒ NO sidecar template, NO Known-Values directory, AND
                     NO Live test case for that cell.

The ``recorded`` and ``sidecar_only`` (positive) arms together with the
``none`` (negative) arm are written *strictly* — they assert hard equality
with the matrix cell value. The ``live`` arm uses the wave-1 heuristic.
"""

from __future__ import annotations

import sys
from pathlib import Path
from typing import Any

# ---------------------------------------------------------------------------
# Path bootstrap — make ``run_validation`` importable regardless of cwd.
# ``tests/unit/`` is two levels under ``validation/``.
# ---------------------------------------------------------------------------

_VALIDATION_DIR = Path(__file__).resolve().parents[2]
_STATS_CODE_DIR = _VALIDATION_DIR.parent  # crates/stats-code/
_TEMPLATES_DIR = _STATS_CODE_DIR / "src" / "sidecar" / "templates"
_KNOWN_VALUES_DIR = _VALIDATION_DIR / "known_values"
_PARITY_DIR = _VALIDATION_DIR / "parity"

if str(_VALIDATION_DIR) not in sys.path:
    sys.path.insert(0, str(_VALIDATION_DIR))

import run_validation  # noqa: E402


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def _matrix_entries() -> list[dict[str, Any]]:
    """Return the parsed ``[[algorithm]]`` array from the build-mirrored matrix."""
    matrix = run_validation.load_coverage_matrix(_VALIDATION_DIR)
    entries = matrix.get("algorithm")
    assert isinstance(entries, list) and entries, (
        "coverage_matrix.toml must contain at least one [[algorithm]] entry "
        "(was the build.rs mirror step run?)"
    )
    return entries


def _known_values_dir(software: str, algorithm_id: str) -> Path:
    """Resolve ``validation/known_values/<software>/<algorithm_id>/``.

    ``software`` is lower-cased to match the on-disk convention (the matrix
    spells software names in TitleCase: ``R``, ``SAS``, ``Python``, ``SPSS``).
    """
    return _KNOWN_VALUES_DIR / software.lower() / algorithm_id


def _template_path(software: str, algorithm_id: str) -> Path:
    """Resolve ``src/sidecar/templates/<software>/<algorithm_id>.tmpl.txt``."""
    return _TEMPLATES_DIR / software.lower() / f"{algorithm_id}.tmpl.txt"


def _known_methods_from_init() -> set[str]:
    """Return the known-method identifier set declared in ``parity/__init__.py``.

    The set is consulted only as the second arm of the heuristic Live-cell
    check — see the module docstring. Today ``__init__.py`` is empty, so this
    returns the empty set; once it grows a ``KNOWN_METHODS`` constant, this
    helper picks it up automatically.
    """
    try:
        import parity  # noqa: WPS433 — local import to avoid pkg-level side-effects at collection time
    except ImportError:
        return set()
    raw = getattr(parity, "KNOWN_METHODS", None)
    if raw is None:
        return set()
    return {str(name) for name in raw}


def _live_collector_present(algorithm_id: str) -> bool:
    """Wave-1 heuristic for "Live test case exists for this cell".

    True iff ``parity/<algorithm_id>.py`` exists OR ``algorithm_id`` is
    declared in ``parity/__init__.py``'s ``KNOWN_METHODS`` set.
    """
    if (_PARITY_DIR / f"{algorithm_id}.py").is_file():
        return True
    return algorithm_id in _known_methods_from_init()


# ---------------------------------------------------------------------------
# Tests — one per coverage value
# ---------------------------------------------------------------------------

def test_every_recorded_cell_has_known_values():
    """``recorded`` ⇒ at least one Known-Values JSON exists for that cell.

    Validates Requirements 4.7, 4.8, 6.6.
    """
    missing: list[str] = []
    for entry in _matrix_entries():
        algorithm_id = str(entry["id"])
        coverage = entry.get("coverage", {})
        for software, value in coverage.items():
            if value != "recorded":
                continue
            kv_dir = _known_values_dir(software, algorithm_id)
            if not kv_dir.is_dir():
                missing.append(
                    f"recorded cell ({algorithm_id}, {software}): "
                    f"missing known_values directory {kv_dir}"
                )
                continue
            json_files = sorted(kv_dir.glob("*.json"))
            if not json_files:
                missing.append(
                    f"recorded cell ({algorithm_id}, {software}): "
                    f"no JSON fixtures in {kv_dir}"
                )

    assert not missing, (
        "Algorithm Coverage Matrix has 'recorded' cells without "
        f"Known-Values fixtures:\n  - " + "\n  - ".join(missing)
    )


def test_every_sidecar_only_cell_has_template():
    """``sidecar_only`` ⇒ a sidecar template exists for that cell.

    Validates Requirements 6.5, 6.6.
    """
    missing: list[str] = []
    for entry in _matrix_entries():
        algorithm_id = str(entry["id"])
        coverage = entry.get("coverage", {})
        for software, value in coverage.items():
            if value != "sidecar_only":
                continue
            tmpl = _template_path(software, algorithm_id)
            if not tmpl.is_file():
                missing.append(
                    f"sidecar_only cell ({algorithm_id}, {software}): "
                    f"missing template {tmpl}"
                )

    assert not missing, (
        "Algorithm Coverage Matrix has 'sidecar_only' cells without "
        f"templates:\n  - " + "\n  - ".join(missing)
    )


def test_every_none_cell_has_no_template_and_no_known_values():
    """``none`` ⇒ no template, no Known-Values dir, no Live collector.

    Validates Requirements 6.5, 6.6 (negative arm).
    """
    leakage: list[str] = []
    for entry in _matrix_entries():
        algorithm_id = str(entry["id"])
        coverage = entry.get("coverage", {})
        for software, value in coverage.items():
            if value != "none":
                continue

            tmpl = _template_path(software, algorithm_id)
            if tmpl.is_file():
                leakage.append(
                    f"none cell ({algorithm_id}, {software}): "
                    f"unexpected template at {tmpl}"
                )

            kv_dir = _known_values_dir(software, algorithm_id)
            if kv_dir.is_dir() and any(kv_dir.glob("*.json")):
                leakage.append(
                    f"none cell ({algorithm_id}, {software}): "
                    f"unexpected Known-Values fixtures under {kv_dir}"
                )

            # Live-collector leakage only matters when a Live test case is
            # uniquely scoped to (algorithm, software); the wave-1 heuristic
            # cannot distinguish per-software collectors, so we only flag
            # leakage when no other (algorithm, *) cell is `live`. This
            # avoids false positives from shared parity modules that legit-
            # imately serve `live` cells in other software columns.
            other_live = any(
                v == "live"
                for sw, v in coverage.items()
                if sw != software
            )
            if not other_live and _live_collector_present(algorithm_id):
                leakage.append(
                    f"none cell ({algorithm_id}, {software}): "
                    f"unexpected parity collector parity/{algorithm_id}.py"
                )

    assert not leakage, (
        "Algorithm Coverage Matrix has 'none' cells with leaked test "
        f"surface:\n  - " + "\n  - ".join(leakage)
    )


def test_every_live_cell_has_a_collector():
    """``live`` ⇒ a parity collector module exists for that algorithm.

    Wave-1 heuristic per task 11.6 — full per-software Live test surface
    lands in task 11.7. Validates Requirement 4.7, 4.8 (live arm).
    """
    missing: list[str] = []
    for entry in _matrix_entries():
        algorithm_id = str(entry["id"])
        coverage = entry.get("coverage", {})
        for software, value in coverage.items():
            if value != "live":
                continue
            if not _live_collector_present(algorithm_id):
                missing.append(
                    f"live cell ({algorithm_id}, {software}): "
                    f"no parity/{algorithm_id}.py and not declared in "
                    f"parity.__init__.KNOWN_METHODS"
                )

    assert not missing, (
        "Algorithm Coverage Matrix has 'live' cells without a parity "
        f"collector:\n  - " + "\n  - ".join(missing)
    )
