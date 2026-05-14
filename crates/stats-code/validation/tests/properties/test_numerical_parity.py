# Feature: validation-correctness, Property 1: Numerical Parity
"""
Property 1: For any supported method and dataset, no ValidationResult has
status == FAIL. This is the core end-to-end correctness property.

This test requires:
  - Stats Code CLI to be built (cargo build -p stats-code)
  - Python dependencies installed (pip install -r requirements.txt)
  - Synthetic datasets generated (python datasets/_preprocess/gen_synthetic.py)

Mark slow tests with @pytest.mark.slow; they are skipped by default.
"""
from pathlib import Path

import pytest

from parity.adapters import ADAPTERS_FOR
from parity.result import Status, ToleranceConfig

VALIDATION_DIR = Path(__file__).resolve().parents[2]
CONFIG_PATH = VALIDATION_DIR / "tolerance_config.yaml"
SYNTHETIC_DIR = VALIDATION_DIR / "datasets" / "synthetic"

# Only run on small dataset by default to keep CI fast
FAST_DATASETS = [SYNTHETIC_DIR / "small_n40.csv"]
SLOW_DATASETS = [
    SYNTHETIC_DIR / "medium_n200.csv",
    SYNTHETIC_DIR / "large_n2000.csv",
]

FAST_METHODS = ["linear", "logistic", "cox"]


def _enumerate_applicable_pairs(methods, datasets):
    pairs = []
    for method in methods:
        for dataset in datasets:
            if dataset.exists():
                pairs.append((method, dataset))
    return pairs


@pytest.fixture(scope="module")
def tol_config() -> ToleranceConfig:
    return ToleranceConfig.from_yaml(CONFIG_PATH)


@pytest.mark.parametrize(
    "method,dataset",
    _enumerate_applicable_pairs(FAST_METHODS, FAST_DATASETS),
    ids=lambda x: x.name if isinstance(x, Path) else x,
)
def test_numerical_parity_holds(method: str, dataset: Path, tol_config: ToleranceConfig) -> None:
    """Property 1: no FAIL results for (method, dataset) pair."""
    import importlib
    import sys

    sys.path.insert(0, str(VALIDATION_DIR))
    mod = importlib.import_module(f"parity.{method}")
    adapters = ADAPTERS_FOR.get(method, [])

    results = mod.collect(
        dataset_path=dataset,
        tol_config=tol_config,
        adapters=adapters,
    )

    failures = [r for r in results if r.status == Status.FAIL]
    if failures:
        failure_msgs = "\n".join(
            f"  {r.metric}: expected={r.expected}, actual={r.actual}, "
            f"diff={r.difference:.3e}, tol={r.tolerance:.3e}"
            for r in failures
        )
        pytest.fail(
            f"Numerical parity FAILED for {method} on {dataset.name}:\n{failure_msgs}"
        )


@pytest.mark.slow
@pytest.mark.parametrize(
    "method,dataset",
    _enumerate_applicable_pairs(FAST_METHODS, SLOW_DATASETS),
    ids=lambda x: x.name if isinstance(x, Path) else x,
)
def test_numerical_parity_holds_slow(
    method: str, dataset: Path, tol_config: ToleranceConfig
) -> None:
    """Property 1 (slow): parity on medium and large datasets."""
    test_numerical_parity_holds(method, dataset, tol_config)
