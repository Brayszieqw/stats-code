# Edge Case Dataset Provenance

## Overview

All files in this directory are **fully synthetic** edge cases designed to trigger
specific numerical or algorithmic boundary conditions.

## Files

### `logistic_perfect_separation.csv`

| Property | Value |
| --- | --- |
| N | 20 |
| Generator | `datasets/_preprocess/gen_edge_logistic_perfect_separation.py` |
| Seed | 20260510 |
| License | Synthetic / public domain |

**Trigger scenario:** Perfect separation — `y=1` iff `x > 0`. Logistic regression
coefficients diverge to ±∞. Tests graceful handling of non-convergence.

**Applicable methods:** `logistic`

**Columns:** `x` (float), `y` (int 0/1)

---

### `survival_tied_times.csv`

| Property | Value |
| --- | --- |
| N | 50 |
| Generator | `datasets/_preprocess/gen_edge_survival_tied_times.py` |
| Seed | 20260510 |
| License | Synthetic / public domain |

**Trigger scenario:** Only 8 distinct event times across 50 observations. Tests
Breslow/Efron tie-handling in Cox PH and Kaplan–Meier estimators.

**Applicable methods:** `cox`, `survival`

**Columns:** `age` (float), `bmi` (float), `time` (float), `death` (int 0/1)

---

### `zero_variance_covariate.csv`

| Property | Value |
| --- | --- |
| N | 100 |
| Generator | `datasets/_preprocess/gen_edge_zero_variance_covariate.py` |
| Seed | 20260510 |
| License | Synthetic / public domain |

**Trigger scenario:** `constant_col` is identically 5.0 for all rows, making the
design matrix rank-deficient. Tests singular matrix handling.

**Applicable methods:** `linear`, `logistic`

**Columns:** `age`, `bmi`, `constant_col` (always 5.0), `disease` (0/1), `linear_y`

---

### `single_obs_group.csv`

| Property | Value |
| --- | --- |
| N | 30 |
| Generator | `datasets/_preprocess/gen_edge_single_obs_group.py` |
| Seed | 20260510 |
| License | Synthetic / public domain |

**Trigger scenario:** Group "C" contains only 1 observation. Tests Table One
group-comparison methods (t-test, chi-square, Kruskal–Wallis) with degenerate
group sizes.

**Applicable methods:** `tableone`

**Columns:** `age`, `bmi`, `linear_y`, `group` (A/B/C with counts 15/14/1)

---

### `collinear_predictors.csv`

| Property | Value |
| --- | --- |
| N | 100 |
| Generator | `datasets/_preprocess/gen_edge_collinear_predictors.py` |
| Seed | 20260510 |
| License | Synthetic / public domain |

**Trigger scenario:** `x2 ≈ 2*x1 + ε` (ε ~ N(0, 0.01)), condition number of X'X ≈ 8.7×10⁶.
Tests numerical stability under near-multicollinearity.

**Applicable methods:** `linear`, `logistic`

**Columns:** `x1`, `x2` (≈ 2·x1), `linear_y`, `disease` (0/1)

---

## Modification History

| Date | Change |
| --- | --- |
| 2026-05-13 | Initial generation (seed 20260510) |
