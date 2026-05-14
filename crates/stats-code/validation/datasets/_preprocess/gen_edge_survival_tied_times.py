#!/usr/bin/env python3
"""
gen_edge_survival_tied_times.py — Survival data with many tied event times.

N=50: multiple observations share the same event time, testing Breslow/Efron
tie-handling in Cox and Kaplan-Meier implementations.
"""

from pathlib import Path
import numpy as np
import pandas as pd

SEED = 20260510
OUT_DIR = Path(__file__).resolve().parents[1] / "edge_cases"


def main() -> None:
    OUT_DIR.mkdir(parents=True, exist_ok=True)
    rng = np.random.default_rng(SEED)

    n = 50
    age = rng.uniform(40, 75, size=n)
    bmi = rng.uniform(20, 35, size=n)

    # Force ties: only 8 distinct event times
    distinct_times = [2.0, 4.0, 6.0, 8.0, 10.0, 12.0, 15.0, 18.0]
    time_raw = rng.choice(distinct_times, size=n)
    # ~60% events, 40% censored
    death = rng.binomial(1, 0.6, size=n)

    df = pd.DataFrame({
        "age":   np.round(age, 2),
        "bmi":   np.round(bmi, 2),
        "time":  time_raw.astype(float),
        "death": death,
    })
    out_path = OUT_DIR / "survival_tied_times.csv"
    df.to_csv(out_path, index=False)
    print(f"Written {out_path}  ({n} rows, {len(distinct_times)} distinct times)")


if __name__ == "__main__":
    main()
