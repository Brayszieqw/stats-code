#!/usr/bin/env python3
"""Multinomial logistic regression bridge via statsmodels MNLogit."""

import math

import pandas as pd
import statsmodels.api as sm
from statsmodels.discrete.discrete_model import MNLogit

try:
    import statsmodels

    PACKAGE_NAME = "statsmodels"
    PACKAGE_VERSION = statsmodels.__version__
except Exception:
    PACKAGE_NAME = "statsmodels"
    PACKAGE_VERSION = "unknown"


def run(data_path: str, params: dict) -> dict:
    outcome = params["outcome"]
    predictors = params.get("predictors", [])
    reference = params.get("reference")
    alpha = 1.0 - params.get("ci_level", 0.95)
    df = pd.read_csv(data_path)
    n_total = len(df)
    model_df = df[[outcome] + predictors].dropna().copy()
    n_used = len(model_df)
    categories = list(pd.unique(model_df[outcome]))
    categories = sorted(categories, key=lambda value: str(value))
    if len(categories) < 3:
        raise ValueError(
            "Multinomial logistic regression requires at least 3 outcome categories; use binary logistic regression instead"
        )
    if reference is not None:
        if reference not in [str(c) for c in categories] and reference not in categories:
            raise ValueError(f"Reference category `{reference}` was not found in outcome `{outcome}`")
        ref_value = next(c for c in categories if str(c) == str(reference))
        categories = [ref_value] + [c for c in categories if c != ref_value]
    else:
        reference = str(categories[0])

    y = pd.Categorical(model_df[outcome], categories=categories).codes
    x = _design_matrix(model_df, predictors)
    x = sm.add_constant(x, has_constant="add")
    fit = MNLogit(y, x).fit(method="newton", maxiter=100, disp=False)
    conf = fit.conf_int(alpha=alpha)

    groups = []
    warnings = []
    for col_idx, category in enumerate(categories[1:]):
        coefficients = []
        for term in fit.params.index:
            beta_raw = float(fit.params.loc[term, col_idx])
            se_raw = float(fit.bse.loc[term, col_idx])
            ci_key = (str(col_idx + 1), term)
            lo_raw = float(conf.loc[ci_key, "lower"]) if ci_key in conf.index else beta_raw - 1.96 * se_raw
            hi_raw = float(conf.loc[ci_key, "upper"]) if ci_key in conf.index else beta_raw + 1.96 * se_raw
            p_raw = float(fit.pvalues.loc[term, col_idx])
            if not all(math.isfinite(v) for v in [beta_raw, se_raw, lo_raw, hi_raw, p_raw]):
                warnings.append(
                    f"Non-finite MNLogit estimate for category `{category}`, term `{term}`; reported as 0.0"
                )
            beta = _finite(beta_raw)
            se = _finite(se_raw)
            lo = _finite(lo_raw)
            hi = _finite(hi_raw)
            p_value = _finite(p_raw, 1.0)
            coefficients.append(
                {
                    "term": str(term),
                    "variable": str(term),
                    "level": None,
                    "reference": str(reference),
                    "beta": beta,
                    "standard_error": se,
                    "odds_ratio": float(math.exp(beta)),
                    "ci_lower": float(math.exp(lo)),
                    "ci_upper": float(math.exp(hi)),
                    "p_value": p_value,
                }
            )
        groups.append({"category": str(category), "coefficients": coefficients})

    return {
        "status": "ok",
        "data_path": data_path,
        "analysis_path": None,
        "n_total": int(n_total),
        "n_used": int(n_used),
        "n_excluded_missing": int(n_total - n_used),
        "notes": [f"Fitted via Python statsmodels {PACKAGE_VERSION} MNLogit"],
        "warnings": warnings,
        "outcome": outcome,
        "predictors": predictors,
        "reference": str(reference),
        "categories": [str(c) for c in categories],
        "coefficients_per_category": groups,
        "log_likelihood": _finite(float(fit.llf)),
        "aic": _finite(float(fit.aic)),
        "pseudo_r2": _finite(float(getattr(fit, "prsquared", 0.0))),
    }


def _design_matrix(df: pd.DataFrame, predictors: list[str]) -> pd.DataFrame:
    x = df[predictors].copy()
    categorical = [name for name in predictors if not pd.api.types.is_numeric_dtype(x[name])]
    if categorical:
        x = pd.get_dummies(x, columns=categorical, drop_first=True, dtype=float)
    return x.astype(float)


def _finite(value: float, default: float = 0.0) -> float:
    return float(value) if math.isfinite(value) else default
