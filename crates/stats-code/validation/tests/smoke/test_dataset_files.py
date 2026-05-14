"""Smoke test: dataset files exist and PROVENANCE docs are present."""
from pathlib import Path

VALIDATION_DIR = Path(__file__).resolve().parents[2]
DATASETS_DIR = VALIDATION_DIR / "datasets"


def test_synthetic_csvs_exist():
    synthetic = DATASETS_DIR / "synthetic"
    csvs = list(synthetic.glob("*.csv"))
    assert len(csvs) >= 3, f"Expected ≥3 synthetic CSVs, found {len(csvs)}: {csvs}"
    names = {f.name for f in csvs}
    assert "small_n40.csv" in names
    assert "medium_n200.csv" in names
    assert "large_n2000.csv" in names


def test_edge_case_csvs_exist():
    edge = DATASETS_DIR / "edge_cases"
    csvs = list(edge.glob("*.csv"))
    assert len(csvs) >= 5, f"Expected ≥5 edge case CSVs, found {len(csvs)}: {csvs}"
    names = {f.name for f in csvs}
    assert "logistic_perfect_separation.csv" in names
    assert "survival_tied_times.csv" in names
    assert "zero_variance_covariate.csv" in names
    assert "single_obs_group.csv" in names
    assert "collinear_predictors.csv" in names


def test_provenance_docs_exist():
    for subdir in ("synthetic", "edge_cases"):
        provenance = DATASETS_DIR / subdir / "PROVENANCE.md"
        assert provenance.exists(), f"Missing PROVENANCE.md in datasets/{subdir}/"
        content = provenance.read_text(encoding="utf-8")
        assert len(content) > 100, f"PROVENANCE.md in {subdir} looks empty"
