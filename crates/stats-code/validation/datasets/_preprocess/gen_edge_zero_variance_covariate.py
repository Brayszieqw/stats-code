#!/usr/bin/env python3
"""
gen_edge_zero_variance_covariate.py — Dataset with a constant (zero-variance) covariate.

N=100: one column is constant, making the design matrix singular.
Tests that Stats Code handles rank-deficient matrices gracefully.
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
    age = rng.uniform(30, 80, size=n)
    bmi = rng.uniform(18, 40, size=n)
    constant_col = np.ones(n) * 5.0  # zero variance

    lp = -5.0 + 0.05 * age + 0.07 * bmi
    prob = 1.0 / (1.0 + np.exp(-lp))
    disease = rng.binomial(1, prob).astype(int)

    linear_y = 3.0 + 0.4 * age + 0.8 * bmi + rng.normal(0, 2.0, size=n)

    df = pd.DataFrame({
        "age":          np.round(age, 2),
        "bmi":          np.round(bmi, 2),
        "constant_col": constant_col,
        "disease":      disease,
        "linear_y":     np.round(linear_y, 4),
    })
    out_path = OUT_DIR / "zero_variance_covariate.csv"
    df.to_csv(out_path, index=False)
    print(f"Written {out_path}  ({n} rows)")


if __name__ == "__main__":
    main()
