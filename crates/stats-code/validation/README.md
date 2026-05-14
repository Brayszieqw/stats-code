# Stats Code — Validation Correctness Framework

This directory contains the **Validation Correctness Framework (VCF)** for Stats Code.
VCF systematically compares Stats Code's numerical outputs against established reference
engines (Python `statsmodels` / `lifelines` / `scipy` / `sklearn`, R `survival`) and
textbook known values to build a citable evidence chain of numerical correctness.

---

## Migration from `scripts/verify_numeric_parity.py`

The legacy script `crates/stats-code/scripts/verify_numeric_parity.py` (≈200 lines,
covering linear / logistic / Cox on a single 40-row synthetic dataset) has been
**superseded** by this framework and deleted.

| Old script | New equivalent |
| --- | --- |
| `compare_linear()` | `parity/linear.py` — covers all coefficients, R², F-stat |
| `compare_logistic()` | `parity/logistic.py` — covers all coefficients, log-likelihood |
| `compare_cox()` | `parity/cox.py` — covers all coefficients, log-partial-likelihood, concordance |
| Single 40-row dataset | `datasets/synthetic/small_n40.csv` + medium + large + edge cases |
| R comparison | `parity/adapters.py::RsurvivalAdapter` (graceful SKIP when unavailable) |

To run the equivalent of the old script:

```bash
cd crates/stats-code/validation
python run_validation.py --methods linear,logistic,cox --datasets "datasets/synthetic/small_n40.csv"
```

---

## Quick Start

```bash
# Install Python dependencies (Python ≥ 3.11 required)
pip install -r requirements.txt

# Run full validation suite
python run_validation.py

# Run specific methods
python run_validation.py --methods linear,logistic

# Run with verbose output (populates details field)
python run_validation.py --verbose

# Custom output directory
python run_validation.py --out my_reports/
```

---

## CLI Reference

| Flag | Default | Description |
| --- | --- | --- |
| `--methods METHOD[,...]` | all | Comma-separated method names to validate |
| `--datasets GLOB[,...]` | per-method defaults | Glob patterns for dataset CSV files |
| `--verbose` | off | Populate `ValidationResult.details` with intermediate values |
| `--out DIR` | `reports/` | Output directory for `report.json` and `report.md` |
| `--tolerance-config YAML` | `tolerance_config.yaml` | Path to custom tolerance configuration |

Exit codes: `0` = all PASS/SKIP, `1` = any FAIL or ERROR.

---

## Supported Methods

| Method | Reference Engines | Default Datasets |
| --- | --- | --- |
| `linear` | statsmodels, known_value | synthetic, public |
| `logistic` | statsmodels, known_value | synthetic, public |
| `cox` | lifelines, Rscript/survival, known_value | synthetic, public |
| `survival` | lifelines, Rscript/survival | synthetic, public |
| `rate` | scipy | synthetic |
| `power` | statsmodels | (built-in test points) |
| `math_core` | scipy | (built-in test points) |
| `tableone` | scipy | synthetic, public |
| `diagnostic_roc` | sklearn | synthetic |

---

## Tolerance Rationale

Tolerance values are defined in `tolerance_config.yaml`. The rationale for each value
is documented in `.kiro/specs/validation-correctness/design.md`, section "容差决策表".

Summary:

| Category | Tolerance | Justification |
| --- | --- | --- |
| Closed-form (linear, KM, rate) | ≤ 1e-8 | QR/SVD or product-limit; double precision |
| Iterative (logistic) | ≤ 1e-5 | IRLS convergence + cross-engine differences |
| Iterative (Cox) | ≤ 1e-4 | Partial likelihood + tie handling |
| CDF-derived p-values | ≤ 1e-6 | CDF algorithm differences across libraries |
| Math core CDFs | ≤ 1e-10 | Same algorithm family as scipy |
| Integer / exact | 0.0 | Must match exactly |

---

## Property → Test File Matrix

| Property | Title | Test File |
| --- | --- | --- |
| P1 | Numerical Parity | `tests/properties/test_numerical_parity.py` |
| P2 | Math Core Functional Equivalence | `tests/properties/test_math_core.py` |
| P3 | Tolerance Policy Monotonicity | `tests/properties/test_tolerance_policy.py` |
| P4 | Failure Result Completeness | `tests/properties/test_comparator_failures.py` |
| P5 | Primary Reference Engine Coverage | `tests/properties/test_primary_engine_coverage.py` |
| P6 | Adapter Availability Gating | `tests/properties/test_adapter_availability.py` |
| P7 | Reference Engine Discrepancy Detection | `tests/properties/test_discrepancy_detection.py` |
| P8 | Report Structural Completeness | `tests/properties/test_report_structure.py` |
| P9 | Summary Aggregation Fidelity | `tests/properties/test_summary_aggregation.py` |
| P10 | Exit Code Consistency | `tests/properties/test_exit_code.py` |
| P11 | CLI Filter Subset Relation | `tests/properties/test_cli_filter.py` |
| P12 | Method Coverage Completeness | `tests/properties/test_method_coverage.py` |
| P13 | Verbose Gating of Details | `tests/properties/test_verbose_details.py` |

---

## Dataset PROVENANCE

- `datasets/synthetic/PROVENANCE.md` — generated datasets (seed 20260510)
- `datasets/public/PROVENANCE.md` — NHANES subset, Framingham-like
- `datasets/edge_cases/PROVENANCE.md` — edge case datasets

---

## Adding a New Method

1. Create `parity/<method>.py` — implement `METHOD`, `METRICS`, `collect()`
2. Add tolerance entries to `tolerance_config.yaml`
3. Add at least one dataset to `datasets/` and update its `PROVENANCE.md`
4. Register the method in `run_validation.py::METHOD_IMPORTERS`
5. Property test P1 will automatically cover the new method (parametrized)

---

## Stata / SPSS Coverage

Stata and SPSS are not available on GitHub Actions runners (commercial licenses).
This is a known gap. The recommended manual workflow:

1. Run the analysis in Stata/SPSS
2. Record results in `known_values/stata/<method>.json`
3. VCF picks them up automatically via `KnownValueAdapter`

---

## Running Tests

```bash
# Unit tests only
pytest tests/unit -v

# Property tests
pytest tests/properties -v

# Smoke tests
pytest tests/smoke -v

# All tests (excluding slow)
pytest -m "not slow" -v

# All tests including slow (N=2000 datasets)
pytest -v
```
