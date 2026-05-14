#!/usr/bin/env python3
"""
gen_edge_logistic_perfect_separation.py — Perfect separation edge case for logistic regression.

N=20: x perfectly predicts y (y=1 iff x > 0), causing logistic regression
to diverge (infinite coefficients). Tests that Stats Code handles this gracefully.
"""

from pathlib import Path
import numpy as np
import pandas as pd

SEED = 20260510
OUT_DIR = Path(__file__).resolve().parents[1] / "edge_cases"


def main() -> None:
    OUT_DIR.mkdir(parents=True, exist_ok=True)
    rng = np.random.default_rng(SEED)

    n = 20
    # x in [-2, -0.1] for y=0, x in [0.1, 2] for y=1 — perfect separation
    x_neg = rng.uniform(-2.0, -0.1, size=n // 2)
    x_pos = rng.uniform(0.1, 2.0, size=n // 2)
    x = np.concatenate([x_neg, x_pos])
    y = np.concatenate([np.zeros(n // 2, dtype=int), np.ones(n // 2, dtype=int)])

    # Shuffle
    idx = rng.permutation(n)
    df = pd.DataFrame({"x": np.round(x[idx], 4), "y": y[idx]})
    out_path = OUT_DIR / "logistic_perfect_separation.csv"
    df.to_csv(out_path, index=False)
    print(f"Written {out_path}  ({n} rows)")


if __name__ == "__main__":
    main()
