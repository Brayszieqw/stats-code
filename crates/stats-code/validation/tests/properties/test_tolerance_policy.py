# Feature: validation-correctness, Property 3: Tolerance Policy Monotonicity
"""
Property 3: Iterative methods have tolerance ≤ 1e-5 for coefficient metrics;
closed-form methods have tolerance ≤ 1e-8; KM survival_probability ≤ 1e-6.
"""
from pathlib import Path

import pytest

from parity.result import ToleranceConfig

VALIDATION_DIR = Path(__file__).resolve().parents[2]
CONFIG_PATH = VALIDATION_DIR / "tolerance_config.yaml"


@pytest.fixture(scope="module")
def tol_config() -> ToleranceConfig:
    return ToleranceConfig.from_yaml(CONFIG_PATH)


# Logistic (IRLS): coefficient-like metrics must have tolerance ≤ 1e-5
LOGISTIC_COEF_METRICS = [
    ("logistic", "beta"),
    ("logistic", "stderr"),
    ("logistic", "wald"),
    ("logistic", "odds_ratio"),
    ("logistic", "log_likelihood"),
]

# Cox (partial likelihood + tie handling): tolerance ≤ 1e-4 per design.md.
# Stats Code now matches lifelines' default Efron tie handling, so
# log_partial_likelihood is held to the same budget as the other Cox metrics.
COX_COEF_METRICS = [
    ("cox", "beta"),
    ("cox", "stderr"),
    ("cox", "hazard_ratio"),
    ("cox", "log_partial_likelihood"),
]

@pytest.mark.parametrize("method,metric", LOGISTIC_COEF_METRICS)
def test_logistic_coef_tolerance_le_1e5(
    tol_config: ToleranceConfig, method: str, metric: str
) -> None:
    """Property 3: logistic coefficient metrics must have tolerance ≤ 1e-5."""
    tol = tol_config.lookup(method, metric)
    assert tol <= 1e-5, (
        f"{method}.{metric}: tolerance {tol:.2e} exceeds 1e-5 limit for logistic"
    )


@pytest.mark.parametrize("method,metric", COX_COEF_METRICS)
def test_cox_coef_tolerance_le_1e4(
    tol_config: ToleranceConfig, method: str, metric: str
) -> None:
    """Property 3: Cox coefficient metrics must have tolerance ≤ 1e-4 (tie handling)."""
    tol = tol_config.lookup(method, metric)
    assert tol <= 1e-4, (
        f"{method}.{metric}: tolerance {tol:.2e} exceeds 1e-4 limit for Cox PLE"
    )


# Closed-form methods: primary metrics must have tolerance ≤ 1e-8
CLOSED_FORM_METRICS = [
    ("linear", "beta"),
    ("linear", "stderr"),
    ("linear", "r_squared"),
    ("linear", "adj_r_squared"),
    ("km", "survival_probability"),
    ("km", "greenwood_se"),
    ("rate", "estimate_per_1000"),
]

@pytest.mark.parametrize("method,metric", CLOSED_FORM_METRICS)
def test_closed_form_tolerance_le_1e8(
    tol_config: ToleranceConfig, method: str, metric: str
) -> None:
    """Property 3: closed-form primary metrics must have tolerance ≤ 1e-8."""
    tol = tol_config.lookup(method, metric)
    assert tol <= 1e-8, (
        f"{method}.{metric}: tolerance {tol:.2e} exceeds 1e-8 limit for closed-form methods"
    )


def test_km_survival_probability_le_1e6(tol_config: ToleranceConfig) -> None:
    """Property 3: KM survival_probability tolerance ≤ 1e-6."""
    tol = tol_config.lookup("km", "survival_probability")
    assert tol <= 1e-6, f"km.survival_probability tolerance {tol:.2e} exceeds 1e-6"
