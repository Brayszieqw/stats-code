#!/usr/bin/env python3
"""
gen_edge_collinear_predictors.py — Dataset with near-perfectly collinear predictors.

N=100: x2 = 2*x1 + small_noise, making the condition number of X'X extremely high.
Tests numerical stability of linear/logistic regression under near-multicollinearity.
"""

from pathlib import Path
import numpy as np
import pandas as pd

SEED = 20260510
OUT_DIR = Path(__file__).resolve().parents[1] / "edge_cases"


def main() -> None:
    OUT_DIR.mkdir(parents=True, exist_ok=True)
    rng = np.random.default_rng(SEED)

    n = 100
    x1 = rng.uniform(1.0, 10.0, size=n)
    # x2 ≈ 2*x1 with tiny noise → condition number >> 1000
    x2 = 2.0 * x1 + rng.normal(0, 0.01, size=n)

    linear_y = 1.0 + 0.5 * x1 + 0.3 * x2 + rng.normal(0, 1.0, size=n)

    lp = -3.0 + 0.2 * x1 + 0.1 * x2
    prob = 1.0 / (1.0 + np.exp(-lp))
    disease = rng.binomial(1, prob).astype(int)

    df = pd.DataFrame({
        "x1":       np.round(x1, 4),
        "x2":       np.round(x2, 4),
        "linear_y": np.round(linear_y, 4),
        "disease":  disease,
    })
    out_path = OUT_DIR / "collinear_predictors.csv"
    df.to_csv(out_path, index=False)

    # Report condition number
    X = np.column_stack([np.ones(n), x1, x2])
    cond = np.linalg.cond(X.T @ X)
    print(f"Written {out_path}  ({n} rows, condition number ≈ {cond:.1e})")


if __name__ == "__main__":
    main()
