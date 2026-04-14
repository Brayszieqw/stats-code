#!/usr/bin/env python3
"""
Stats Code Bridge — Logistic Regression via statsmodels GLM.

Outputs a result dict matching the LogisticResult schema.
"""
import json
import numpy as np
import pandas as pd
import statsmodels.api as sm
from statsmodels.genmod.families import Binomial

try:
    import statsmodels
    PACKAGE_NAME = "statsmodels"
    PACKAGE_VERSION = statsmodels.__version__
except Exception:
    PACKAGE_NAME = "statsmodels"
    PACKAGE_VERSION = "unknown"


def run(data_path: str, params: dict) -> dict:
    """Run logistic regression and return a LogisticResult-compatible dict."""
    outcome = params["outcome"]
    predictors = params.get("predictors", [])
    ci_level = params.get("ci_level", 0.95)
    alpha = 1.0 - ci_level

    # Load data
    df = pd.read_csv(data_path)
    n_total = len(df)

    # Resolve outcome
    y = df[outcome]

    # Build design matrix with automatic categorical expansion
    formula_parts = []
    for pred in predictors:
        col = df[pred]
        if col.dtype == object or col.nunique() <= 5:
            formula_parts.append(f"C({pred})")
        else:
            formula_parts.append(pred)

    if not formula_parts:
        raise ValueError("No predictors specified")

    import patsy
    formula_str = f"{outcome} ~ " + " + ".join(formula_parts)
    y_design, X = patsy.dmatrices(formula_str, df, return_type="dataframe",
                                   NA_action="drop")

    y_vec = y_design.iloc[:, 0]
    n_used = len(y_vec)
    n_excluded_missing = n_total - n_used
    n_events = int(y_vec.sum())
    n_nonevents = n_used - n_events

    # Fit GLM logistic
    model = sm.GLM(y_vec, X, family=Binomial())
    result = model.fit(maxiter=100)

    converged = result.converged
    iterations = getattr(result, 'fit_history', {}).get('iteration', 0)
    if isinstance(iterations, list):
        iterations = len(iterations)
    log_likelihood = float(result.llf)

    # Null model for pseudo R²
    null_model = sm.GLM(y_vec, np.ones(n_used), family=Binomial())
    null_result = null_model.fit()
    null_ll = float(null_result.llf)

    # Nagelkerke pseudo R²
    n = n_used
    lr = 2 * (log_likelihood - null_ll)
    pseudo_r2 = (1 - np.exp(-lr / n)) / (1 - np.exp(2 * null_ll / n))

    # AIC / BIC
    aic = float(result.aic)
    # statsmodels GLM doesn't have .bic directly; compute manually
    k = len(result.params)
    bic = -2 * log_likelihood + k * np.log(n)

    # C-statistic (concordance)
    from sklearn.metrics import roc_auc_score
    try:
        pred_probs = result.predict(X)
        c_stat = float(roc_auc_score(y_vec, pred_probs))
    except Exception:
        c_stat = None

    # Build coefficients
    conf = result.conf_int(alpha=alpha)
    coefficients = []
    for i, term in enumerate(X.columns):
        beta = float(result.params.iloc[i])
        se = float(result.bse.iloc[i])
        ci_lo = float(conf.iloc[i, 0])
        ci_hi = float(conf.iloc[i, 1])
        pval = float(result.pvalues.iloc[i])
        odds_ratio = float(np.exp(beta))

        # Parse variable/level from patsy term names like "C(smoke)[T.current]"
        variable, level, reference = _parse_term(term)

        coefficients.append({
            "term": term,
            "variable": variable,
            "level": level,
            "reference": reference,
            "beta": beta,
            "standard_error": se,
            "odds_ratio": odds_ratio,
            "ci_lower": float(np.exp(ci_lo)),
            "ci_upper": float(np.exp(ci_hi)),
            "p_value": pval,
        })

    warnings_list = []
    if not converged:
        warnings_list.append("Model did not converge")
    if n_events < 10 * len(predictors):
        warnings_list.append(
            f"Events per variable ({n_events / max(len(predictors), 1):.1f}) "
            f"is below recommended minimum of 10"
        )

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
        "n_events": n_events,
        "n_nonevents": n_nonevents,
        "iterations": int(iterations),
        "converged": converged,
        "log_likelihood": log_likelihood,
        "null_log_likelihood": null_ll,
        "pseudo_r2_nagelkerke": float(pseudo_r2) if pseudo_r2 is not None else None,
        "aic": aic,
        "bic": float(bic),
        "c_statistic": c_stat,
        "coefficients": coefficients,
        "notes": [
            f"Fitted via Python statsmodels {PACKAGE_VERSION} GLM(Binomial())",
            f"CI level: {ci_level}",
        ],
        "warnings": warnings_list,
    }


def _parse_term(term: str):
    """Parse patsy term name into (variable, level, reference).

    Examples:
        "Intercept"          -> ("Intercept", None, None)
        "age"                -> ("age", None, None)
        "C(smoke)[T.current]" -> ("smoke", "current", "T")
    """
    import re
    m = re.match(r"C\((\w+)\)\[T\.(.+)\]", term)
    if m:
        return m.group(1), m.group(2), "T"
    if term == "Intercept":
        return "Intercept", None, None
    return term, None, None
