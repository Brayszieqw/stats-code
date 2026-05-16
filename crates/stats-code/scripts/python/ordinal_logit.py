#!/usr/bin/env python3
"""Ordinal logistic regression bridge via statsmodels OrderedModel."""

import math

import numpy as np
import pandas as pd
from scipy import stats
from statsmodels.discrete.discrete_model import Logit
from statsmodels.miscmodels.ordinal_model import OrderedModel

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
    alpha = 1.0 - params.get("ci_level", 0.95)
    df = pd.read_csv(data_path)
    n_total = len(df)
    required = [outcome] + predictors
    model_df = df[required].dropna().copy()
    n_used = len(model_df)
    if n_used == 0:
        raise ValueError("No complete rows remain for ordinal logistic regression")

    levels = list(pd.unique(model_df[outcome]))
    levels = sorted(levels, key=lambda value: str(value))
    if len(levels) < 3:
        raise ValueError(
            "Ordinal logistic regression requires at least 3 ordered outcome levels; use binary logistic regression instead"
        )

    y = pd.Series(
        pd.Categorical(model_df[outcome], categories=levels, ordered=True).codes,
        index=model_df.index,
        name=outcome,
        dtype=float,
    )
    x = _design_matrix(model_df, predictors)
    model = OrderedModel(y, x, distr="logit")
    fit = model.fit(method="bfgs", maxiter=100, disp=False)

    conf = fit.conf_int(alpha=alpha)
    exog_names = set(x.columns)
    coefficients = []
    thresholds = []
    for term, beta in fit.params.items():
        se = float(fit.bse[term])
        lo = float(conf.loc[term, 0])
        hi = float(conf.loc[term, 1])
        if term in exog_names:
            coefficients.append(
                {
                    "term": str(term),
                    "variable": str(term),
                    "level": None,
                    "reference": None,
                    "beta": float(beta),
                    "standard_error": se,
                    "odds_ratio": float(math.exp(beta)),
                    "ci_lower": float(math.exp(lo)),
                    "ci_upper": float(math.exp(hi)),
                    "p_value": float(fit.pvalues[term]),
                }
            )
        else:
            thresholds.append(float(beta))

    brant_chi_square, brant_p = _brant_screen(model_df, outcome, levels, predictors)
    converged = bool(fit.mle_retvals.get("converged", True))
    warnings = [] if converged else ["Model did not converge within statsmodels optimizer limits"]

    return {
        "status": "ok",
        "data_path": data_path,
        "analysis_path": None,
        "n_total": int(n_total),
        "n_used": int(n_used),
        "n_excluded_missing": int(n_total - n_used),
        "notes": [f"Fitted via Python statsmodels {PACKAGE_VERSION} OrderedModel"],
        "warnings": warnings,
        "outcome": outcome,
        "predictors": predictors,
        "thresholds": thresholds,
        "coefficients": coefficients,
        "brant_chi_square": brant_chi_square,
        "brant_p": brant_p,
        "log_likelihood": float(fit.llf),
        "aic": float(fit.aic),
    }


def _design_matrix(df: pd.DataFrame, predictors: list[str]) -> pd.DataFrame:
    x = df[predictors].copy()
    categorical = [name for name in predictors if not pd.api.types.is_numeric_dtype(x[name])]
    if categorical:
        x = pd.get_dummies(x, columns=categorical, drop_first=True, dtype=float)
    return x.astype(float)


def _brant_screen(df: pd.DataFrame, outcome: str, levels: list, predictors: list[str]):
    if len(levels) < 3 or not predictors:
        return None, None
    estimates = []
    variances = []
    x = _design_matrix(df, predictors)
    x = pd.concat([pd.Series(1.0, index=x.index, name="const"), x], axis=1)
    codes = pd.Categorical(df[outcome], categories=levels, ordered=True).codes
    for cut in range(len(levels) - 1):
        y = (codes > cut).astype(int)
        if y.min() == y.max():
            continue
        try:
            fit = Logit(y, x).fit(disp=False, maxiter=100)
        except Exception:
            continue
        estimates.append(fit.params.drop(labels=["const"], errors="ignore").to_numpy(dtype=float))
        variances.append(np.diag(fit.cov_params().drop(index=["const"], columns=["const"], errors="ignore")).astype(float))
    if len(estimates) < 2:
        return None, None
    beta = np.vstack(estimates)
    var = np.vstack(variances)
    mean_beta = beta.mean(axis=0)
    chi = float(np.nansum((beta - mean_beta) ** 2 / np.maximum(var, 1e-12)))
    dfree = int((beta.shape[0] - 1) * beta.shape[1])
    if dfree <= 0:
        return None, None
    return chi, float(1.0 - stats.chi2.cdf(chi, dfree))
