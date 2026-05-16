#!/usr/bin/env python3
"""Linear discriminant analysis bridge via scikit-learn."""

import numpy as np
import pandas as pd
from scipy import stats
from sklearn.discriminant_analysis import LinearDiscriminantAnalysis
from sklearn.metrics import confusion_matrix
from sklearn.model_selection import LeaveOneOut, cross_val_predict

try:
    import sklearn

    PACKAGE_NAME = "sklearn"
    PACKAGE_VERSION = sklearn.__version__
except Exception:
    PACKAGE_NAME = "sklearn"
    PACKAGE_VERSION = "unknown"


def run(data_path: str, params: dict) -> dict:
    group = params["group"]
    variables = params.get("vars", params.get("variables", []))
    df = pd.read_csv(data_path)
    n_total = len(df)
    model_df = df[[group] + variables].dropna().copy()
    n_used = len(model_df)
    if n_used == 0:
        raise ValueError("No complete rows remain for LDA")
    y = model_df[group].astype(str).to_numpy()
    groups = sorted(pd.unique(y).tolist())
    x = model_df[variables].astype(float).to_numpy()
    p = x.shape[1]
    for label in groups:
        if int(np.sum(y == label)) <= p:
            raise ValueError(
                f"LDA requires each group to have more observations than predictors; group `{label}` has {int(np.sum(y == label))}, predictors={p}"
            )

    lda = LinearDiscriminantAnalysis()
    lda.fit(x, y)
    loo_pred = cross_val_predict(LinearDiscriminantAnalysis(), x, y, cv=LeaveOneOut())
    matrix = confusion_matrix(y, loo_pred, labels=groups)
    correct_rate_per_group = []
    for i, label in enumerate(groups):
        denom = matrix[i, :].sum()
        correct_rate_per_group.append(float(matrix[i, i] / denom) if denom else 0.0)
    overall = float(np.trace(matrix) / matrix.sum()) if matrix.sum() else 0.0

    wilks_lambda, wilks_chi, wilks_p = _wilks_lambda(x, y, groups)
    sd = np.std(x, axis=0, ddof=1)
    coeff = np.atleast_2d(lda.scalings_[:, : max(1, len(groups) - 1)].T)
    standardized = coeff * sd

    return {
        "status": "ok",
        "data_path": data_path,
        "analysis_path": None,
        "n_total": int(n_total),
        "n_used": int(n_used),
        "n_excluded_missing": int(n_total - n_used),
        "notes": [f"Fitted via Python scikit-learn {PACKAGE_VERSION} LinearDiscriminantAnalysis"],
        "warnings": [],
        "group": group,
        "groups": groups,
        "variables": variables,
        "wilks_lambda": float(wilks_lambda),
        "wilks_chi_square": float(wilks_chi),
        "wilks_p": float(wilks_p),
        "function_coefficients": coeff.tolist(),
        "standardized_coefficients": standardized.tolist(),
        "centroids": lda.means_.tolist(),
        "confusion_matrix": matrix.astype(int).tolist(),
        "correct_rate_per_group": correct_rate_per_group,
        "overall_correct_rate": overall,
    }


def _wilks_lambda(x: np.ndarray, y: np.ndarray, groups: list[str]):
    overall = x.mean(axis=0)
    total = (x - overall).T @ (x - overall)
    within = np.zeros_like(total)
    for label in groups:
        subset = x[y == label]
        center = subset.mean(axis=0)
        within += (subset - center).T @ (subset - center)
    lam = float(max(np.linalg.det(within), 1e-12) / max(np.linalg.det(total), 1e-12))
    n, p = x.shape
    g = len(groups)
    dfree = p * (g - 1)
    chi = float(-(n - 1 - (p + g) / 2.0) * np.log(max(lam, 1e-12)))
    return lam, chi, float(1.0 - stats.chi2.cdf(chi, dfree))
