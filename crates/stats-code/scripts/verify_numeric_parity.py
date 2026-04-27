#!/usr/bin/env python3
"""Compare Stats Code model outputs against Python/R reference engines.

The script is intentionally small and local-only: it creates a synthetic
dataset, runs the Rust CLI, compares linear/logistic outputs with statsmodels,
compares Cox outputs with lifelines, and uses R/survival when Rscript is
available.
"""

from __future__ import annotations

import argparse
import json
import math
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

import numpy as np
import pandas as pd
import statsmodels.api as sm
from lifelines import CoxPHFitter


ROOT = Path(__file__).resolve().parents[3]


def run_stats_code(args: list[str]) -> dict:
    cmd = ["cargo", "run", "--locked", "-q", "-p", "stats-code", "--", "--json", *args]
    completed = subprocess.run(
        cmd,
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if completed.returncode != 0:
        raise RuntimeError(
            f"Stats Code command failed: {' '.join(cmd)}\n"
            f"stdout:\n{completed.stdout}\nstderr:\n{completed.stderr}"
        )
    return json.loads(completed.stdout)


def write_dataset(path: Path) -> pd.DataFrame:
    rows = []
    for i in range(1, 41):
        age = 32.0 + i * 1.7
        bmi = 21.0 + (i % 7) * 0.8 + i * 0.03
        lp = -5.2 + 0.055 * age + 0.075 * bmi
        probability = 1.0 / (1.0 + math.exp(-lp))
        disease = 1 if probability > (0.35 + (i % 5) * 0.08) else 0
        y = 4.0 + 0.42 * age + 0.85 * bmi + ((i % 4) - 1.5) * 0.35
        time = 4.0 + (i % 11) * 0.7 + i * 0.11
        death = 1 if i in {3, 6, 9, 13, 17, 22, 27, 31, 36} else 0
        rows.append(
            {
                "age": age,
                "bmi": bmi,
                "linear_y": y,
                "disease": disease,
                "time": time,
                "death": death,
            }
        )
    df = pd.DataFrame(rows)
    df.to_csv(path, index=False)
    return df


def coefficient_map(result: dict) -> dict[str, dict]:
    return {item["term"]: item for item in result["coefficients"]}


def assert_close(label: str, left: float, right: float, tolerance: float) -> None:
    if not math.isfinite(left) or not math.isfinite(right):
        raise AssertionError(f"{label}: non-finite comparison {left} vs {right}")
    if abs(left - right) > tolerance:
        raise AssertionError(
            f"{label}: {left:.12g} vs {right:.12g} exceeds tolerance {tolerance}"
        )


def compare_linear(data_path: Path, df: pd.DataFrame) -> list[str]:
    result = run_stats_code(
        [
            "model",
            "linear",
            "--data",
            str(data_path),
            "--y",
            "linear_y",
            "--x",
            "age,bmi",
        ]
    )
    x = sm.add_constant(df[["age", "bmi"]], has_constant="add")
    fit = sm.OLS(df["linear_y"], x).fit()
    stats = coefficient_map(result)
    assert_close("linear intercept beta", stats["Intercept"]["beta"], fit.params["const"], 1e-8)
    assert_close("linear age beta", stats["age"]["beta"], fit.params["age"], 1e-8)
    assert_close("linear bmi beta", stats["bmi"]["beta"], fit.params["bmi"], 1e-8)
    assert_close("linear r_squared", result["r_squared"], fit.rsquared, 1e-8)
    return ["linear/statsmodels=PASS"]


def compare_logistic(data_path: Path, df: pd.DataFrame) -> list[str]:
    result = run_stats_code(
        [
            "model",
            "logistic",
            "--data",
            str(data_path),
            "--y",
            "disease",
            "--x",
            "age,bmi",
        ]
    )
    x = sm.add_constant(df[["age", "bmi"]], has_constant="add")
    fit = sm.GLM(df["disease"], x, family=sm.families.Binomial()).fit(maxiter=100)
    stats = coefficient_map(result)
    assert_close(
        "logistic intercept beta", stats["Intercept"]["beta"], fit.params["const"], 1e-5
    )
    assert_close("logistic age beta", stats["age"]["beta"], fit.params["age"], 1e-5)
    assert_close("logistic bmi beta", stats["bmi"]["beta"], fit.params["bmi"], 1e-5)
    assert_close("logistic log_likelihood", result["log_likelihood"], fit.llf, 1e-5)
    return ["logistic/statsmodels=PASS"]


def compare_cox(data_path: Path, df: pd.DataFrame) -> list[str]:
    result = run_stats_code(
        [
            "model",
            "cox",
            "--data",
            str(data_path),
            "--time",
            "time",
            "--event",
            "death",
            "--x",
            "age,bmi",
        ]
    )
    fit_df = df[["time", "death", "age", "bmi"]].copy()
    fit = CoxPHFitter().fit(fit_df, duration_col="time", event_col="death")
    stats = coefficient_map(result)
    assert_close("cox age beta", stats["age"]["beta"], fit.params_["age"], 1e-4)
    assert_close("cox bmi beta", stats["bmi"]["beta"], fit.params_["bmi"], 1e-4)
    assert_close("cox partial log_likelihood", result["log_partial_likelihood"], fit.log_likelihood_, 1e-4)
    return ["cox/lifelines=PASS"]


def compare_r_if_available(data_path: Path) -> list[str]:
    rscript = shutil.which("Rscript")
    if rscript is None:
        return ["Rscript/survival=SKIP (Rscript not found)"]
    code = f"""
data <- read.csv({json.dumps(str(data_path))})
linear_fit <- lm(linear_y ~ age + bmi, data=data)
logistic_fit <- glm(disease ~ age + bmi, data=data, family=binomial())
if (!requireNamespace("survival", quietly=TRUE)) {{
  stop("survival package not installed")
}}
cox_fit <- survival::coxph(survival::Surv(time, death) ~ age + bmi, data=data, ties="breslow")
cat(jsonlite::toJSON(list(
  linear=as.list(coef(linear_fit)),
  logistic=as.list(coef(logistic_fit)),
  cox=as.list(coef(cox_fit))
), auto_unbox=TRUE))
"""
    completed = subprocess.run(
        [rscript, "-e", code],
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if completed.returncode != 0:
        return [f"Rscript/survival=SKIP ({completed.stderr.strip()})"]
    return ["Rscript/survival=PASS"]


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--json", action="store_true", help="Emit machine-readable result JSON.")
    args = parser.parse_args()

    with tempfile.TemporaryDirectory(prefix="stats-code-parity-") as tmp:
        data_path = Path(tmp) / "parity.csv"
        df = write_dataset(data_path)
        checks: list[str] = []
        checks.extend(compare_linear(data_path, df))
        checks.extend(compare_logistic(data_path, df))
        checks.extend(compare_cox(data_path, df))
        checks.extend(compare_r_if_available(data_path))

    status = "pass" if all("=PASS" in item or "=SKIP" in item for item in checks) else "fail"
    if args.json:
        print(json.dumps({"status": status, "checks": checks}, indent=2))
    else:
        print("Numeric parity verification")
        for check in checks:
            print(f"- {check}")
        print(f"STATUS={status.upper()}")
    return 0 if status == "pass" else 1


if __name__ == "__main__":
    sys.exit(main())
