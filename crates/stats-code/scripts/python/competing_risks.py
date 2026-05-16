#!/usr/bin/env python3
"""Competing-risks bridge: cause-specific Cox plus Aalen-Johansen CIF."""

import math

import numpy as np
import pandas as pd
from lifelines import AalenJohansenFitter, CoxPHFitter

try:
    import lifelines

    PACKAGE_NAME = "lifelines"
    PACKAGE_VERSION = lifelines.__version__
except Exception:
    PACKAGE_NAME = "lifelines"
    PACKAGE_VERSION = "unknown"


def run(data_path: str, params: dict) -> dict:
    time = params["time"]
    event_type = params["event_type"]
    cause = str(params.get("cause", ""))
    predictors = params.get("predictors", params.get("x", []))
    alpha = 1.0 - params.get("ci_level", 0.95)
    df = pd.read_csv(data_path)
    n_total = len(df)
    model_df = df[[time, event_type] + predictors].dropna().copy()
    model_df[event_type] = model_df[event_type].astype(str)
    causes = sorted([value for value in model_df[event_type].unique().tolist() if value not in ("0", "False", "false", "censored")])
    if len(causes) < 2:
        raise ValueError("Competing-risks analysis requires at least two observed event types; use standard Cox regression instead")
    if cause and cause not in causes:
        raise ValueError(f"Cause `{cause}` was not found in event type column `{event_type}`")

    x = _design_matrix(model_df, predictors)
    working = pd.concat([model_df[[time, event_type]].reset_index(drop=True), x.reset_index(drop=True)], axis=1)
    cause_fits = []
    for label in causes:
        cox_df = working.copy()
        cox_df["_event"] = (cox_df[event_type] == label).astype(int)
        fit_cols = [time, "_event"] + list(x.columns)
        cph = CoxPHFitter(alpha=alpha)
        cph.fit(cox_df[fit_cols], duration_col=time, event_col="_event")
        coefficients = []
        for term, row in cph.summary.iterrows():
            beta = float(row["coef"])
            se = _finite(float(row["se(coef)"]))
            lower = row.get("exp(coef) lower 95%", None)
            upper = row.get("exp(coef) upper 95%", None)
            coefficients.append(
                {
                    "term": str(term),
                    "variable": str(term),
                    "level": None,
                    "reference": None,
                    "beta": _finite(beta),
                    "standard_error": se,
                    "hazard_ratio": _safe_exp(beta),
                    "ci_lower": _finite(float(lower)) if lower is not None else _safe_exp(beta - 1.96 * se),
                    "ci_upper": _finite(float(upper), 1e99) if upper is not None else _safe_exp(beta + 1.96 * se),
                    "p_value": _finite(float(row["p"]), 1.0),
                }
            )
        cause_fits.append(
            {
                "cause": str(label),
                "coefficients": coefficients,
                "log_partial_likelihood": float(cph.log_likelihood_),
                "n_events": int((working[event_type] == label).sum()),
            }
        )

    encoded_events = model_df[event_type].map({label: i + 1 for i, label in enumerate(causes)}).fillna(0).astype(int)
    cif_curves = {}
    for label in causes:
        event_code = causes.index(label) + 1
        aj = AalenJohansenFitter()
        aj.fit(model_df[time].astype(float), encoded_events, event_of_interest=event_code)
        frame = aj.cumulative_density_.reset_index()
        time_col = frame.columns[0]
        cif_col = frame.columns[1]
        cif_curves[str(label)] = [
            {"time": float(row[time_col]), "cif": float(row[cif_col]), "se": 0.0}
            for _, row in frame.iterrows()
        ]

    return {
        "status": "ok",
        "data_path": data_path,
        "analysis_path": None,
        "n_total": int(n_total),
        "n_used": int(len(model_df)),
        "n_excluded_missing": int(n_total - len(model_df)),
        "notes": [f"Fitted via Python lifelines {PACKAGE_VERSION}; Gray test is not available in lifelines"],
        "warnings": ["Gray's test is returned as null; CIF point estimates and cause-specific Cox models are available"],
        "time": time,
        "event_type": event_type,
        "causes": [str(c) for c in causes],
        "cause_fits": cause_fits,
        "cif_curves": cif_curves,
        "gray_chi_square": None,
        "gray_df": None,
        "gray_p": None,
    }


def _design_matrix(df: pd.DataFrame, predictors: list[str]) -> pd.DataFrame:
    if not predictors:
        return pd.DataFrame(index=df.index)
    x = df[predictors].copy()
    categorical = [name for name in predictors if not pd.api.types.is_numeric_dtype(x[name])]
    if categorical:
        x = pd.get_dummies(x, columns=categorical, drop_first=True, dtype=float)
    return x.astype(float)


def _safe_exp(value: float) -> float:
    if not math.isfinite(value):
        return 1e99 if value > 0 else 0.0
    if value > 709.0:
        return 1e99
    if value < -745.0:
        return 0.0
    return float(math.exp(value))


def _finite(value: float, default: float = 0.0) -> float:
    return float(value) if math.isfinite(value) else default
