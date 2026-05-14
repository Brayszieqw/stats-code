"""
parity/diagnostic_roc.py — Numerical parity module for diagnostic ROC analysis.

Stats Code CLI: ``diagnostic roc --data ... --truth ... --score ... [--threshold ...]``

JSON output fields used:
  - auc                              : area under the ROC curve
  - threshold_metrics.sensitivity    : sensitivity at requested threshold
  - threshold_metrics.specificity    : specificity at requested threshold
  - youden.sensitivity               : sensitivity at Youden's J threshold
  - youden.specificity               : specificity at Youden's J threshold

This module requires a dataset with a binary truth column and a continuous score
column. The synthetic datasets don't have a pre-computed score, so we generate
one on-the-fly from logistic regression predictions.
"""

from __future__ import annotations

import tempfile
from pathlib import Path
from typing import Any

import numpy as np
import pandas as pd

from .adapters import ReferenceAdapter
from .common import StatsCodeInvocationError, compare_scalar, run_stats_code
from .result import Status, ToleranceConfig, ValidationResult

METHOD = "diagnostic_roc"
METRICS = ["auc", "sensitivity", "specificity"]

_DEFAULT_SPEC: dict[str, Any] = {
    "truth_col": "disease",
    "score_col": "score",   # generated from logistic predictions
    "threshold": 0.5,
}


def _add_score_column(dataset_path: Path, truth_col: str, score_col: str) -> Path:
    """
    Add a logistic-regression-derived score column to the dataset.

    Returns a path to a temporary CSV with the score column added.
    The temp file is written to the system temp directory.
    """
    import statsmodels.api as sm

    df = pd.read_csv(dataset_path)
    if score_col in df.columns:
        return dataset_path  # already has score column

    # Use age and bmi as predictors for the score
    feature_cols = [c for c in ["age", "bmi", "x1", "x2"] if c in df.columns]
    if not feature_cols or truth_col not in df.columns:
        raise ValueError(
            f"Cannot generate score: need '{truth_col}' and at least one of {feature_cols}"
        )

    X = sm.add_constant(df[feature_cols])
    y = df[truth_col]
    try:
        model = sm.GLM(y, X, family=sm.families.Binomial()).fit(maxiter=100, disp=False)
        df[score_col] = model.predict(X)
    except Exception:
        # Fallback: use a simple linear combination as score
        df[score_col] = df[feature_cols].mean(axis=1)
        df[score_col] = (df[score_col] - df[score_col].min()) / (
            df[score_col].max() - df[score_col].min() + 1e-10
        )

    # Write to temp file
    tmp = tempfile.NamedTemporaryFile(
        suffix=".csv", prefix="roc_", delete=False, mode="w"
    )
    df.to_csv(tmp.name, index=False)
    tmp.close()
    return Path(tmp.name)


def collect(
    dataset_path: Path,
    tol_config: ToleranceConfig,
    adapters: list[ReferenceAdapter],
    spec: dict[str, Any] | None = None,
) -> list[ValidationResult]:
    """Run diagnostic ROC parity checks for *dataset_path*."""
    if spec is None:
        spec = _DEFAULT_SPEC

    dataset_label = dataset_path.name
    results: list[ValidationResult] = []

    truth_col = spec["truth_col"]
    score_col = spec["score_col"]
    threshold = float(spec.get("threshold", 0.5))

    # ── Prepare dataset with score column ────────────────────────────────────
    tmp_path: Path | None = None
    try:
        enriched_path = _add_score_column(dataset_path, truth_col, score_col)
        if enriched_path != dataset_path:
            tmp_path = enriched_path
    except Exception as exc:
        return [ValidationResult(
            method=METHOD, dataset=dataset_label,
            reference_engine="stats_code_cli", metric="__setup__",
            tolerance=0.0, status=Status.ERROR,
            message=f"Failed to add score column: {exc}",
        )]

    # ── 1. Call Stats Code CLI ───────────────────────────────────────────────
    try:
        sc_out = run_stats_code([
            "--json", "diagnostic", "roc",
            "--data", str(enriched_path.resolve()),
            "--truth", truth_col,
            "--score", score_col,
            "--threshold", str(threshold),
        ])
    except StatsCodeInvocationError as exc:
        results.append(ValidationResult(
            method=METHOD, dataset=dataset_label,
            reference_engine="stats_code_cli", metric="__invoke__",
            tolerance=0.0, status=Status.ERROR, message=str(exc),
        ))
        return results
    finally:
        if tmp_path and tmp_path.exists():
            tmp_path.unlink(missing_ok=True)

    sc_auc = float(sc_out.get("auc", float("nan")))
    thr_metrics = sc_out.get("threshold_metrics") or sc_out.get("youden", {})
    sc_sensitivity = float(thr_metrics.get("sensitivity", float("nan")))
    sc_specificity = float(thr_metrics.get("specificity", float("nan")))

    # ── 2. Compare against each adapter ─────────────────────────────────────
    # Re-read the enriched dataset for adapter comparison
    try:
        df_enriched = pd.read_csv(enriched_path if enriched_path.exists() else dataset_path)
    except Exception:
        df_enriched = pd.read_csv(dataset_path)

    adapter_spec = {
        "label_col": truth_col,
        "score_col": score_col,
        "threshold": threshold,
    }

    for adapter in adapters:
        if not adapter.is_available():
            for metric in METRICS:
                results.append(ValidationResult(
                    method=METHOD, dataset=dataset_label,
                    reference_engine=adapter.name, metric=metric,
                    tolerance=tol_config.lookup(METHOD, metric),
                    status=Status.SKIP, message=f"{adapter.name} unavailable",
                ))
            continue

        # Write enriched CSV for adapter
        with tempfile.NamedTemporaryFile(
            suffix=".csv", prefix="roc_adapter_", delete=False, mode="w"
        ) as f:
            adapter_csv = Path(f.name)
        df_enriched.to_csv(adapter_csv, index=False)

        try:
            ref = adapter.fit(METHOD, adapter_csv, adapter_spec)
        except Exception as exc:
            results.append(ValidationResult(
                method=METHOD, dataset=dataset_label,
                reference_engine=adapter.name, metric="__fit__",
                tolerance=0.0, status=Status.ERROR,
                message=f"adapter.fit() raised: {exc}",
            ))
            continue
        finally:
            adapter_csv.unlink(missing_ok=True)

        # AUC
        results.append(compare_scalar(
            METHOD, "auc", dataset_label,
            adapter.name, ref.get("auc", float("nan")), sc_auc, tol_config,
        ))

        # Sensitivity and specificity at threshold
        sens_key = f"sensitivity@{threshold}"
        spec_key = f"specificity@{threshold}"
        if sens_key in ref:
            results.append(compare_scalar(
                METHOD, f"sensitivity@{threshold}", dataset_label,
                adapter.name, float(ref[sens_key]), sc_sensitivity, tol_config,
            ))
        if spec_key in ref:
            results.append(compare_scalar(
                METHOD, f"specificity@{threshold}", dataset_label,
                adapter.name, float(ref[spec_key]), sc_specificity, tol_config,
            ))

    return results
