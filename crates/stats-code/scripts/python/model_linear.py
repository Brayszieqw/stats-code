#!/usr/bin/env python3
"""
Stats Code Bridge — Linear Regression via statsmodels OLS.

Outputs a result dict matching the LinearResult schema.
"""
import numpy as np
import pandas as pd
import statsmodels.api as sm

try:
    import statsmodels
    PACKAGE_NAME = "statsmodels"
    PACKAGE_VERSION = statsmodels.__version__
except Exception:
    PACKAGE_NAME = "statsmodels"
    PACKAGE_VERSION = "unknown"


def run(data_path: str, params: dict) -> dict:
    """Run OLS linear regression and return a LinearResult-compatible dict."""
    outcome = params["outcome"]
    predictors = params.get("predictors", [])
    ci_level = params.get("ci_level", 0.95)
    alpha = 1.0 - ci_level

    df = pd.read_csv(data_path)
    n_total = len(df)

    # Build design matrix
    import patsy
    formula_parts = []
    for pred in predictors:
        col = df[pred]
        if col.dtype == object or col.nunique() <= 5:
            formula_parts.append(f"C({pred})")
        else:
            formula_parts.append(pred)

    if not formula_parts:
        raise ValueError("No predictors specified")

    formula_str = f"{outcome} ~ " + " + ".join(formula_parts)
    y_design, X = patsy.dmatrices(formula_str, df, return_type="dataframe",
                                   NA_action="drop")

    y_vec = y_design.iloc[:, 0]
    n_used = len(y_vec)
    n_excluded_missing = n_total - n_used

    # Fit OLS
    model = sm.OLS(y_vec, X)
    result = model.fit()

    r_squared = float(result.rsquared)
    adj_r_squared = float(result.rsquared_adj)
    f_stat = float(result.fvalue) if result.fvalue is not None else None
    f_pval = float(result.f_pvalue) if result.f_pvalue is not None else None
    residual_se = float(np.sqrt(result.mse_resid))
    aic = float(result.aic)
    bic = float(result.bic)

    # Build coefficients
    conf = result.conf_int(alpha=alpha)
    coefficients = []
    for i, term in enumerate(X.columns):
        beta = float(result.params.iloc[i])
        se = float(result.bse.iloc[i])
        t_stat = float(result.tvalues.iloc[i])
        ci_lo = float(conf.iloc[i, 0])
        ci_hi = float(conf.iloc[i, 1])
        pval = float(result.pvalues.iloc[i])

        variable, level, reference = _parse_term(term)

        coefficients.append({
            "term": term,
            "variable": variable,
            "level": level,
            "reference": reference,
            "beta": beta,
            "standard_error": se,
            "t_statistic": t_stat,
            "ci_lower": ci_lo,
            "ci_upper": ci_hi,
            "p_value": pval,
        })

    warnings_list = []

    return {
        "status": "ok",
        "data_path": data_path,
        "analysis_path": None,
        "formula": formula_str,
        "outcome": outcome,
        "predictors": predictors,
        "n_total": n_total,
        "n_used": n_used,
        "n_excluded_missing": n_excluded_missing,
        "n_excluded_invalid": 0,
        "converged": True,
        "r_squared": r_squared,
        "adjusted_r_squared": adj_r_squared,
        "f_statistic": f_stat,
        "f_p_value": f_pval,
        "residual_std_error": residual_se,
        "aic": aic,
        "bic": bic,
        "coefficients": coefficients,
        "notes": [
            f"Fitted via Python statsmodels {PACKAGE_VERSION} OLS",
            f"CI level: {ci_level}",
        ],
        "warnings": warnings_list,
    }


def _parse_term(term: str):
    """Parse patsy term name into (variable, level, reference)."""
    import re
    m = re.match(r"C\((\w+)\)\[T\.(.+)\]", term)
    if m:
        return m.group(1), m.group(2), "T"
    if term == "Intercept":
        return "Intercept", None, None
    return term, None, None
