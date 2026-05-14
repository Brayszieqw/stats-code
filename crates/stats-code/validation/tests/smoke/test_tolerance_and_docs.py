"""Smoke test: tolerance_config.yaml covers all 9 methods; README has Tolerance Rationale."""
from pathlib import Path

import yaml

VALIDATION_DIR = Path(__file__).resolve().parents[2]

REQUIRED_METHODS = {
    "linear", "logistic", "cox", "km", "rate",
    "power", "math_core", "tableone", "diagnostic_roc",
}


def test_tolerance_config_covers_all_methods():
    config_path = VALIDATION_DIR / "tolerance_config.yaml"
    assert config_path.exists(), "tolerance_config.yaml not found"

    with open(config_path, encoding="utf-8") as f:
        raw = yaml.safe_load(f)

    assert "default" in raw, "tolerance_config.yaml missing 'default' key"
    per_metric = raw.get("per_metric", {})
    assert per_metric, "tolerance_config.yaml has empty per_metric"

    # Extract method prefixes from keys like "linear.beta"
    covered_methods = {k.split(".")[0] for k in per_metric}
    missing = REQUIRED_METHODS - covered_methods
    assert not missing, f"tolerance_config.yaml missing entries for methods: {missing}"


def test_readme_has_tolerance_rationale():
    readme = VALIDATION_DIR / "README.md"
    assert readme.exists(), "validation/README.md not found"
    content = readme.read_text(encoding="utf-8")
    assert "Tolerance Rationale" in content, (
        "README.md missing 'Tolerance Rationale' section"
    )
