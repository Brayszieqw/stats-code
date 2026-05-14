# Stats Code Validation Report

**Status: ✅ VALIDATED**

Generated: 2026-05-13T12:04:45.554114+00:00
Stats Code: commit `6fa0358efd5c`
Python: 3.13.5 | R: unavailable | OS: Windows-11-10.0.26200-SP0

## Summary

| Method | Pass | Fail | Skip | Error |
| --- | ---: | ---: | ---: | ---: |
| cox | 10 | 0 | 6 | 0 |
| linear | 16 | 0 | 0 | 0 |
| logistic | 17 | 0 | 1 | 0 |
| **Total** | **43** | **0** | **7** | **0** |

## Failures

_No failures._

## Per-method Details

### cox

- Datasets: small_n40.csv
- References: Rscript/survival, lifelines
- Results: 10/16 pass

| Dataset | Reference | Metric | Status | Expected | Actual | Diff |
| --- | --- | --- | --- | ---: | ---: | ---: |
| small_n40.csv | lifelines | beta[age] | pass | 0.023504989484685816 | 0.02350500168137634 | 1.220e-08 |
| small_n40.csv | lifelines | stderr[age] | pass | 0.016809179076126054 | 0.016809179114127947 | 3.800e-11 |
| small_n40.csv | lifelines | hazard_ratio[age] | pass | 1.023783408885526 | 1.0237834213722954 | 1.249e-08 |
| small_n40.csv | lifelines | pvalue[age] | pass | 0.1620102412554526 | 0.162010128928064 | 1.123e-07 |
| small_n40.csv | lifelines | beta[bmi] | pass | 0.03446573256445954 | 0.03446573556060644 | 2.996e-09 |
| small_n40.csv | lifelines | stderr[bmi] | pass | 0.03447787626939119 | 0.034477877231710995 | 9.623e-10 |
| small_n40.csv | lifelines | hazard_ratio[bmi] | pass | 1.0350665586918102 | 1.0350665617930217 | 3.101e-09 |
| small_n40.csv | lifelines | pvalue[bmi] | pass | 0.31748099043782807 | 0.3174809728638577 | 1.757e-08 |
| small_n40.csv | lifelines | log_partial_likelihood | pass | -87.31629557611846 | -87.31629557611808 | 3.837e-13 |
| small_n40.csv | lifelines | concordance | pass | 0.6405325443786982 | 0.6405325443786982 | 0.000e+00 |
| small_n40.csv | Rscript/survival | beta | skip | — | — | — |
| small_n40.csv | Rscript/survival | stderr | skip | — | — | — |
| small_n40.csv | Rscript/survival | hazard_ratio | skip | — | — | — |
| small_n40.csv | Rscript/survival | pvalue | skip | — | — | — |
| small_n40.csv | Rscript/survival | log_partial_likelihood | skip | — | — | — |
| small_n40.csv | Rscript/survival | concordance | skip | — | — | — |

### linear

- Datasets: small_n40.csv
- References: statsmodels
- Results: 16/16 pass

| Dataset | Reference | Metric | Status | Expected | Actual | Diff |
| --- | --- | --- | --- | ---: | ---: | ---: |
| small_n40.csv | statsmodels | beta[age] | pass | 0.4180502153336406 | 0.4180502153336514 | 1.077e-14 |
| small_n40.csv | statsmodels | stderr[age] | pass | 0.034157949047581036 | 0.03415794904758099 | 4.857e-17 |
| small_n40.csv | statsmodels | t_stat[age] | pass | 12.238738770624336 | 12.238738770624668 | 3.322e-13 |
| small_n40.csv | statsmodels | pvalue[age] | pass | 1.4189311621319277e-14 | 1.418931162130797e-14 | 1.131e-26 |
| small_n40.csv | statsmodels | beta[bmi] | pass | 0.7422724110256803 | 0.7422724110256809 | 5.551e-16 |
| small_n40.csv | statsmodels | stderr[bmi] | pass | 0.06649196126471908 | 0.06649196126471936 | 2.776e-16 |
| small_n40.csv | statsmodels | t_stat[bmi] | pass | 11.163340603994685 | 11.163340603994646 | 3.908e-14 |
| small_n40.csv | statsmodels | pvalue[bmi] | pass | 2.10219850996667e-13 | 2.1021985099668834e-13 | 2.136e-26 |
| small_n40.csv | statsmodels | beta[Intercept] | pass | 6.633752204216432 | 6.633752204215625 | 8.065e-13 |
| small_n40.csv | statsmodels | stderr[Intercept] | pass | 2.560617606993805 | 2.5606176069938105 | 5.329e-15 |
| small_n40.csv | statsmodels | t_stat[Intercept] | pass | 2.590684445072036 | 2.590684445071716 | 3.202e-13 |
| small_n40.csv | statsmodels | pvalue[Intercept] | pass | 0.013624524879368218 | 0.013624524879378985 | 1.077e-14 |
| small_n40.csv | statsmodels | r_squared | pass | 0.8993650575093157 | 0.8993650575093157 | 0.000e+00 |
| small_n40.csv | statsmodels | adj_r_squared | pass | 0.8939253308881977 | 0.8939253308881976 | 1.110e-16 |
| small_n40.csv | statsmodels | f_stat | pass | 165.33276764641195 | 165.33276764641204 | 8.527e-14 |
| small_n40.csv | statsmodels | f_pvalue | pass | 3.555108427387577e-19 | 0.0 | 3.555e-19 |

### logistic

- Datasets: small_n40.csv
- References: statsmodels
- Results: 17/18 pass

| Dataset | Reference | Metric | Status | Expected | Actual | Diff |
| --- | --- | --- | --- | ---: | ---: | ---: |
| small_n40.csv | statsmodels | beta[age] | pass | 0.03219257112802129 | 0.03219257112802148 | 1.874e-16 |
| small_n40.csv | statsmodels | stderr[age] | pass | 0.029582398149980458 | 0.0295823987711995 | 6.212e-10 |
| small_n40.csv | statsmodels | wald[age] | pass | 1.0882339884957082 | 1.0882339656432176 | 2.285e-08 |
| small_n40.csv | statsmodels | pvalue[age] | pass | 0.2764918236552736 | 0.2764919075316785 | 8.388e-08 |
| small_n40.csv | statsmodels | odds_ratio[age] | pass | 1.032716357511799 | 1.0327163575117992 | 2.220e-16 |
| small_n40.csv | statsmodels | beta[bmi] | pass | 0.16796584929075514 | 0.16796584929075592 | 7.772e-16 |
| small_n40.csv | statsmodels | stderr[bmi] | pass | 0.062111915658249826 | 0.06211191784636711 | 2.188e-09 |
| small_n40.csv | statsmodels | wald[bmi] | pass | 2.7042451920969786 | 2.7042450968301592 | 9.527e-08 |
| small_n40.csv | statsmodels | pvalue[bmi] | pass | 0.006845975264281853 | 0.006846076846220184 | 1.016e-07 |
| small_n40.csv | statsmodels | odds_ratio[bmi] | pass | 1.18289621321337 | 1.182896213213371 | 8.882e-16 |
| small_n40.csv | statsmodels | beta[Intercept] | pass | -6.983991278464087 | -6.9839912784641225 | 3.553e-14 |
| small_n40.csv | statsmodels | stderr[Intercept] | pass | 2.5604208630244054 | 2.5604209655780985 | 1.026e-07 |
| small_n40.csv | statsmodels | wald[Intercept] | pass | -2.727673164721247 | -2.72767305546854 | 1.093e-07 |
| small_n40.csv | statsmodels | pvalue[Intercept] | pass | 0.00637827700892332 | 0.006378384653260127 | 1.076e-07 |
| small_n40.csv | statsmodels | odds_ratio[Intercept] | pass | 0.0009265975042226056 | 0.0009265975042225727 | 3.296e-17 |
| small_n40.csv | statsmodels | log_likelihood | pass | -22.021356397940778 | -22.02135639794077 | 7.105e-15 |
| small_n40.csv | statsmodels | c_statistic | pass | 0.79 | 0.79 | 0.000e+00 |
| small_n40.csv | statsmodels | nagelkerke_r2 | skip | — | — | — |

## Appendix: Tolerance Rationale

Tolerance values are defined in `validation/tolerance_config.yaml`. The rationale for each value is documented in the design document (`design.md`, section "容差决策表").

| Category | Tolerance | Justification |
| --- | --- | --- |
| Closed-form (linear, KM, rate) | ≤ 1e-8 | QR/SVD or product-limit; double precision |
| Iterative (logistic) | ≤ 1e-5 | IRLS convergence + cross-engine diff |
| Iterative (Cox) | ≤ 1e-4 | Partial likelihood + tie handling |
| CDF-derived p-values | ≤ 1e-6 | CDF algorithm differences |
| Math core CDFs | ≤ 1e-10 | Same algorithm family as scipy |
| Integer / exact | 0.0 | Must match exactly |

## Reference Engine Versions

| Engine | Version |
| --- | --- |
| lifelines | 0.30.3 |
| numpy | 2.4.1 |
| pandas | 2.3.3 |
| scikit-learn | 1.8.0 |
| scipy | 1.16.3 |
| statsmodels | 0.14.6 |
| Rscript | unavailable |
