"""Smoke test: known_values/ contains ≥3 JSON files with required keys."""
import json
from pathlib import Path

VALIDATION_DIR = Path(__file__).resolve().parents[2]
KNOWN_VALUES_DIR = VALIDATION_DIR / "known_values"


def test_known_values_count():
    json_files = list(KNOWN_VALUES_DIR.glob("*.json"))
    assert len(json_files) >= 3, (
        f"Expected ≥3 known-value JSON files, found {len(json_files)}"
    )


def test_known_values_have_required_keys():
    json_files = list(KNOWN_VALUES_DIR.glob("*.json"))
    for path in json_files:
        with open(path, encoding="utf-8") as f:
            data = json.load(f)
        assert "method" in data, f"{path.name} missing 'method' key"
        assert "expected" in data, f"{path.name} missing 'expected' key"
        assert isinstance(data["expected"], dict), (
            f"{path.name}: 'expected' must be a dict"
        )
        assert len(data["expected"]) > 0, f"{path.name}: 'expected' dict is empty"
