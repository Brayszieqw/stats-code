#!/usr/bin/env python3
"""
gen_edge_single_obs_group.py — Dataset where one group has only a single observation.

N=30: the 'group' column has one category with only 1 record.
Tests Table One and group-comparison methods with degenerate group sizes.
"""

from pathlib import Path
import numpy as np
import pandas as pd

SEED = 20260510
OUT_DIR = Path(__file__).resolve().parents[1] / "edge_cases"


def main() -> None:
    OUT_DIR.mkdir(parents=True, exist_ok=True)
    rng = np.random.default_rng(SEED)

    n = 30
    age = rng.uniform(30, 80, size=n)
    bmi = rng.uniform(18, 40, size=n)
    linear_y = 3.0 + 0.4 * age + 0.8 * bmi + rng.normal(0, 2.0, size=n)

    # group: 1 obs in group "C", rest split between "A" and "B"
    groups = ["A"] * 15 + ["B"] * 14 + ["C"] * 1
    rng.shuffle(groups)

    df = pd.DataFrame({
        "age":      np.round(age, 2),
        "bmi":      np.round(bmi, 2),
        "linear_y": np.round(linear_y, 4),
        "group":    groups,
    })
    out_path = OUT_DIR / "single_obs_group.csv"
    df.to_csv(out_path, index=False)
    counts = df["group"].value_counts().to_dict()
    print(f"Written {out_path}  ({n} rows, group counts: {counts})")


if __name__ == "__main__":
    main()
