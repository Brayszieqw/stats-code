# `parity/cases/` — Per-cell Live test case definitions

Validates Requirements 4.3, 4.8 (task 11.7).

This directory is the **registry** for the Live arm of the CI Parity
Suite. Every ``(algorithm, software)`` cell in
`crates/stats-code/src/coverage_matrix/matrix.toml` whose value is
`live` has exactly one `LiveCase` entry in [`registry.py`](registry.py),
which the test driver in
`crates/stats-code/validation/tests/properties/test_live_parity_cases.py`
parametrizes over.

## Why a registry instead of one test file per cell?

* All Live cells share the same harness shape:

      hypothesis dataset
          ─▶ parity.<algorithm>.collect(dataset, tol_config, adapters, spec)
              ├─ Stats Engine CLI invocation
              └─ Reference adapter invocation
          ─▶ threshold.fail_predicate
          ─▶ assert no failure verdicts

  Centralising the data definition avoids 13 near-identical test files
  and keeps the matrix ↔ case mapping legible at a glance.

* The test driver is the only place that runs `cargo run -p stats-code`,
  so we get one slow-test gate (`@pytest.mark.slow`) and one collection
  surface — `pytest --collect-only` shows the case for every cell while
  default `pytest -q` keeps CI fast on contributors who haven't built
  Rust yet.

* New cells (added by future PRs) only need to append a `LiveCase` here;
  the driver picks them up automatically.

## What is *not* in the registry

* SAS / SPSS cells — the matrix only ever marks them `recorded`; their
  cases live under `validation/known_values/{sas,spss}/<algorithm>/`
  and are exercised by the Recorded arm (task 11.8).

* `sidecar_only` cells — by definition no Reference adapter is wired
  for them, so no Live case is meaningful.

* `none` cells — no snippet, no case, no work.

## Strategy invariants

[`strategies.py`](strategies.py) draws a 30-row dataset shaped exactly
like the columns the existing parity collectors default to
(`age, bmi, linear_y, disease, time, death, group`). The drawn
instances are:

* balanced 15/15 on `group`,
* strictly positive on every cell of the 2x2 `(disease × group)` and
  `(death × group)` tables,
* signal-bearing on `linear_y` so OLS recovers a well-defined β.

These constraints prevent Hypothesis from shrinking to inputs that
crash Cox / OR-RR / KM with singular-matrix or zero-cell errors.

## Cells whose R Reference adapter is not yet wired

`parity/adapters.py` currently exposes Rscript adapters only for
`cox` / `kaplan_meier`. The remaining R live cells (ttest, anova,
nonparametric, correlation, or_rr, linear, logistic,
power_single_arm, power_phase3, tableone, diagnostic_roc) still have
a registry entry — their adapter selector returns an empty list and
the test driver turns that into a structured
`live_R_adapter_not_yet_implemented` skip. This keeps the matrix
consistency check (task 11.6) honest while the rest of the R
adapters are implemented in a follow-up wave.
