#!/usr/bin/env python3
"""
gen_synthetic.py — Generate synthetic datasets for the Validation Correctness Framework.

Produces three CSV files in datasets/synthetic/:
  - small_n40.csv   (N=40)
  - medium_n200.csv (N=200)
  - large_n2000.csv (N=2000)

Each file contains columns:
  age, bmi, linear_y, disease, time, death, group

Random seed: 20260510 (fixed for reproducibility).
"""

from __future__ import annotations

import math
from pathlib import Path

import numpy as np
import pandas as pd

SEED = 20260510
OUT_DIR = Path(__file__).resolve().parents[1] / "synthetic"


def generate(n: int, rng: np.random.Generator) -> pd.DataFrame:
    """Generate a synthetic dataset with N rows."""
    age = rng.uniform(30, 80, size=n)
    bmi = rng.uniform(18, 40, size=n)

    # Linear outcome: y = 4 + 0.42*age + 0.85*bmi + noise
    noise_linear = rng.normal(0, 2.5, size=n)
    linear_y = 4.0 + 0.42 * age + 0.85 * bmi + noise_linear

    # Logistic outcome: logit(p) = -5.2 + 0.055*age + 0.075*bmi
    lp = -5.2 + 0.055 * age + 0.075 * bmi
    prob = 1.0 / (1.0 + np.exp(-lp))
    disease = rng.binomial(1, prob).astype(int)

    # Survival time: exponential with rate depending on age/bmi
    hazard = np.exp(-4.0 + 0.03 * age + 0.02 * bmi)
    time_raw = rng.exponential(1.0 / hazard)
    # Censoring at uniform random time
    censor_time = rng.uniform(2.0, 20.0, size=n)
    time = np.minimum(time_raw, censor_time)
    death = (time_raw <= censor_time).astype(int)

    # Group variable (binary, balanced)
    group = rng.integers(0, 2, size=n)

    df = pd.DataFrame({
        "age":      np.round(age, 2),
        "bmi":      np.round(bmi, 2),
        "linear_y": np.round(linear_y, 4),
        "disease":  disease,
        "time":     np.round(time, 4),
        "death":    death,
        "group":    group,
    })
    return df


def main() -> None:
    OUT_DIR.mkdir(parents=True, exist_ok=True)
    rng = np.random.default_rng(SEED)

    sizes = [
        ("small_n40.csv",   40),
        ("medium_n200.csv", 200),
        ("large_n2000.csv", 2000),
    ]

    for filename, n in sizes:
        df = generate(n, rng)
        out_path = OUT_DIR / filename
        df.to_csv(out_path, index=False)
        print(f"Written {out_path}  ({n} rows)")


if __name__ == "__main__":
    main()
