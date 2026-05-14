"""
parity/adapters.py — Reference engine adapters for the Validation Correctness Framework.

Each adapter wraps one reference engine and exposes a uniform interface:
  - is_available() → bool
  - fit(method, dataset_path, spec) → dict[str, float]

Adapters are registered in ADAPTERS_FOR: dict[str, list[ReferenceAdapter]].

Availability is checked lazily so that missing optional engines (R, known-value
files) degrade gracefully to SKIP rather than ERROR.
"""

from __future__ import annotations

import json
import shutil
import subprocess
from abc import ABC, abstractmethod
from pathlib import Path
from typing import Any

import numpy as np
import pandas as pd


# ---------------------------------------------------------------------------
# Abstract base
# ---------------------------------------------------------------------------

class ReferenceAdapter(ABC):
    """Abstract reference engine adapter."""

    name: str  # "statsmodels" | "lifelines" | "scipy" | "sklearn" | ...

    @property
    def is_python(self) -> bool:
        """True for pure-Python adapters (statsmodels, lifelines, scipy, sklearn)."""
        return True

    @abstractmethod
    def is_available(self) -> bool:
        """Return True if this adapter can be used in the current environment."""
        ...

    @abstractmethod
    def fit(
        self,
        method: str,
        dataset_path: Path,
        spec: dict[str, Any],
    ) -> dict[str, float]:
        """
        Fit the reference model and return metric → value mapping.

        Parameters
        ----------
        method:       Stats Code method name, e.g. ``"linear"``.
        dataset_path: Absolute path to the CSV dataset.
        spec:         Method-specific configuration (covariates, outcome, etc.).

        Returns
        -------
        dict[str, float]
            Metric names aligned with Stats Code JSON output keys.
        """
        ...


# ---------------------------------------------------------------------------
# StatsmodelsAdapter
# ---------------------------------------------------------------------------

class StatsmodelsAdapter(ReferenceAdapter):
    """
    Reference adapter using statsmodels.

    Covers: linear, logistic, power, math_core.
    """

    name = "statsmodels"

    def is_available(self) -> bool:
        try:
            import statsmodels  # noqa: F401
            return True
        except ImportError:
            return False

    def fit(
        self,
        method: str,
        dataset_path: Path,
        spec: dict[str, Any],
    ) -> dict[str, float]:
        if method == "linear":
            return self._fit_linear(dataset_path, spec)
        if method == "logistic":
            return self._fit_logistic(dataset_path, spec)
        if method == "power":
            return self._fit_power(spec)
        if method == "math_core":
            return self._fit_math_core(spec)
        raise NotImplementedError(f"StatsmodelsAdapter does not cover method '{method}'")

    # -- linear ---------------------------------------------------------------

    def _fit_linear(self, dataset_path: Path, spec: dict[str, Any]) -> dict[str, float]:
        import statsmodels.api as sm

        df = pd.read_csv(dataset_path)
        outcome = spec["outcome"]
        covariates = spec["covariates"]

        X = sm.add_constant(df[covariates])
        y = df[outcome]
        model = sm.OLS(y, X).fit()

        results: dict[str, float] = {}
        for cov in covariates:
            results[f"beta[{cov}]"] = float(model.params[cov])
            results[f"stderr[{cov}]"] = float(model.bse[cov])
            results[f"t_stat[{cov}]"] = float(model.tvalues[cov])
            results[f"pvalue[{cov}]"] = float(model.pvalues[cov])
        # intercept
        results["beta[const]"] = float(model.params["const"])
        results["stderr[const]"] = float(model.bse["const"])
        results["t_stat[const]"] = float(model.tvalues["const"])
        results["pvalue[const]"] = float(model.pvalues["const"])

        results["r_squared"] = float(model.rsquared)
        results["adj_r_squared"] = float(model.rsquared_adj)
        results["f_stat"] = float(model.fvalue)
        results["f_pvalue"] = float(model.f_pvalue)
        return results

    # -- logistic -------------------------------------------------------------

    def _fit_logistic(self, dataset_path: Path, spec: dict[str, Any]) -> dict[str, float]:
        import statsmodels.api as sm
        from statsmodels.genmod.families import Binomial

        df = pd.read_csv(dataset_path)
        outcome = spec["outcome"]
        covariates = spec["covariates"]

        X = sm.add_constant(df[covariates])
        y = df[outcome]
        model = sm.GLM(y, X, family=Binomial()).fit()

        results: dict[str, float] = {}
        for cov in covariates:
            results[f"beta[{cov}]"] = float(model.params[cov])
            results[f"stderr[{cov}]"] = float(model.bse[cov])
            results[f"wald[{cov}]"] = float(model.tvalues[cov])
            results[f"pvalue[{cov}]"] = float(model.pvalues[cov])
            results[f"odds_ratio[{cov}]"] = float(np.exp(model.params[cov]))
        results["beta[const]"] = float(model.params["const"])
        results["stderr[const]"] = float(model.bse["const"])
        results["wald[const]"] = float(model.tvalues["const"])
        results["pvalue[const]"] = float(model.pvalues["const"])
        results["odds_ratio[const]"] = float(np.exp(model.params["const"]))

        results["log_likelihood"] = float(model.llf)
        # C-statistic (AUC) via sklearn
        try:
            from sklearn.metrics import roc_auc_score
            y_pred = model.predict(X)
            results["c_statistic"] = float(roc_auc_score(y, y_pred))
        except Exception:
            pass

        # Nagelkerke R²
        try:
            null_model = sm.GLM(y, np.ones(len(y)), family=Binomial()).fit()
            ll_full = model.llf
            ll_null = null_model.llf
            n = len(y)
            cox_snell = 1 - np.exp((2 / n) * (ll_null - ll_full))
            max_r2 = 1 - np.exp((2 / n) * ll_null)
            results["nagelkerke_r2"] = float(cox_snell / max_r2)
        except Exception:
            pass

        return results

    # -- power ----------------------------------------------------------------

    def _fit_power(self, spec: dict[str, Any]) -> dict[str, float]:
        from statsmodels.stats.power import (
            NormalIndPower,
            TTestIndPower,
            zt_ind_solve_power,
        )

        power_type = spec.get("power_type", "two_means")
        alpha = float(spec.get("alpha", 0.05))
        power = float(spec.get("power", 0.80))

        results: dict[str, float] = {}

        if power_type == "one_proportion":
            p0 = float(spec["p0"])
            p1 = float(spec["p1"])
            effect = abs(p1 - p0) / (p0 * (1 - p0)) ** 0.5
            analysis = NormalIndPower()
            n = analysis.solve_power(effect_size=effect, alpha=alpha, power=power)
            results["required_n"] = float(np.ceil(n))
            results["achieved_power"] = float(
                analysis.solve_power(effect_size=effect, alpha=alpha, nobs1=np.ceil(n))
            )

        elif power_type == "two_proportions":
            p1_val = float(spec["p1"])
            p2_val = float(spec["p2"])
            from statsmodels.stats.proportion import proportion_effectsize
            effect = proportion_effectsize(p1_val, p2_val)
            analysis = NormalIndPower()
            n = analysis.solve_power(effect_size=effect, alpha=alpha, power=power)
            results["required_n"] = float(np.ceil(n))
            results["achieved_power"] = float(
                analysis.solve_power(effect_size=effect, alpha=alpha, nobs1=np.ceil(n))
            )

        elif power_type == "two_means":
            mean_diff = float(spec["mean_diff"])
            std = float(spec["std"])
            effect = mean_diff / std
            analysis = TTestIndPower()
            n = analysis.solve_power(effect_size=effect, alpha=alpha, power=power)
            results["required_n"] = float(np.ceil(n))
            results["achieved_power"] = float(
                analysis.solve_power(effect_size=effect, alpha=alpha, nobs1=np.ceil(n))
            )

        return results

    # -- math_core ------------------------------------------------------------

    def _fit_math_core(self, spec: dict[str, Any]) -> dict[str, float]:
        # math_core uses scipy; statsmodels delegates to it
        return ScipyAdapter()._fit_math_core(spec)


# ---------------------------------------------------------------------------
# LifelinesAdapter
# ---------------------------------------------------------------------------

class LifelinesAdapter(ReferenceAdapter):
    """
    Reference adapter using lifelines.

    Covers: cox, km (Kaplan–Meier + log-rank).
    """

    name = "lifelines"

    def is_available(self) -> bool:
        try:
            import lifelines  # noqa: F401
            return True
        except ImportError:
            return False

    def fit(
        self,
        method: str,
        dataset_path: Path,
        spec: dict[str, Any],
    ) -> dict[str, float]:
        if method == "cox":
            return self._fit_cox(dataset_path, spec)
        if method in ("km", "survival"):
            return self._fit_km(dataset_path, spec)
        raise NotImplementedError(f"LifelinesAdapter does not cover method '{method}'")

    # -- cox ------------------------------------------------------------------

    def _fit_cox(self, dataset_path: Path, spec: dict[str, Any]) -> dict[str, float]:
        from lifelines import CoxPHFitter

        df = pd.read_csv(dataset_path)
        duration_col = spec["duration_col"]
        event_col = spec["event_col"]
        covariates = spec["covariates"]

        cph = CoxPHFitter()
        # Both lifelines and Stats Code default to the Efron tie handling
        # approximation, so log_partial_likelihood agrees within the standard
        # Newton-Raphson tolerance. See tolerance_config.yaml for cox.*.
        cph.fit(
            df[[duration_col, event_col] + covariates],
            duration_col=duration_col,
            event_col=event_col,
        )

        results: dict[str, float] = {}
        for cov in covariates:
            results[f"beta[{cov}]"] = float(cph.params_[cov])
            results[f"stderr[{cov}]"] = float(cph.standard_errors_[cov])
            results[f"hazard_ratio[{cov}]"] = float(np.exp(cph.params_[cov]))
            results[f"pvalue[{cov}]"] = float(cph.summary.loc[cov, "p"])

        results["log_partial_likelihood"] = float(cph.log_likelihood_)
        results["concordance"] = float(cph.concordance_index_)
        return results

    # -- km -------------------------------------------------------------------

    def _fit_km(self, dataset_path: Path, spec: dict[str, Any]) -> dict[str, float]:
        from lifelines import KaplanMeierFitter
        from lifelines.statistics import logrank_test

        df = pd.read_csv(dataset_path)
        duration_col = spec["duration_col"]
        event_col = spec["event_col"]

        kmf = KaplanMeierFitter()
        kmf.fit(df[duration_col], event_observed=df[event_col])

        results: dict[str, float] = {}
        # survival probabilities at each event time
        sf = kmf.survival_function_
        for i, (t, s) in enumerate(zip(sf.index, sf["KM_estimate"])):
            results[f"survival_probability[{i}]"] = float(s)
            results[f"event_time[{i}]"] = float(t)

        # Greenwood SE
        timeline = kmf.timeline
        for i, t in enumerate(timeline):
            se = float(kmf.conditional_time_to_event_.loc[t] if t in kmf.conditional_time_to_event_.index else float("nan"))
            # Use variance from survival function directly
        # Simpler: use kmf.confidence_interval_ to derive SE
        ci = kmf.confidence_interval_
        if ci is not None and len(ci) > 0:
            for i, t in enumerate(sf.index):
                if t in ci.index:
                    upper = float(ci.loc[t, "KM_estimate_upper_0.95"])
                    lower = float(ci.loc[t, "KM_estimate_lower_0.95"])
                    # Greenwood SE ≈ (upper - lower) / (2 * 1.96)
                    results[f"greenwood_se[{i}]"] = (upper - lower) / (2 * 1.96)

        results["median_survival"] = float(kmf.median_survival_time_)

        # log-rank test (requires group column)
        group_col = spec.get("group_col")
        if group_col and group_col in df.columns:
            groups = df[group_col].unique()
            if len(groups) == 2:
                g0, g1 = groups
                mask0 = df[group_col] == g0
                mask1 = df[group_col] == g1
                lr = logrank_test(
                    df.loc[mask0, duration_col], df.loc[mask1, duration_col],
                    event_observed_A=df.loc[mask0, event_col],
                    event_observed_B=df.loc[mask1, event_col],
                )
                results["logrank_chi2"] = float(lr.test_statistic)
                results["logrank_p"] = float(lr.p_value)

        return results


# ---------------------------------------------------------------------------
# ScipyAdapter
# ---------------------------------------------------------------------------

class ScipyAdapter(ReferenceAdapter):
    """
    Reference adapter using scipy.

    Covers: math_core (CDF functions), rate (Byar CI), fisher_exact.
    """

    name = "scipy"

    def is_available(self) -> bool:
        try:
            import scipy  # noqa: F401
            return True
        except ImportError:
            return False

    def fit(
        self,
        method: str,
        dataset_path: Path,
        spec: dict[str, Any],
    ) -> dict[str, float]:
        if method == "math_core":
            return self._fit_math_core(spec)
        if method == "rate":
            return self._fit_rate(dataset_path, spec)
        raise NotImplementedError(f"ScipyAdapter does not cover method '{method}'")

    # -- math_core ------------------------------------------------------------

    def _fit_math_core(self, spec: dict[str, Any]) -> dict[str, float]:
        from scipy import stats as sp_stats

        func = spec["function"]
        x = float(spec["x"])
        results: dict[str, float] = {}

        if func == "normal_cdf":
            results["normal_cdf"] = float(sp_stats.norm.cdf(x))
        elif func == "chi_square_cdf":
            df = float(spec["df"])
            results["chi_square_cdf"] = float(sp_stats.chi2.cdf(x, df))
        elif func == "t_cdf":
            df = float(spec["df"])
            results["t_cdf"] = float(sp_stats.t.cdf(x, df))
        elif func == "f_cdf":
            dfn = float(spec["dfn"])
            dfd = float(spec["dfd"])
            results["f_cdf"] = float(sp_stats.f.cdf(x, dfn, dfd))
        elif func == "fisher_exact":
            table = spec["table"]  # [[a, b], [c, d]]
            _, pvalue = sp_stats.fisher_exact(table)
            results["fisher_exact_pvalue"] = float(pvalue)
        else:
            raise ValueError(f"Unknown math_core function: {func!r}")

        return results

    # -- rate (Byar CI) -------------------------------------------------------

    def _fit_rate(self, dataset_path: Path, spec: dict[str, Any]) -> dict[str, float]:
        from scipy.stats import chi2

        df = pd.read_csv(dataset_path)
        events_col = spec["events_col"]
        person_time_col = spec["person_time_col"]
        multiplier = float(spec.get("multiplier", 1000.0))
        alpha = float(spec.get("alpha", 0.05))

        events = float(df[events_col].sum())
        person_time = float(df[person_time_col].sum())

        rate = (events / person_time) * multiplier

        # Byar's approximation for Poisson CI
        # Lower: chi2(2*d, alpha/2) / (2 * T) * multiplier
        # Upper: chi2(2*(d+1), 1-alpha/2) / (2 * T) * multiplier
        lower = chi2.ppf(alpha / 2, 2 * events) / (2 * person_time) * multiplier
        upper = chi2.ppf(1 - alpha / 2, 2 * (events + 1)) / (2 * person_time) * multiplier

        return {
            "estimate_per_1000": float(rate),
            "byar_ci_lower": float(lower),
            "byar_ci_upper": float(upper),
        }


# ---------------------------------------------------------------------------
# SklearnAdapter
# ---------------------------------------------------------------------------

class SklearnAdapter(ReferenceAdapter):
    """
    Reference adapter using scikit-learn.

    Covers: diagnostic_roc (AUC, sensitivity, specificity at threshold).
    """

    name = "sklearn"

    def is_available(self) -> bool:
        try:
            import sklearn  # noqa: F401
            return True
        except ImportError:
            return False

    def fit(
        self,
        method: str,
        dataset_path: Path,
        spec: dict[str, Any],
    ) -> dict[str, float]:
        if method == "diagnostic_roc":
            return self._fit_roc(dataset_path, spec)
        raise NotImplementedError(f"SklearnAdapter does not cover method '{method}'")

    def _fit_roc(self, dataset_path: Path, spec: dict[str, Any]) -> dict[str, float]:
        from sklearn.metrics import roc_auc_score, roc_curve

        df = pd.read_csv(dataset_path)
        label_col = spec["label_col"]
        score_col = spec["score_col"]
        threshold = float(spec.get("threshold", 0.5))

        y_true = df[label_col].values
        y_score = df[score_col].values

        auc = float(roc_auc_score(y_true, y_score))

        fpr, tpr, thresholds = roc_curve(y_true, y_score)
        # Find the index closest to the requested threshold
        idx = int(np.argmin(np.abs(thresholds - threshold)))
        sensitivity = float(tpr[idx])
        specificity = float(1.0 - fpr[idx])

        return {
            "auc": auc,
            f"sensitivity@{threshold}": sensitivity,
            f"specificity@{threshold}": specificity,
        }


# ---------------------------------------------------------------------------
# RsurvivalAdapter
# ---------------------------------------------------------------------------

class RsurvivalAdapter(ReferenceAdapter):
    """
    Reference adapter using R's survival package via Rscript.

    Optional: degrades to SKIP when Rscript is not installed.
    """

    name = "Rscript/survival"

    @property
    def is_python(self) -> bool:
        return False

    def is_available(self) -> bool:
        if shutil.which("Rscript") is None:
            return False
        # Check that the survival package is loadable
        try:
            result = subprocess.run(
                ["Rscript", "-e", "library(survival); cat('ok')"],
                capture_output=True,
                text=True,
                timeout=15,
            )
            return result.returncode == 0 and "ok" in result.stdout
        except Exception:
            return False

    def fit(
        self,
        method: str,
        dataset_path: Path,
        spec: dict[str, Any],
    ) -> dict[str, float]:
        if method == "cox":
            return self._fit_cox(dataset_path, spec)
        if method in ("km", "survival"):
            return self._fit_km(dataset_path, spec)
        raise NotImplementedError(f"RsurvivalAdapter does not cover method '{method}'")

    def _fit_cox(self, dataset_path: Path, spec: dict[str, Any]) -> dict[str, float]:
        duration_col = spec["duration_col"]
        event_col = spec["event_col"]
        covariates = spec["covariates"]
        cov_str = " + ".join(covariates)

        r_script = f"""
library(survival)
library(jsonlite)
df <- read.csv("{dataset_path.as_posix()}")
fit <- coxph(Surv({duration_col}, {event_col}) ~ {cov_str}, data=df, ties="efron")
s <- summary(fit)
coefs <- s$coefficients
out <- list()
for (cov in rownames(coefs)) {{
  out[[paste0("beta[", cov, "]")]] <- coefs[cov, "coef"]
  out[[paste0("stderr[", cov, "]")]] <- coefs[cov, "se(coef)"]
  out[[paste0("hazard_ratio[", cov, "]")]] <- coefs[cov, "exp(coef)"]
  out[[paste0("pvalue[", cov, "]")]] <- coefs[cov, "Pr(>|z|)"]
}}
out[["log_partial_likelihood"]] <- fit$loglik[2]
out[["concordance"]] <- s$concordance["C"]
cat(toJSON(out, auto_unbox=TRUE))
"""
        return self._run_r(r_script)

    def _fit_km(self, dataset_path: Path, spec: dict[str, Any]) -> dict[str, float]:
        duration_col = spec["duration_col"]
        event_col = spec["event_col"]

        r_script = f"""
library(survival)
library(jsonlite)
df <- read.csv("{dataset_path.as_posix()}")
fit <- survfit(Surv({duration_col}, {event_col}) ~ 1, data=df)
out <- list()
for (i in seq_along(fit$time)) {{
  out[[paste0("survival_probability[", i-1, "]")]] <- fit$surv[i]
  out[[paste0("event_time[", i-1, "]")]] <- fit$time[i]
  out[[paste0("greenwood_se[", i-1, "]")]] <- fit$std.err[i]
}}
out[["median_survival"]] <- as.numeric(quantile(fit, probs=0.5)$quantile)
cat(toJSON(out, auto_unbox=TRUE))
"""
        return self._run_r(r_script)

    def _run_r(self, r_script: str) -> dict[str, float]:
        result = subprocess.run(
            ["Rscript", "-e", r_script],
            capture_output=True,
            text=True,
            timeout=60,
        )
        if result.returncode != 0:
            raise RuntimeError(
                f"Rscript failed (exit {result.returncode}): {result.stderr.strip()[:300]}"
            )
        stdout = result.stdout.strip()
        if not stdout:
            raise RuntimeError("Rscript produced no output")
        raw = json.loads(stdout)
        return {k: float(v) for k, v in raw.items()}


# ---------------------------------------------------------------------------
# KnownValueAdapter
# ---------------------------------------------------------------------------

class KnownValueAdapter(ReferenceAdapter):
    """
    Reference adapter that reads hardcoded known values from JSON files.

    Files live in ``validation/known_values/<method>_*.json``.
    Each file must contain ``{"method": "...", "expected": {"metric": value, ...}}``.
    """

    name = "known_value"

    def __init__(self) -> None:
        self._known_values_dir = Path(__file__).resolve().parent.parent / "known_values"

    def is_available(self) -> bool:
        return self._known_values_dir.exists() and any(
            self._known_values_dir.glob("*.json")
        )

    def fit(
        self,
        method: str,
        dataset_path: Path,
        spec: dict[str, Any],
    ) -> dict[str, float]:
        """
        Return known values for *method* from the first matching JSON file.

        The JSON file is matched by ``file["method"] == method``.
        """
        for json_file in sorted(self._known_values_dir.glob("*.json")):
            with open(json_file, "r", encoding="utf-8") as fh:
                data = json.load(fh)
            if data.get("method") == method:
                expected = data.get("expected", {})
                return {k: float(v) for k, v in expected.items()}
        raise FileNotFoundError(
            f"No known-value JSON found for method '{method}' "
            f"in {self._known_values_dir}"
        )

    def files_for_method(self, method: str) -> list[Path]:
        """Return all known-value JSON files that match *method*."""
        matches = []
        for json_file in sorted(self._known_values_dir.glob("*.json")):
            try:
                with open(json_file, "r", encoding="utf-8") as fh:
                    data = json.load(fh)
                if data.get("method") == method:
                    matches.append(json_file)
            except Exception:
                pass
        return matches


# ---------------------------------------------------------------------------
# Adapter registry
# ---------------------------------------------------------------------------

# Instantiate once; adapters are stateless.
_statsmodels = StatsmodelsAdapter()
_lifelines = LifelinesAdapter()
_scipy = ScipyAdapter()
_sklearn = SklearnAdapter()
_rsurvival = RsurvivalAdapter()
_known_value = KnownValueAdapter()

ADAPTERS_FOR: dict[str, list[ReferenceAdapter]] = {
    "linear":         [_statsmodels],
    "logistic":       [_statsmodels],
    "cox":            [_lifelines, _rsurvival],
    "survival":       [_lifelines, _rsurvival],
    "rate":           [_scipy],
    "power":          [_statsmodels],
    "math_core":      [_scipy, _statsmodels],
    "tableone":       [_scipy],
    "diagnostic_roc": [_sklearn],
}
