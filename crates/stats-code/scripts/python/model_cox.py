#!/usr/bin/env python3
"""
Stats Code Bridge — Cox Proportional Hazards via lifelines.

Outputs a result dict matching the CoxResult schema.
Requires: pip install lifelines
"""
import numpy as np
import pandas as pd

try:
    from lifelines import CoxPHFitter
    import lifelines
    PACKAGE_NAME = "lifelines"
    PACKAGE_VERSION = lifelines.__version__
except ImportError:
    PACKAGE_NAME = "lifelines"
    PACKAGE_VERSION = "not_installed"

    def CoxPHFitter(*a, **kw):
        raise ImportError(
            "lifelines is not installed. Run: pip install lifelines"
        )


def run(data_path: str, params: dict) -> dict:
    """Run Cox PH regression and return a CoxResult-compatible dict."""
    time_col = params["time"]
    event_col = params["event"]
    predictors = params.get("predictors", [])
    ci_level = params.get("ci_level", 0.95)
    alpha = 1.0 - ci_level

    df = pd.read_csv(data_path)
    n_total = len(df)

    # Prepare columns: expand categoricals manually for lifelines
    cols_needed = [time_col, event_col] + predictors
    sub = df[cols_needed].copy()

    # Identify and dummy-encode categorical columns
    cat_cols = []
    for pred in predictors:
        col = sub[pred]
        if col.dtype == object or col.nunique() <= 5:
            cat_cols.append(pred)

    if cat_cols:
        sub = pd.get_dummies(sub, columns=cat_cols, drop_first=True, dtype=float)

    # Drop rows with missing values
    sub_clean = sub.dropna()
    n_used = len(sub_clean)
    n_excluded_missing = n_total - n_used

    T = sub_clean[time_col]
    E = sub_clean[event_col]
    n_events = int(E.sum())
    n_censored = n_used - n_events
    tied_event_times = int(T[E == 1].duplicated().sum())

    # Fit Cox model
    cph = CoxPHFitter()
    predictor_cols = [c for c in sub_clean.columns if c not in (time_col, event_col)]
    cph.fit(sub_clean, duration_col=time_col, event_col=event_col)

    converged = cph.summary is not None
    iterations = 0  # lifelines doesn't expose iteration count easily
    log_partial_ll = float(cph.log_likelihood_)
    concordance = float(cph.concordance_index_)

    # Build formula string
    formula_str = f"Surv({time_col}, {event_col}) ~ " + " + ".join(predictor_cols)

    # Build coefficients
    summary = cph.summary
    coefficients = []
    for term in summary.index:
        row = summary.loc[term]
        beta = float(row["coef"])
        se = float(row["se(coef)"])
        hr = float(row["exp(coef)"])
        pval = float(row["p"])

        # CI columns vary by lifelines version
        ci_lo_key = f"coef lower {int(ci_level*100)}%"
        ci_hi_key = f"coef upper {int(ci_level*100)}%"
        if ci_lo_key in row:
            ci_lo = float(np.exp(row[ci_lo_key]))
            ci_hi = float(np.exp(row[ci_hi_key]))
        else:
            # Fallback: compute from beta ± z*se
            import scipy.stats
            z = scipy.stats.norm.ppf(1 - alpha / 2)
            ci_lo = float(np.exp(beta - z * se))
            ci_hi = float(np.exp(beta + z * se))

        variable, level, reference = _parse_dummy_term(term)

        coefficients.append({
            "term": term,
            "variable": variable,
            "level": level,
            "reference": reference,
            "beta": beta,
            "standard_error": se,
            "hazard_ratio": hr,
            "ci_lower": ci_lo,
            "ci_upper": ci_hi,
            "p_value": pval,
        })

    warnings_list = []
    if n_events < 10 * len(predictor_cols):
        warnings_list.append(
            f"Events per variable ({n_events / max(len(predictor_cols), 1):.1f}) "
            f"is below recommended minimum of 10"
        )

    return {
        "status": "ok",
        "data_path": data_path,
        "analysis_path": None,
        "formula": formula_str,
        "time": time_col,
        "event": event_col,
        "predictors": predictors,
        "n_total": n_total,
        "n_used": n_used,
        "n_excluded_missing": n_excluded_missing,
        "n_excluded_invalid": 0,
        "n_events": n_events,
        "n_censored": n_censored,
        "tied_event_times": tied_event_times,
        "iterations": iterations,
        "converged": converged,
        "log_partial_likelihood": log_partial_ll,
        "concordance": concordance,
        "coefficients": coefficients,
        "notes": [
            f"Fitted via Python lifelines {PACKAGE_VERSION} CoxPHFitter",
            f"CI level: {ci_level}",
        ],
        "warnings": warnings_list,
    }


def _parse_dummy_term(term: str):
    """Parse pandas get_dummies term name into (variable, level, reference).

    Examples:
        "age"           -> ("age", None, None)
        "smoke_current" -> ("smoke", "current", None)
    """
    # Try to detect dummy pattern: varname_level
    parts = term.rsplit("_", 1)
    if len(parts) == 2:
        return parts[0], parts[1], None
    return term, None, None
