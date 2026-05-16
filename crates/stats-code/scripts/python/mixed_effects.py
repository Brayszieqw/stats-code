#!/usr/bin/env python3
"""Linear mixed-effects model bridge via statsmodels MixedLM."""

import math

import numpy as np
import pandas as pd
import statsmodels.formula.api as smf

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
    random = params["random"]
    alpha = 1.0 - params.get("ci_level", 0.95)
    df = pd.read_csv(data_path)
    n_total = len(df)
    model_df = df[[outcome, random] + predictors].dropna().copy()
    n_used = len(model_df)
    if model_df[random].nunique() < 2:
        raise ValueError("Mixed-effects model requires at least two random-effect groups")
    formula = f"{outcome} ~ " + (" + ".join(predictors) if predictors else "1")
    fit = smf.mixedlm(formula, model_df, groups=model_df[random]).fit(reml=True, maxiter=100)
    conf = fit.conf_int(alpha=alpha)
    fixed_effects = []
    for term, estimate in fit.fe_params.items():
        se = float(fit.bse_fe[term])
        fixed_effects.append(
            {
                "term": str(term),
                "estimate": float(estimate),
                "standard_error": se,
                "ci_lower": float(conf.loc[term, 0]),
                "ci_upper": float(conf.loc[term, 1]),
                "p_value": float(fit.pvalues.get(term, math.nan)),
            }
        )
    random_var = float(fit.cov_re.iloc[0, 0]) if fit.cov_re.size else 0.0
    residual_var = float(fit.scale)
    llf = float(fit.llf)
    k = len(fit.params)
    aic = float(fit.aic) if np.isfinite(fit.aic) else float(-2.0 * llf + 2.0 * k)
    bic = float(fit.bic) if np.isfinite(fit.bic) else float(-2.0 * llf + math.log(max(n_used, 1)) * k)

    return {
        "status": "ok",
        "data_path": data_path,
        "analysis_path": None,
        "n_total": int(n_total),
        "n_used": int(n_used),
        "n_excluded_missing": int(n_total - n_used),
        "notes": [f"Fitted via Python statsmodels {PACKAGE_VERSION} MixedLM REML"],
        "warnings": [] if bool(getattr(fit, "converged", True)) else ["Model did not converge"],
        "outcome": outcome,
        "predictors": predictors,
        "random_group": random,
        "n_groups": int(model_df[random].nunique()),
        "iterations": 0,
        "converged": bool(getattr(fit, "converged", True)),
        "fixed_effects": fixed_effects,
        "random_intercept_variance": random_var,
        "residual_variance": residual_var,
        "icc": float(random_var / max(random_var + residual_var, 1e-12)),
        "log_likelihood": llf,
        "aic": aic,
        "bic": bic,
    }
