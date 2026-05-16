0# Implementation Plan: Statistical Methods Complete

## Overview

This plan implements 30 statistical and epidemiological methods in three phases (HIGH → MEDIUM → LOW), strictly following the phased delivery in `design.md`. Implementation is Rust-native by default in the existing `crates/stats-code` workspace, with Python-bridge fallback (via `bridge_runner.py`) for methods that require numerically delicate iterative procedures (REML, multinomial/ordinal logits, LDA, Aalen-Johansen variance, Gray's test).

The plan is organized so that:

1. **Foundation tasks** (math primitives, CLI scaffolding, helpers, schema prelude) land first so every method downstream has a stable substrate.
2. **Each method** gets its own implementation task plus mandatory unit tests against R/SciPy gold-reference fixtures, an optional CLI integration task, and (where applicable) optional `proptest` invariants from requirements §G4.
3. **Wiring tasks** (`Command::Stats` dispatch arms, `analysis.yaml` step registration, text renderers) are batched per phase to avoid serial conflicts on `cli.rs`, `handlers.rs`, `schema/results.rs`, and `render/stats.rs`.
4. **Checkpoints** between phases confirm that the new code is green before moving deeper into the dependency cone.

All optional sub-tasks (test-related, marked with `*`) implement the proptest invariants and integration coverage mandated by requirements §G4 but can be skipped for a faster MVP. Top-level tasks and core-implementation sub-tasks are never optional.

## Tasks

### Foundation

- [x] 1. Wire global CLI surface for the `stats` group
  - [x] 1.1 Add `Command::Stats { command: StatsCommand }` variant and skeleton enum in `crates/stats-code/src/cli.rs`
    - Define `StatsCommand`, `TtestCommand`, `AnovaCommand`, `NonparamCommand`, `DiagnosticCommand`, `EpiStatsCommand`, `AgreementCommand`, `MultivariateCommand`, `SampleSizeCommand`, `StatsSurvivalCommand`, `StatsModelCommand` enums with empty/`todo!()` payloads for now (filled per-method later)
    - Add `--alpha: f64` and `--na-strategy: NaStrategy { Drop, Error }` as global clap flags on `Cli`
    - Re-export `StatsCommand` and `NaStrategy` from `lib.rs`
    - _Requirements: G1.1, G1.3, G1.6, G2.2_

  - [x] 1.2 Wire `Command::Stats` into `handlers::dispatch` with a placeholder `handle_stats` shim
    - Create `crates/stats-code/src/handlers/stats.rs` exporting `handle_stats(cli, command) -> Result<ArtifactPayload, String>` that matches on `StatsCommand` and currently returns `Err("not yet implemented for <method>")` for every leaf
    - Register the new file via `mod stats;` in `handlers.rs`
    - _Requirements: G1.1, G1.6_

- [x] 2. Build shared math, helper, and schema foundations
  - [x] 2.1 Refactor `math.rs` into a `math/` module and add missing distribution primitives
    - Move existing `math.rs` content into `crates/stats-code/src/math/mod.rs` with `pub use` re-exports for back-compat
    - Add `math/distributions.rs`: `student_t_two_sided`, `student_t_inv`, `f_distribution_p`, `chi_square_p`, `studentized_range_p`, `lilliefors_p`, normal/inverse-normal already exist or extend
    - Add `math/linalg.rs`: matrix multiply, `invert_with_ridge`, `jacobi_eigh` (symmetric eigen-decomposition for PCA)
    - _Requirements: foundational dependency for Req 1–4, 11, 16, 24_

  - [x] 2.2 Add the shared IRLS engine in `math/glm.rs`
    - Generic `irls_fit<F: Family>(x, y, offset, max_iter, tol) -> IrlsFit { beta, vcov, iterations, converged, deviance, pearson_chi_square, log_likelihood }`
    - Implement `Family::Poisson` (log link) so Req 13 and Req 20 share one IRLS implementation
    - _Requirements: 13.1, 13.4, 20.1_

  - [x] 2.3 Extend `helpers.rs` with multi-column primitives
    - Add `require_columns(headers, names) -> Result<Vec<usize>, String>`
    - Add `drop_missing_rows_for_columns(rows, col_indices, dictionary) -> (Vec<StringRecord>, /*excluded*/ usize)` honoring the new global `na_strategy`
    - Add `parse_binary_event_column(values, dictionary, column_name, override_event) -> Result<Vec<bool>, String>`
    - Add `parse_numeric_column(values, dictionary, column_name) -> Result<Vec<f64>, String>`
    - _Requirements: G2.1, G2.2, G2.4_

  - [x]* 2.4 Property test for missing-value handling (proptest invariant from G4.3)
    - Generate `Vec<Vec<Option<f64>>>` and a permutation; assert `drop_missing_rows_for_columns` returns the same surviving rows (as a multiset) before and after permutation
    - File: `crates/stats-code/tests/proptest/missing_props.rs`
    - _Requirements: G4.3 (row-permutation invariance), G2.1, G2.4_

- [x] 3. Schema prelude scaffolding for the new result family
  - [x] 3.1 Add the `*Result` prelude convention to `schema/results.rs`
    - Define a private `result_prelude!` declarative macro (or a `ResultPrelude` struct flattened via `#[serde(flatten)]`) that injects `status`, `data_path`, `analysis_path`, `n_total`, `n_used`, `n_excluded_missing`, `notes: Vec<String>`, `warnings: Vec<String>` so every new struct uses an identical prelude
    - Add empty placeholder `*Result` structs (one per method) so downstream tasks can implement bodies independently
    - Re-export every new struct via `schema/mod.rs` and `lib.rs`
    - _Requirements: G1.5_

### Phase 1 — HIGH-Priority Methods (Req 1–10, 16–19)

- [ ] 4. Implement t-test family (Req 1, 2)
  - [x] 4.1 Implement paired and one-sample t-tests
    - File: `crates/stats-code/src/stats/ttest.rs`
    - `paired_ttest_csv(...) -> Result<TtestPairedResult, String>` and `one_sample_ttest_csv(...) -> Result<TtestOneSampleResult, String>` per `design.md` §Components→T-Tests
    - Wire `TtestPairedArgs` / `TtestOneSampleArgs` into `cli.rs::TtestCommand` and `handlers/stats.rs`
    - _Requirements: 1.1, 1.2, 1.3, 1.4, 2.1, 2.2, 2.3, 2.4_

  - [ ] 4.2 Add gold-reference fixture and unit tests for t-tests
    - Place R-generated JSON fixtures under `crates/stats-code/tests/fixtures/r/ttest_*.json`
    - Cross-check sampled cases against SciPy fixtures under `tests/fixtures/python/ttest_*.json`
    - _Requirements: G4.1, G5.1, G5.3_

  - [x]* 4.3 Property test: paired t symmetry under sign flip and df = n-1
    - **Property: paired-t sign flip symmetry — flipping the sign of all paired differences negates `t_statistic` and preserves `|t|`, `p_value`, `degrees_freedom == n - 1`**
    - File: `crates/stats-code/tests/proptest/ttest_props.rs`
    - _Requirements: G4.3, 1.1, 1.2_

  - [ ]* 4.4 CLI integration test (snapshot JSON shape)
    - File: `crates/stats-code/tests/stats_ttest_cli.rs`
    - _Requirements: G4.2_

- [ ] 5. Implement ANOVA family — one-way and randomized block (Req 3)
  - [x] 5.1 Implement one-way ANOVA and RBD ANOVA
    - File: `crates/stats-code/src/stats/anova.rs`
    - `oneway_anova_csv(...)` partitions SS_total = SS_between + SS_within; switches to `rbd_anova_csv(...)` when `--block` is given
    - Sparse-group error: any group with n < 2 returns the descriptive error from Req 3.4
    - Wire `AnovaCommand::Oneway` into `cli.rs` and `handlers/stats.rs`
    - _Requirements: 3.1, 3.2, 3.3, 3.4, 3.5_

  - [ ] 5.2 R gold-reference fixtures and unit tests for ANOVA
    - One CRD example, one RBD example, one sparse-group failure case
    - _Requirements: G4.1, G5.1_

  - [ ]* 5.3 Property test: SS decomposition identity
    - **Property: SS_total = SS_between + SS_within (exact, modulo f64 epsilon)**
    - File: `crates/stats-code/tests/proptest/anova_props.rs`
    - _Requirements: G4.3, 3.1_

  - [ ]* 5.4 CLI integration test for ANOVA
    - _Requirements: G4.2_

- [ ] 6. Implement Cochran-Armitage trend test (Req 4)
  - [x] 6.1 Implement Cochran-Armitage in `stats/nonparam.rs`
    - Default scores 0..k-1 unless `--scores` overrides; reject when fewer than 2 ordered categories carry events
    - Wire `NonparamCommand::CochranArmitage` into `cli.rs` and `handlers/stats.rs`
    - _Requirements: 4.1, 4.2, 4.3, 4.4_

  - [ ] 6.2 R gold-reference fixtures and unit tests
    - _Requirements: G4.1, G5.1_

  - [ ]* 6.3 CLI integration test
    - _Requirements: G4.2_

- [ ] 7. Implement McNemar, Wilcoxon signed-rank, and Mann-Whitney U (Req 5, 6, 7)
  - [x] 7.1 Implement McNemar (with continuity correction and exact binomial fallback for b+c < 25)
    - Append to `stats/nonparam.rs`
    - Wire `NonparamCommand::Mcnemar` into `cli.rs` and `handlers/stats.rs`
    - _Requirements: 5.1, 5.2, 5.3, 5.4_

  - [x] 7.2 Implement Wilcoxon signed-rank (average ranks, tie-correction, zero-pair exclusion)
    - Append to `stats/nonparam.rs`
    - Wire `NonparamCommand::Wilcoxon`
    - _Requirements: 6.1, 6.2, 6.3, 6.4_

  - [x] 7.3 Implement Mann-Whitney U (z-score with tie correction)
    - Append to `stats/nonparam.rs`
    - Wire `NonparamCommand::Mannwhitney`; reject when grouping variable does not have exactly 2 levels
    - _Requirements: 7.1, 7.2, 7.3, 7.4_

  - [ ] 7.4 R gold-reference fixtures and unit tests for nonparam tests
    - _Requirements: G4.1, G5.1_

  - [ ]* 7.5 CLI integration tests for nonparam tests
    - _Requirements: G4.2_

- [ ] 8. Implement correlation analysis (Req 8)
  - [x] 8.1 Implement Pearson r and Spearman ρ in `stats/correlation.rs`
    - Pearson via Fisher z transform for CI, t-distribution for p-value
    - Spearman via average ranks then Pearson on ranks; refuse when n < 3 complete pairs
    - Wire `StatsCommand::Correlation(CorrelationArgs)` into `cli.rs` and `handlers/stats.rs`
    - _Requirements: 8.1, 8.2, 8.3, 8.4, 8.5_

  - [ ] 8.2 R + SciPy cross-checked fixtures and unit tests
    - _Requirements: G4.1, G5.1, G5.3_

  - [ ]* 8.3 CLI integration test
    - _Requirements: G4.2_

- [ ] 9. Implement OR/RR with Mantel-Haenszel stratification (Req 9)
  - [x] 9.1 Implement crude OR/RR + 2×2 chi-square in `stats/epi/effect.rs`
    - Apply 0.5 continuity correction when any cell is zero; emit warning
    - Wire `EpiStatsCommand::OrRr` into `cli.rs` and `handlers/stats.rs`
    - _Requirements: 9.1, 9.2, 9.3, 9.4, 9.5_

  - [ ] 9.2 Add Mantel-Haenszel stratified pooling and Breslow-Day homogeneity
    - Activated when `--strata` is provided
    - _Requirements: 9.5_

  - [ ] 9.3 R + SciPy cross-checked fixtures and unit tests
    - Cover at least: zero-cell, zero-stratum, single-row stratum
    - _Requirements: G4.1, G5.1, G5.3_

  - [ ]* 9.4 Property tests: 2×2 edge cases and continuity-correction safety
    - **Property: continuity-corrected OR and RR are always finite (never NaN/Inf) for any non-negative integer 2×2 table**
    - **Property: zero-cell and single-row inputs do not panic and emit the expected warning string**
    - File: `crates/stats-code/tests/proptest/effect_props.rs` and `tests/proptest/two_by_two_props.rs`
    - _Requirements: G4.3, 9.1, 9.2, 9.4_

  - [ ]* 9.5 CLI integration test
    - _Requirements: G4.2_

- [ ] 10. Implement rate standardization — direct and SMR (Req 10)
  - [x] 10.1 Implement direct standardization and SMR with Byar CI in `stats/epi/standardize.rs`
    - Built-in standard populations: `who_world_2000`, `china_census_2010`, `segi_world`
    - Resolve user CSV path data-relative if `--standard-pop` is not a built-in name
    - Exclude strata with zero person-time, with warning
    - Wire `EpiStatsCommand::Standardize` into `cli.rs` and `handlers/stats.rs`
    - _Requirements: 10.1, 10.2, 10.3, 10.4, 10.5_

  - [ ] 10.2 R gold-reference fixtures and unit tests
    - _Requirements: G4.1, G5.1_

  - [ ]* 10.3 CLI integration test
    - _Requirements: G4.2_

- [ ] 11. Implement attributable risk measures — AR, AR%, PAR, PAR% (Req 19)
  - [x] 11.1 Implement AR/AR%/PAR/PAR% in `stats/epi/attributable.rs`
    - Variance via delta method; emit "protective association detected" warning when R_u > R_e
    - Wire `EpiStatsCommand::Attributable` into `cli.rs` and `handlers/stats.rs`
    - _Requirements: 19.1, 19.2, 19.3, 19.4, 19.5_

  - [ ] 11.2 R gold-reference fixtures and unit tests
    - _Requirements: G4.1, G5.1_

  - [ ]* 11.3 CLI integration test
    - _Requirements: G4.2_

- [ ] 12. Implement normality tests — Shapiro-Wilk + Lilliefors KS (Req 16)
  - [x] 12.1 Implement Lilliefors-corrected K-S, skewness/kurtosis in `stats/diagnostic/normality.rs`
    - _Requirements: 16.2, 16.3, 16.5_

  - [x] 12.2 Implement Shapiro-Wilk W via Royston (1992) coefficient series
    - Populate `shapiro_w` for n ≥ 3; if n > 5000, set `shapiro_p = None`, `shapiro_p_unreliable = true`, and append the overpowered warning
    - Wire `DiagnosticCommand::Normality` into `cli.rs` and `handlers/stats.rs`
    - _Requirements: 16.1, 16.4_

  - [ ] 12.3 SciPy cross-check fixtures and unit tests
    - _Requirements: G4.1, G5.2_

  - [ ]* 12.4 CLI integration test
    - _Requirements: G4.2_

- [ ] 13. Implement homogeneity-of-variance tests — Levene + Bartlett (Req 17)
  - [x] 13.1 Implement Levene (median form / Brown-Forsythe) and Bartlett in `stats/diagnostic/variance.rs`
    - Reject when any group has n < 2
    - Wire `DiagnosticCommand::Variance` into `cli.rs` and `handlers/stats.rs`
    - _Requirements: 17.1, 17.2, 17.3, 17.4, 17.5_

  - [ ] 13.2 R gold-reference fixtures and unit tests
    - _Requirements: G4.1, G5.1_

  - [ ]* 13.3 CLI integration test
    - _Requirements: G4.2_

- [ ] 14. Implement actuarial life-table survival (Req 18)
  - [x] 14.1 Implement `life_table_csv(...)` in `stats/survival/lifetable.rs`
    - Support `--input-format individual` (with `--time`, `--status`, `--intervals` spec like `0,1,2,5,10` or `width=1`) and `--input-format grouped` (with `--entering`, `--events`, `--withdrawals`)
    - Pre-binning logic in `bin_individuals`
    - Greenwood SE and 95% CI per cumulative survival
    - Wire `StatsSurvivalCommand::Lifetable` into `cli.rs` and `handlers/stats.rs`
    - _Requirements: 18.1, 18.2, 18.3, 18.4_

  - [ ] 14.2 R gold-reference fixtures and unit tests (compare with `survival::survfit` actuarial)
    - _Requirements: G4.1, G5.1_

  - [ ]* 14.3 CLI integration test
    - _Requirements: G4.2_

- [ ] 15. Wire Phase 1 dispatch, renderers, and `analysis.yaml` step registration
  - [ ] 15.1 Add text renderers for every Phase 1 result in `render/stats.rs`
    - One `render_*_text` function per `*Result`; JSON output remains via `serde_json::to_string_pretty`
    - _Requirements: G1.4_

  - [ ] 15.2 Register HIGH-tier step names in `schema/contract.rs`
    - Add: `ttest.paired`, `ttest.one_sample`, `anova.oneway`, `nonparam.cochran_armitage`, `nonparam.mcnemar`, `nonparam.wilcoxon`, `nonparam.mannwhitney`, `correlation`, `epi.or_rr`, `epi.standardize`, `epi.attributable`, `diagnostic.normality`, `diagnostic.variance`, `survival.lifetable`
    - _Requirements: G3.1, G3.2_

  - [ ] 15.3 Add HIGH-tier `examples/` CSVs and `analysis.example.yaml` snippets
    - One CSV + one analysis-step snippet per HIGH method per requirements §G6
    - _Requirements: G6_

- [ ] 16. **Checkpoint — Phase 1 complete**
  - Ensure all tests pass, ask the user if questions arise.

### Phase 2 — MEDIUM-Priority Methods (Req 11, 12, 13, 20, 21, 22, 23, 24, 30)

- [ ] 17. Implement post-hoc multiple comparisons — Bonferroni + Tukey HSD (Req 11)
  - [x] 17.1 Implement Bonferroni pairwise comparisons in `stats/anova.rs`
    - Append `posthoc_csv(...)` returning `PosthocResult` with `method = "bonferroni"`
    - Wire `AnovaCommand::Posthoc` into `cli.rs` and `handlers/stats.rs`
    - _Requirements: 11.1, 11.3, 11.4_

  - [ ] 17.2 Implement Tukey HSD with studentized-range CDF
    - Use `math/distributions::studentized_range_p`
    - _Requirements: 11.2, 11.3, 11.4_

  - [ ] 17.3 Tukey HSD validation harness (R-fixture gate)
    - File: `crates/stats-code/tests/tukey_validation.rs`
    - Assert `max |p_rust − p_R_ptukey| < 1e-3` for `df ∈ [2, 200]`, `k ∈ [2, 20]`; if it fails, the handler must error with `requires --engine python`
    - _Requirements: 11.2, design Tukey HSD Validation Gate_

  - [ ]* 17.4 CLI integration test for posthoc
    - _Requirements: G4.2_

- [ ] 18. Implement repeated-measures ANOVA (Req 12)
  - [ ] 18.1 Implement `repeated_measures_anova_csv(...)` in `stats/anova.rs`
    - Subject × time matrix; partition SS_total = SS_subject + SS_time + SS_error
    - Mauchly's W with Greenhouse-Geisser ε and Huynh-Feldt ε corrections
    - Drop subjects with any missing time-point measurement; report exclusion count
    - Wire `AnovaCommand::Repeated` into `cli.rs` and `handlers/stats.rs`
    - _Requirements: 12.1, 12.2, 12.3, 12.4_

  - [ ] 18.2 R gold-reference fixtures and unit tests (`car::Anova` repeated-measures output)
    - _Requirements: G4.1, G5.1_

  - [ ]* 18.3 CLI integration test
    - _Requirements: G4.2_

- [ ] 19. Implement Poisson regression (Req 13)
  - [x] 19.1 Implement `poisson_glm_csv(...)` in `stats/model/poisson.rs` reusing `math/glm.rs`
    - Mutually-exclusive `--offset <col>` (already on log scale) vs `--exposure <col>` (raw, internally log-transformed); record `offset_kind` in result
    - 25-iteration cap with descriptive non-convergence error
    - Wire `StatsModelCommand::Poisson` into `cli.rs` and `handlers/stats.rs`
    - _Requirements: 13.1, 13.2, 13.3, 13.4, 13.5_

  - [ ] 19.2 R + statsmodels cross-checked fixtures and unit tests
    - _Requirements: G4.1, G5.1, G5.2_

  - [ ]* 19.3 CLI integration test
    - _Requirements: G4.2_

- [ ] 20. Implement dose-response analysis (Req 20)
  - [x] 20.1 Implement `dose_response_csv(...)` in `stats/epi/doseresponse.rs`
    - Reuse `math/glm.rs` Poisson IRLS with `log(person_time)` offset
    - Trend χ² + linearity-departure χ² (df = k - 2)
    - Wire `EpiStatsCommand::DoseResponse` into `cli.rs` and `handlers/stats.rs`
    - _Requirements: 20.1, 20.2, 20.3, 20.4_

  - [ ] 20.2 R gold-reference fixtures and unit tests
    - _Requirements: G4.1, G5.1_

  - [ ]* 20.3 CLI integration test
    - _Requirements: G4.2_

- [ ] 21. Implement meta-analysis — fixed-effect + DerSimonian-Laird random (Req 21)
  - [x] 21.1 Implement `meta_analysis_csv(...)` in `stats/meta.rs`
    - Inverse-variance fixed effect, DL random effect, Q, I², τ²
    - Per-study fixed/random weights, forest/funnel plot points
    - Reject when fewer than 2 studies provided
    - Wire `StatsCommand::Meta(MetaArgs)` into `cli.rs` and `handlers/stats.rs`
    - _Requirements: 21.1, 21.2, 21.3, 21.4, 21.5, 21.6_

  - [ ] 21.2 R `meta` package gold-reference fixtures and unit tests
    - _Requirements: G4.1, G5.1_

  - [ ]* 21.3 CLI integration test
    - _Requirements: G4.2_

- [ ] 22. Implement Cohen's kappa and weighted kappa (Req 22)
  - [x] 22.1 Implement kappa in `stats/agreement.rs`
    - Build c×c agreement matrix from union of both raters' categories (per design clarification — disjoint sets are filled with zeros, NOT an error; supersedes Req 22.4 wording)
    - Linear and quadratic Fleiss-Cohen weights
    - SE and 95% CI
    - Wire `AgreementCommand::Kappa` into `cli.rs` and `handlers/stats.rs`
    - _Requirements: 22.1, 22.2, 22.3, 22.4, 22.5_

  - [ ] 22.2 R `psych::cohen.kappa` fixtures and unit tests
    - _Requirements: G4.1, G5.1_

  - [ ]* 22.3 CLI integration test
    - _Requirements: G4.2_

- [ ] 23. Implement Bland-Altman analysis (Req 23)
  - [x] 23.1 Implement Bland-Altman in `stats/agreement.rs`
    - Bias, SD of differences, 95% LOA (mean ± 1.96·SD)
    - 95% CI for bias and each LOA via t_{n-1}; per-point `(mean, diff)` in result for plotting
    - Warn when n < 10
    - Wire `AgreementCommand::BlandAltman` into `cli.rs` and `handlers/stats.rs`
    - _Requirements: 23.1, 23.2, 23.3, 23.4, 23.5_

  - [ ] 23.2 R `BlandAltmanLeh` fixtures and unit tests
    - _Requirements: G4.1, G5.1_

  - [ ]* 23.3 CLI integration test
    - _Requirements: G4.2_

- [ ] 24. Implement principal component analysis (Req 24)
  - [ ] 24.1 Implement PCA in `stats/multivariate/pca.rs`
    - Correlation-matrix default, covariance-matrix opt-in via `--matrix covariance`
    - Jacobi eigen-decomposition from `math/linalg.rs`
    - KMO and Bartlett's sphericity tests
    - Drop zero-variance variables with warning
    - Wire `MultivariateCommand::Pca` into `cli.rs` and `handlers/stats.rs`
    - _Requirements: 24.1, 24.2, 24.3, 24.4, 24.5_

  - [ ] 24.2 R `psych::principal` cross-checked fixtures and unit tests
    - _Requirements: G4.1, G5.1_

  - [ ]* 24.3 CLI integration test
    - _Requirements: G4.2_

- [ ] 25. Implement log-rank sample-size calculator (Req 30)
  - [x] 25.1 Implement Schoenfeld required-events + sample size in `stats/sample_size.rs`
    - Inputs: median survivals per arm, accrual T_a, follow-up T_f, target power, allocation ratio, optional dropout rate
    - Wire `SampleSizeCommand::LogRank` into `cli.rs` and `handlers/stats.rs`; reuse existing `PowerResult` with `method = "log_rank"`
    - _Requirements: 30.1–30.x (Sample Size for Log-Rank, see requirements §Requirement 30)_

  - [ ] 25.2 Cross-check against `gsDesign` / `survSNP` fixtures and unit tests
    - _Requirements: G4.1, G5.1_

  - [ ]* 25.3 CLI integration test
    - _Requirements: G4.2_

- [ ] 26. Wire Phase 2 dispatch, renderers, and `analysis.yaml` step registration
  - [ ] 26.1 Add text renderers for Phase 2 result types in `render/stats.rs`
    - _Requirements: G1.4_

  - [ ] 26.2 Register MEDIUM-tier step names in `schema/contract.rs`
    - Add: `anova.posthoc`, `anova.repeated`, `model.poisson`, `epi.dose_response`, `meta`, `agreement.kappa`, `agreement.bland_altman`, `multivariate.pca`, `sample_size.log_rank`
    - _Requirements: G3.1, G3.2_

  - [ ] 26.3 Update README usage table for MEDIUM methods
    - _Requirements: G6_

- [ ] 27. **Checkpoint — Phase 2 complete**
  - Ensure all tests pass, ask the user if questions arise.

### Phase 3 — LOW-Priority Methods (Req 14, 15, 25, 26, 27, 28, 29)

- [ ] 28. Add Python bridge scaffolding for LOW-tier methods
  - [x] 28.1 Add `scripts/python/ordinal_logit.py`, `multinomial_logit.py`, `lda.py`, `mixed_effects.py`, `competing_risks.py` modules
    - Each accepts a JSON request from `bridge_runner.py` and returns a JSON document deserializable into the corresponding `*Result` struct
    - _Requirements: G1.6, G7_

  - [ ] 28.2 Define `UnsupportedEngine` Rust stub helpers in `stats/model/`, `stats/multivariate/lda.rs`, `stats/mixed.rs`, `stats/survival/competing.rs`
    - Each Rust path returns the canonical `Err("This method requires --engine python. Native Rust implementation is planned but not yet available.")` (string is unit-tested for stability)
    - _Requirements: G1.6, G7_

- [ ] 29. Implement ordinal logistic regression (Req 14)
  - [x] 29.1 Wire `multilogit::ordinal_logit_csv(...)` Python bridge in `stats/model/ordinal.rs`
    - Use `statsmodels.miscmodels.ordinal_model.OrderedModel`
    - Brant test via per-cutpoint binary logits
    - Reject when outcome has fewer than 3 ordered levels
    - Wire `StatsModelCommand::Ordinal` into `cli.rs` and `handlers/stats.rs`
    - _Requirements: 14.1, 14.2, 14.3, 14.4, 14.5_

  - [ ] 29.2 Bridge fixtures and unit tests against statsmodels gold output
    - _Requirements: G4.1, G5.2_

  - [ ]* 29.3 CLI integration test (with `--engine python`)
    - _Requirements: G4.2_

- [ ] 30. Implement multinomial logistic regression (Req 15)
  - [x] 30.1 Wire `multinomial_logit_csv(...)` Python bridge in `stats/model/multinomial.rs`
    - Use `statsmodels.discrete.discrete_model.MNLogit`; honor `--reference`
    - Reject when outcome has fewer than 3 categories
    - Wire `StatsModelCommand::Multinomial` into `cli.rs` and `handlers/stats.rs`
    - _Requirements: 15.1, 15.2, 15.3, 15.4, 15.5_

  - [ ] 30.2 Bridge fixtures and unit tests
    - _Requirements: G4.1, G5.2_

  - [ ]* 30.3 CLI integration test (with `--engine python`)
    - _Requirements: G4.2_

- [ ] 31. Implement linear discriminant analysis (Req 25)
  - [x] 31.1 Wire `lda_csv(...)` Python bridge in `stats/multivariate/lda.rs`
    - Use `sklearn.discriminant_analysis.LinearDiscriminantAnalysis`
    - Wilks' Λ, leave-one-out confusion matrix, standardized coefficients, group centroids
    - Reject when any group has fewer observations than the number of predictors
    - Wire `MultivariateCommand::Lda` into `cli.rs` and `handlers/stats.rs`
    - _Requirements: 25.1, 25.2, 25.3, 25.4, 25.5_

  - [ ] 31.2 Bridge fixtures and unit tests
    - _Requirements: G4.1, G5.2_

  - [ ]* 31.3 CLI integration test
    - _Requirements: G4.2_

- [ ] 32. Implement cluster analysis — k-means + Ward hierarchical (Req 26)
  - [ ] 32.1 Implement Lloyd's k-means with k-means++ init and 10 restarts in `stats/multivariate/cluster.rs`
    - Silhouette per observation and average
    - Drop zero-variance variables with warning; require `--seed` for reproducibility
    - Wire `MultivariateCommand::Cluster` (`--method kmeans`) into `cli.rs` and `handlers/stats.rs`
    - _Requirements: 26.1, 26.2, 26.4, 26.5_

  - [ ] 32.2 Implement Ward agglomerative hierarchical clustering via Lance-Williams update
    - Report merge distances and order; activated by `--method hierarchical`
    - _Requirements: 26.3, 26.5_

  - [ ] 32.3 R `stats::kmeans` and `stats::hclust(method="ward.D2")` cross-checked fixtures and unit tests
    - _Requirements: G4.1, G5.1_

  - [ ]* 32.4 CLI integration test
    - _Requirements: G4.2_

- [ ] 33. Implement linear mixed-effects models (Req 27)
  - [x] 33.1 Wire `mixed_lmm_csv(...)` Python bridge in `stats/mixed.rs`
    - Use `statsmodels.MixedLM` (REML); compute ICC = σ²_random / (σ²_random + σ²_residual)
    - 100-iteration cap with descriptive non-convergence diagnostics
    - Wire `StatsCommand::Mixed(MixedArgs)` into `cli.rs` and `handlers/stats.rs`
    - _Requirements: 27.1, 27.2, 27.3, 27.4_

  - [ ] 33.2 Bridge fixtures and unit tests against `lme4::lmer` reference values
    - _Requirements: G4.1, G5.2_

  - [ ]* 33.3 CLI integration test
    - _Requirements: G4.2_

- [ ] 34. Implement propensity score matching (Req 28)
  - [x] 34.1 Implement greedy 1:k no-replacement matching in `stats/psm.rs`
    - Reuse existing `logistic_csv` to estimate propensity scores; compute logit
    - Caliper c = `--caliper` × sd(logit); default ratio 1
    - Standardized mean differences (SMD) before/after per covariate
    - Require `--seed`; default matched-output path is `<artifacts_dir>/psm_matched.csv` unless `--output` overrides
    - Wire `StatsCommand::Psm(PsmArgs)` into `cli.rs` and `handlers/stats.rs`
    - _Requirements: 28.1, 28.2, 28.3, 28.4, 28.5_

  - [ ] 34.2 Cross-check against R `MatchIt` greedy nearest-neighbor fixtures and unit tests
    - _Requirements: G4.1, G5.1_

  - [ ]* 34.3 CLI integration test
    - _Requirements: G4.2_

- [ ] 35. Implement competing risks analysis (Req 29)
  - [x] 35.1 Implement cause-specific Cox per cause + CIF point estimates in `stats/survival/competing.rs`
    - Reuse existing `cox.rs` for cause-specific hazards
    - Aalen-Johansen point estimates of CIF (Rust); `--point-estimate-only` returns these without variance
    - _Requirements: 29.1, 29.2, 29.4, 29.5_

  - [ ] 35.2 Wire Python bridge for full CIF variance and Gray's test
    - Default engine for `--all-causes` and Gray's test is Python
    - Wire `StatsSurvivalCommand::Competing` into `cli.rs` and `handlers/stats.rs`
    - _Requirements: 29.2, 29.3_

  - [ ] 35.3 Cross-check against `lifelines.CompetingRisksFitter` / R `cmprsk` fixtures and unit tests
    - _Requirements: G4.1, G5.1, G5.2_

  - [ ]* 35.4 CLI integration test
    - _Requirements: G4.2_

- [ ] 36. Wire Phase 3 dispatch, renderers, and `analysis.yaml` step registration
  - [ ] 36.1 Add text renderers for Phase 3 result types in `render/stats.rs`
    - _Requirements: G1.4_

  - [ ] 36.2 Register LOW-tier step names in `schema/contract.rs`
    - Add: `model.ordinal`, `model.multinomial`, `multivariate.lda`, `multivariate.cluster`, `mixed`, `psm`, `survival.competing`
    - _Requirements: G3.1, G3.2_

  - [ ] 36.3 Update README usage table for LOW methods
    - _Requirements: G6_

- [ ] 37. **Final checkpoint — full feature complete**
  - Ensure all tests pass, ask the user if questions arise.

## Notes

- Tasks marked with `*` (proptest sub-tasks and CLI integration sub-tasks) are optional per the workflow — they implement the §G4 testing requirements and can be skipped for a faster MVP, but core implementation tasks are never optional.
- Every leaf task references either specific requirement clauses (e.g., 1.1, 9.4) or global criteria (G1–G7) for traceability.
- Phase boundaries are checkpoints: do not start the next phase until prior phase tests are green.
- Engine semantics from `design.md` §Engine Allocation (Final) govern all dispatch behavior. R engine returns `R engine not yet implemented for <method>` everywhere; this string is asserted by unit tests for stability.
- Result-struct serialization, prelude fields, and warning vocabulary follow `design.md` §Data Models verbatim — no ad-hoc field names.
- Property tests cover only the five invariants explicitly listed in requirements §G4.3: paired-t sign-flip symmetry / df = n − 1 (task 4.3); ANOVA SS decomposition (task 5.3); OR/RR continuity-correction safety (task 9.4); 2×2 zero-cell / single-row edge cases (task 9.4); missing-value row-permutation invariance (task 2.4).

## Task Dependency Graph

```json
{
  "waves": [
    { "id": 0, "tasks": ["1.1", "2.1", "3.1"] },
    { "id": 1, "tasks": ["1.2", "2.2", "2.3"] },
    { "id": 2, "tasks": ["2.4"] },
    { "id": 3, "tasks": ["4.1", "5.1", "6.1", "8.1", "9.1", "10.1", "11.1", "12.1", "13.1", "14.1"] },
    { "id": 4, "tasks": ["7.1", "9.2", "12.2"] },
    { "id": 5, "tasks": ["7.2"] },
    { "id": 6, "tasks": ["7.3"] },
    { "id": 7, "tasks": ["4.2", "5.2", "6.2", "7.4", "8.2", "9.3", "10.2", "11.2", "12.3", "13.2", "14.2"] },
    { "id": 8, "tasks": ["4.3", "5.3", "9.4"] },
    { "id": 9, "tasks": ["4.4", "5.4", "6.3", "7.5", "8.3", "9.5", "10.3", "11.3", "12.4", "13.3", "14.3"] },
    { "id": 10, "tasks": ["15.1", "15.2", "15.3"] },
    { "id": 11, "tasks": ["17.1", "18.1", "19.1", "20.1", "21.1", "22.1", "23.1", "24.1", "25.1"] },
    { "id": 12, "tasks": ["17.2"] },
    { "id": 13, "tasks": ["17.3", "18.2", "19.2", "20.2", "21.2", "22.2", "23.2", "24.2", "25.2"] },
    { "id": 14, "tasks": ["17.4", "18.3", "19.3", "20.3", "21.3", "22.3", "23.3", "24.3", "25.3"] },
    { "id": 15, "tasks": ["26.1", "26.2", "26.3"] },
    { "id": 16, "tasks": ["28.1", "28.2"] },
    { "id": 17, "tasks": ["29.1", "30.1", "31.1", "32.1", "33.1", "34.1", "35.1"] },
    { "id": 18, "tasks": ["32.2", "35.2"] },
    { "id": 19, "tasks": ["29.2", "30.2", "31.2", "32.3", "33.2", "34.2", "35.3"] },
    { "id": 20, "tasks": ["29.3", "30.3", "31.3", "32.4", "33.3", "34.3", "35.4"] },
    { "id": 21, "tasks": ["36.1", "36.2", "36.3"] }
  ]
}
```
