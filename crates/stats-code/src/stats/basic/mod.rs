mod agreement;
mod anova;
mod attributable;
mod cluster;
mod common;
mod doseresponse;
mod effect;
mod lifetable;
mod meta;
mod nonparam;
mod normality;
mod pca;
mod poisson;
mod psm;
mod sample_size;
mod standardize;
mod trend;
mod variance;

pub(crate) use agreement::{bland_altman_csv, kappa_csv};
pub(crate) use anova::{oneway_anova_csv, posthoc_csv, rbd_anova_csv, repeated_anova_csv};
pub(crate) use attributable::attributable_csv;
pub(crate) use cluster::cluster_csv;
pub(crate) use doseresponse::dose_response_csv;
pub(crate) use effect::or_rr_csv;
pub(crate) use lifetable::{lifetable_csv, lifetable_individual_csv};
pub(crate) use meta::meta_analysis_csv;
pub(crate) use nonparam::{mann_whitney_csv, mcnemar_csv, wilcoxon_csv};
pub(crate) use normality::normality_csv;
pub(crate) use pca::pca_csv;
pub(crate) use poisson::poisson_glm_csv;
pub(crate) use psm::psm_csv;
pub(crate) use sample_size::logrank_sample_size;
pub(crate) use standardize::standardize_csv;
pub(crate) use trend::cochran_armitage_csv;
pub(crate) use variance::variance_homogeneity_csv;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::NaStrategy;
    use crate::schema::TwoByTwoCells;
    use serde_json::Value;

    fn approx(actual: f64, expected: f64, tol: f64) {
        assert!(
            (actual - expected).abs() <= tol,
            "expected {expected}, got {actual}"
        );
    }

    fn load_fixture(relative_path: &str) -> Value {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(relative_path);
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display()));
        serde_json::from_str(&text)
            .unwrap_or_else(|err| panic!("failed to parse {}: {err}", path.display()))
    }

    fn expected_f64(fixture: &Value, key: &str) -> f64 {
        fixture["expected"][key]
            .as_f64()
            .unwrap_or_else(|| panic!("missing expected.{key}"))
    }

    fn expected_usize(fixture: &Value, key: &str) -> usize {
        fixture["expected"][key]
            .as_u64()
            .unwrap_or_else(|| panic!("missing expected.{key}")) as usize
    }

    fn rows_from_fixture(
        fixture: &Value,
        columns: &[&str],
    ) -> (Vec<csv::StringRecord>, csv::StringRecord) {
        let rows = fixture["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| {
                let fields = columns
                    .iter()
                    .map(|column| match &row[*column] {
                        Value::String(value) => value.clone(),
                        Value::Number(value) => value.to_string(),
                        other => panic!("unsupported fixture cell for {column}: {other}"),
                    })
                    .collect::<Vec<_>>();
                csv::StringRecord::from(fields)
            })
            .collect();
        (rows, csv::StringRecord::from(columns.to_vec()))
    }

    fn cochran_rows_from_fixture(
        fixture: &Value,
    ) -> (Vec<csv::StringRecord>, csv::StringRecord, Vec<f64>) {
        let mut rows = Vec::new();
        let mut scores = Vec::new();
        for summary in fixture["rows_summary"].as_array().unwrap() {
            let exposure = summary["exposure"].as_str().unwrap();
            let score = summary["score"].as_f64().unwrap();
            let n = summary["n"].as_u64().unwrap() as usize;
            let events = summary["events"].as_u64().unwrap() as usize;
            scores.push(score);
            for i in 0..n {
                let outcome = if i < events { "1" } else { "0" };
                rows.push(csv::StringRecord::from(vec![
                    exposure.to_string(),
                    outcome.to_string(),
                ]));
            }
        }
        (
            rows,
            csv::StringRecord::from(vec!["exposure", "outcome"]),
            scores,
        )
    }

    fn push_2x2_rows(
        rows: &mut Vec<csv::StringRecord>,
        stratum: &str,
        exposed: &str,
        outcome: &str,
        n: usize,
    ) {
        for _ in 0..n {
            rows.push(csv::StringRecord::from(vec![exposed, outcome, stratum]));
        }
    }

    fn or_rr_rows_from_fixture(
        fixture: &Value,
    ) -> (Vec<csv::StringRecord>, csv::StringRecord, Vec<String>) {
        let summaries = fixture["rows_summary"].as_array().unwrap();
        let has_stratum = summaries
            .iter()
            .any(|summary| summary.get("stratum").is_some());
        let mut rows = Vec::new();
        for summary in summaries {
            let exposure = summary["exposure"].as_str().unwrap();
            let outcome = summary["outcome"].as_str().unwrap();
            let n = summary["n"].as_u64().unwrap() as usize;
            let stratum = summary
                .get("stratum")
                .and_then(Value::as_str)
                .unwrap_or("__crude__");
            for _ in 0..n {
                if has_stratum {
                    rows.push(csv::StringRecord::from(vec![exposure, outcome, stratum]));
                } else {
                    rows.push(csv::StringRecord::from(vec![exposure, outcome]));
                }
            }
        }
        if has_stratum {
            (
                rows,
                csv::StringRecord::from(vec!["exposure", "outcome", "stratum"]),
                vec!["stratum".to_string()],
            )
        } else {
            (
                rows,
                csv::StringRecord::from(vec!["exposure", "outcome"]),
                vec![],
            )
        }
    }

    fn assert_cells(actual: &TwoByTwoCells, expected: &Value) {
        approx(actual.a, expected["a"].as_f64().unwrap(), 1e-12);
        approx(actual.b, expected["b"].as_f64().unwrap(), 1e-12);
        approx(actual.c, expected["c"].as_f64().unwrap(), 1e-12);
        approx(actual.d, expected["d"].as_f64().unwrap(), 1e-12);
    }

    #[test]
    fn oneway_anova_matches_gold_fixture() {
        let fixture = load_fixture("tests/fixtures/r/anova_oneway.json");
        let (rows, headers) = rows_from_fixture(&fixture, &["group", "value"]);

        let result = oneway_anova_csv(&rows, &headers, "value", "group", NaStrategy::Drop).unwrap();

        assert_eq!(result.df_between, expected_usize(&fixture, "df_between"));
        assert_eq!(result.df_within, expected_usize(&fixture, "df_within"));
        approx(
            result.overall_mean,
            expected_f64(&fixture, "overall_mean"),
            1e-12,
        );
        approx(
            result.ss_between,
            expected_f64(&fixture, "ss_between"),
            1e-12,
        );
        approx(result.ss_within, expected_f64(&fixture, "ss_within"), 1e-12);
        approx(result.ss_total, expected_f64(&fixture, "ss_total"), 1e-12);
        approx(
            result.ms_between,
            expected_f64(&fixture, "ms_between"),
            1e-12,
        );
        approx(result.ms_within, expected_f64(&fixture, "ms_within"), 1e-12);
        approx(
            result.f_statistic,
            expected_f64(&fixture, "f_statistic"),
            1e-10,
        );
        approx(result.p_value, expected_f64(&fixture, "p_value"), 1e-10);

        let expected_groups = fixture["expected"]["groups"].as_array().unwrap();
        assert_eq!(result.groups.len(), expected_groups.len());
        for expected in expected_groups {
            let label = expected["group"].as_str().unwrap();
            let actual = result
                .groups
                .iter()
                .find(|group| group.group == label)
                .unwrap_or_else(|| panic!("missing group {label}"));
            assert_eq!(actual.n, expected["n"].as_u64().unwrap() as usize);
            approx(actual.mean, expected["mean"].as_f64().unwrap(), 1e-12);
            approx(actual.sd, expected["sd"].as_f64().unwrap(), 1e-12);
        }
    }

    #[test]
    fn rbd_anova_matches_gold_fixture() {
        let fixture = load_fixture("tests/fixtures/r/anova_rbd.json");
        let (rows, headers) = rows_from_fixture(&fixture, &["group", "block", "value"]);

        let result =
            rbd_anova_csv(&rows, &headers, "value", "group", "block", NaStrategy::Drop).unwrap();

        assert_eq!(
            result.treatment_df1,
            expected_usize(&fixture, "treatment_df1")
        );
        assert_eq!(
            result.treatment_df2,
            expected_usize(&fixture, "treatment_df2")
        );
        assert_eq!(result.block_df1, expected_usize(&fixture, "block_df1"));
        assert_eq!(result.block_df2, expected_usize(&fixture, "block_df2"));
        approx(
            result.treatment_f,
            expected_f64(&fixture, "treatment_f"),
            1e-8,
        );
        approx(
            result.treatment_p,
            expected_f64(&fixture, "treatment_p"),
            1e-10,
        );
        approx(result.block_f, expected_f64(&fixture, "block_f"), 1e-8);
        approx(result.block_p, expected_f64(&fixture, "block_p"), 1e-10);
        approx(result.error_ms, expected_f64(&fixture, "error_ms"), 1e-12);
    }

    #[test]
    fn oneway_anova_sparse_group_reports_group_label() {
        let headers = csv::StringRecord::from(vec!["group", "value"]);
        let rows = vec![
            csv::StringRecord::from(vec!["A", "12"]),
            csv::StringRecord::from(vec!["A", "14"]),
            csv::StringRecord::from(vec!["B", "18"]),
        ];

        let err =
            oneway_anova_csv(&rows, &headers, "value", "group", NaStrategy::Drop).unwrap_err();
        assert!(err.contains("group `B` has 1"), "err={err}");
    }

    #[test]
    fn cochran_armitage_matches_r_fixture() {
        let fixture = load_fixture("tests/fixtures/r/trend_cochran_armitage.json");
        let (rows, headers, scores) = cochran_rows_from_fixture(&fixture);

        let result = cochran_armitage_csv(
            &rows,
            &headers,
            "exposure",
            "outcome",
            &scores,
            NaStrategy::Drop,
        )
        .unwrap();

        assert_eq!(result.n_used, expected_usize(&fixture, "n_used"));
        approx(
            result.trend_statistic,
            expected_f64(&fixture, "trend_statistic"),
            1e-12,
        );
        approx(result.p_value, expected_f64(&fixture, "p_value"), 2e-7);

        let expected_categories = fixture["expected"]["categories"].as_array().unwrap();
        assert_eq!(result.categories.len(), expected_categories.len());
        for expected in expected_categories {
            let category = expected["category"].as_str().unwrap();
            let actual = result
                .categories
                .iter()
                .find(|item| item.category == category)
                .unwrap_or_else(|| panic!("missing category {category}"));
            assert_eq!(actual.n, expected["n"].as_u64().unwrap() as usize);
            assert_eq!(actual.events, expected["events"].as_u64().unwrap() as usize);
            approx(actual.score, expected["score"].as_f64().unwrap(), 1e-12);
            approx(
                actual.proportion,
                expected["proportion"].as_f64().unwrap(),
                1e-12,
            );
        }
    }

    #[test]
    fn mcnemar_matches_r_fixture() {
        let fixture = load_fixture("tests/fixtures/r/nonparam_mcnemar.json");
        let (rows, headers) = rows_from_fixture(&fixture, &["var1", "var2"]);

        let result = mcnemar_csv(&rows, &headers, "var1", "var2", 25, NaStrategy::Drop).unwrap();

        assert_eq!(result.b, expected_usize(&fixture, "b"));
        assert_eq!(result.c, expected_usize(&fixture, "c"));
        assert_eq!(
            result.n_concordant,
            expected_usize(&fixture, "n_concordant")
        );
        approx(
            result.chi_square,
            expected_f64(&fixture, "chi_square"),
            1e-12,
        );
        approx(result.p_value, expected_f64(&fixture, "p_value"), 2e-7);
        approx(
            result.exact_p_value.unwrap(),
            expected_f64(&fixture, "exact_p_value"),
            1e-12,
        );
    }

    #[test]
    fn wilcoxon_matches_r_fixture() {
        let fixture = load_fixture("tests/fixtures/r/nonparam_wilcoxon.json");
        let (rows, headers) = rows_from_fixture(&fixture, &["before", "after"]);

        let result = wilcoxon_csv(&rows, &headers, "before", "after", NaStrategy::Drop).unwrap();

        approx(result.w_plus, expected_f64(&fixture, "w_plus"), 1e-12);
        approx(
            result.expected_w,
            expected_f64(&fixture, "expected_w"),
            1e-12,
        );
        approx(
            result.variance_w,
            expected_f64(&fixture, "variance_w"),
            1e-12,
        );
        approx(
            result.z_statistic,
            expected_f64(&fixture, "z_statistic"),
            1e-12,
        );
        approx(result.p_value, expected_f64(&fixture, "p_value"), 2e-7);
        assert_eq!(
            result.n_zero_pairs_excluded,
            expected_usize(&fixture, "n_zero_pairs_excluded")
        );
        assert_eq!(
            result.n_ties_corrected,
            expected_usize(&fixture, "n_ties_corrected")
        );
    }

    #[test]
    fn mann_whitney_matches_r_fixture() {
        let fixture = load_fixture("tests/fixtures/r/nonparam_mannwhitney.json");
        let (rows, headers) = rows_from_fixture(&fixture, &["group", "value"]);

        let result = mann_whitney_csv(&rows, &headers, "value", "group", NaStrategy::Drop).unwrap();

        assert_eq!(
            result.group_a_label,
            fixture["expected"]["group_a_label"].as_str().unwrap()
        );
        assert_eq!(
            result.group_b_label,
            fixture["expected"]["group_b_label"].as_str().unwrap()
        );
        assert_eq!(result.n_a, expected_usize(&fixture, "n_a"));
        assert_eq!(result.n_b, expected_usize(&fixture, "n_b"));
        approx(result.median_a, expected_f64(&fixture, "median_a"), 1e-12);
        approx(result.median_b, expected_f64(&fixture, "median_b"), 1e-12);
        approx(
            result.u_statistic,
            expected_f64(&fixture, "u_statistic"),
            1e-12,
        );
        approx(
            result.z_statistic,
            expected_f64(&fixture, "z_statistic"),
            1e-12,
        );
        approx(result.p_value, expected_f64(&fixture, "p_value"), 2e-7);
    }

    #[test]
    fn standardization_direct_matches_r_fixture() {
        let fixture = load_fixture("tests/fixtures/r/standardization_direct.json");
        let (rows, headers) = rows_from_fixture(&fixture, &["age_group", "events", "person_time"]);
        let standard_pop = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(fixture["standard_population"].as_str().unwrap());
        let standard_pop = standard_pop.to_string_lossy();

        let result = standardize_csv(
            &rows,
            &headers,
            "direct",
            "events",
            "person_time",
            "age_group",
            &standard_pop,
            0.05,
            NaStrategy::Drop,
        )
        .unwrap();

        assert_eq!(
            result.method,
            fixture["expected"]["method"].as_str().unwrap()
        );
        assert_eq!(result.n_used, expected_usize(&fixture, "n_used"));
        approx(
            result.standardized_rate.unwrap(),
            expected_f64(&fixture, "standardized_rate"),
            1e-12,
        );
        approx(
            result.direct_ci_lower.unwrap(),
            expected_f64(&fixture, "direct_ci_lower"),
            1e-10,
        );
        approx(
            result.direct_ci_upper.unwrap(),
            expected_f64(&fixture, "direct_ci_upper"),
            1e-10,
        );

        let expected_strata = fixture["expected"]["strata"].as_array().unwrap();
        assert_eq!(result.strata.len(), expected_strata.len());
        for expected in expected_strata {
            let age_group = expected["age_group"].as_str().unwrap();
            let actual = result
                .strata
                .iter()
                .find(|stratum| stratum.age_group == age_group)
                .unwrap_or_else(|| panic!("missing stratum {age_group}"));
            approx(
                actual.observed,
                expected["observed"].as_f64().unwrap(),
                1e-12,
            );
            approx(
                actual.expected,
                expected["expected"].as_f64().unwrap(),
                1e-12,
            );
            approx(actual.weight, expected["weight"].as_f64().unwrap(), 1e-12);
            approx(
                actual.stratum_rate,
                expected["stratum_rate"].as_f64().unwrap(),
                1e-12,
            );
        }
    }

    #[test]
    fn attributable_risk_matches_r_fixture() {
        let fixture = load_fixture("tests/fixtures/r/attributable_risk.json");
        let (rows, headers) = rows_from_fixture(&fixture, &["exposure", "outcome", "person_time"]);

        let result = attributable_csv(
            &rows,
            &headers,
            "exposure",
            "outcome",
            Some("person_time"),
            None,
            0.05,
            NaStrategy::Drop,
        )
        .unwrap();

        approx(
            result.rate_exposed,
            expected_f64(&fixture, "rate_exposed"),
            1e-12,
        );
        approx(
            result.rate_unexposed,
            expected_f64(&fixture, "rate_unexposed"),
            1e-12,
        );
        approx(result.ar, expected_f64(&fixture, "ar"), 1e-12);
        approx(
            result.ar_ci_lower,
            expected_f64(&fixture, "ar_ci_lower"),
            1e-10,
        );
        approx(
            result.ar_ci_upper,
            expected_f64(&fixture, "ar_ci_upper"),
            1e-10,
        );
        approx(
            result.ar_percent,
            expected_f64(&fixture, "ar_percent"),
            1e-12,
        );
        approx(
            result.exposure_prevalence.unwrap(),
            expected_f64(&fixture, "default_exposure_prevalence"),
            1e-12,
        );
        approx(
            result.par.unwrap(),
            expected_f64(&fixture, "default_par"),
            1e-12,
        );
        approx(
            result.par_ci_lower.unwrap(),
            expected_f64(&fixture, "default_par_ci_lower"),
            1e-10,
        );
        approx(
            result.par_ci_upper.unwrap(),
            expected_f64(&fixture, "default_par_ci_upper"),
            1e-10,
        );
        approx(
            result.par_percent.unwrap(),
            expected_f64(&fixture, "default_par_percent"),
            1e-12,
        );
    }

    #[test]
    fn attributable_risk_exposure_prevalence_override_changes_par() {
        let fixture = load_fixture("tests/fixtures/r/attributable_risk.json");
        let (rows, headers) = rows_from_fixture(&fixture, &["exposure", "outcome", "person_time"]);
        let prevalence = expected_f64(&fixture, "override_exposure_prevalence");

        let result = attributable_csv(
            &rows,
            &headers,
            "exposure",
            "outcome",
            Some("person_time"),
            Some(prevalence),
            0.05,
            NaStrategy::Drop,
        )
        .unwrap();

        approx(result.exposure_prevalence.unwrap(), prevalence, 1e-12);
        approx(
            result.par.unwrap(),
            expected_f64(&fixture, "override_par"),
            1e-12,
        );
        approx(
            result.par_ci_lower.unwrap(),
            expected_f64(&fixture, "override_par_ci_lower"),
            1e-10,
        );
        approx(
            result.par_ci_upper.unwrap(),
            expected_f64(&fixture, "override_par_ci_upper"),
            1e-10,
        );
        approx(
            result.par_percent.unwrap(),
            expected_f64(&fixture, "override_par_percent"),
            1e-12,
        );
    }

    #[test]
    fn attributable_risk_rejects_invalid_exposure_prevalence() {
        let headers = csv::StringRecord::from(vec!["exposure", "outcome"]);
        let rows = vec![
            csv::StringRecord::from(vec!["1", "1"]),
            csv::StringRecord::from(vec!["0", "0"]),
        ];

        let err = attributable_csv(
            &rows,
            &headers,
            "exposure",
            "outcome",
            None,
            Some(1.5),
            0.05,
            NaStrategy::Drop,
        )
        .unwrap_err();

        assert!(err.contains("between 0 and 1"), "err={err}");
    }

    #[test]
    fn normality_matches_r_fixture() {
        let fixture = load_fixture("tests/fixtures/r/normality.json");
        let (rows, headers) = rows_from_fixture(&fixture, &["value"]);

        let result = normality_csv(&rows, &headers, "value", NaStrategy::Drop).unwrap();

        assert_eq!(result.n, expected_usize(&fixture, "n"));
        approx(result.skewness, expected_f64(&fixture, "skewness"), 1e-12);
        approx(result.kurtosis, expected_f64(&fixture, "kurtosis"), 1e-12);
        approx(
            result.shapiro_w.unwrap(),
            expected_f64(&fixture, "shapiro_w"),
            1e-12,
        );
        approx(
            result.shapiro_p.unwrap(),
            expected_f64(&fixture, "shapiro_p"),
            1e-10,
        );
        assert_eq!(
            result.shapiro_p_unreliable,
            fixture["expected"]["shapiro_p_unreliable"]
                .as_bool()
                .unwrap()
        );
        approx(result.ks_d, expected_f64(&fixture, "ks_d"), 1e-12);
        approx(result.ks_p, expected_f64(&fixture, "ks_p"), 1e-12);
        assert_eq!(
            result.lilliefors_used,
            fixture["expected"]["lilliefors_used"].as_bool().unwrap()
        );
    }

    #[test]
    fn variance_homogeneity_matches_r_fixture() {
        let fixture = load_fixture("tests/fixtures/r/variance_homogeneity.json");
        let (rows, headers) = rows_from_fixture(&fixture, &["group", "value"]);

        let result = variance_homogeneity_csv(
            &rows,
            &headers,
            "value",
            "group",
            "median",
            NaStrategy::Drop,
        )
        .unwrap();

        assert_eq!(result.n_used, expected_usize(&fixture, "n_used"));
        approx(
            result.levene_statistic,
            expected_f64(&fixture, "levene_statistic"),
            1e-12,
        );
        approx(result.levene_p, expected_f64(&fixture, "levene_p"), 1e-8);
        approx(
            result.bartlett_statistic,
            expected_f64(&fixture, "bartlett_statistic"),
            1e-12,
        );
        approx(
            result.bartlett_p,
            expected_f64(&fixture, "bartlett_p"),
            1e-8,
        );

        let expected_groups = fixture["expected"]["groups"].as_array().unwrap();
        assert_eq!(result.groups.len(), expected_groups.len());
        for expected in expected_groups {
            let label = expected["group"].as_str().unwrap();
            let actual = result
                .groups
                .iter()
                .find(|group| group.group == label)
                .unwrap_or_else(|| panic!("missing group {label}"));
            assert_eq!(actual.n, expected["n"].as_u64().unwrap() as usize);
            approx(
                actual.variance,
                expected["variance"].as_f64().unwrap(),
                1e-12,
            );
            approx(actual.sd, expected["sd"].as_f64().unwrap(), 1e-12);
        }
    }

    #[test]
    fn lifetable_grouped_matches_r_fixture() {
        let fixture = load_fixture("tests/fixtures/r/lifetable_grouped.json");
        let (rows, headers) =
            rows_from_fixture(&fixture, &["interval", "entering", "events", "withdrawals"]);

        let result = lifetable_csv(
            &rows,
            &headers,
            "interval",
            "entering",
            "events",
            "withdrawals",
            0.05,
            NaStrategy::Drop,
        )
        .unwrap();

        assert_eq!(result.n_total, expected_usize(&fixture, "n_total"));
        assert_eq!(result.n_used, expected_usize(&fixture, "n_used"));
        assert_eq!(result.time, "interval");

        let expected_intervals = fixture["expected"]["intervals"].as_array().unwrap();
        assert_eq!(result.intervals.len(), expected_intervals.len());
        for expected in expected_intervals {
            let idx = expected["interval_index"].as_u64().unwrap() as usize;
            let actual = &result.intervals[idx];
            assert_eq!(actual.interval_index, idx);
            approx(actual.start, expected["start"].as_f64().unwrap(), 1e-12);
            approx(actual.end, expected["end"].as_f64().unwrap(), 1e-12);
            assert_eq!(
                actual.entering,
                expected["entering"].as_u64().unwrap() as usize
            );
            assert_eq!(
                actual.withdrawals,
                expected["withdrawals"].as_u64().unwrap() as usize
            );
            assert_eq!(actual.events, expected["events"].as_u64().unwrap() as usize);
            approx(
                actual.effective_at_risk,
                expected["effective_at_risk"].as_f64().unwrap(),
                1e-12,
            );
            approx(
                actual.conditional_survival,
                expected["conditional_survival"].as_f64().unwrap(),
                1e-12,
            );
            approx(
                actual.cumulative_survival,
                expected["cumulative_survival"].as_f64().unwrap(),
                1e-12,
            );
            approx(
                actual.se_cumulative,
                expected["se_cumulative"].as_f64().unwrap(),
                1e-12,
            );
            approx(
                actual.ci_lower,
                expected["ci_lower"].as_f64().unwrap(),
                1e-10,
            );
            approx(
                actual.ci_upper,
                expected["ci_upper"].as_f64().unwrap(),
                1e-10,
            );
            approx(
                actual.hazard_rate,
                expected["hazard_rate"].as_f64().unwrap(),
                1e-12,
            );
            approx(
                actual.cumulative_hazard,
                expected["cumulative_hazard"].as_f64().unwrap(),
                1e-12,
            );
        }
    }

    #[test]
    fn lifetable_individual_rejects_negative_time() {
        let headers = csv::StringRecord::from(vec!["time", "status"]);
        let rows = vec![csv::StringRecord::from(vec!["-0.5", "1"])];

        let err = lifetable_individual_csv(
            &rows,
            &headers,
            "time",
            "status",
            "width=1",
            0.05,
            NaStrategy::Drop,
        )
        .unwrap_err();

        assert!(err.contains("non-negative"), "err={err}");
    }

    #[test]
    fn or_rr_crude_matches_scipy_fixture() {
        let fixture = load_fixture("tests/fixtures/python/or_rr_crude.json");
        let (rows, headers, strata) = or_rr_rows_from_fixture(&fixture);

        let result = or_rr_csv(
            &rows,
            &headers,
            "exposure",
            "outcome",
            &strata,
            None,
            None,
            0.05,
            NaStrategy::Drop,
        )
        .unwrap();

        assert_eq!(result.n_total, expected_usize(&fixture, "n_total"));
        assert_eq!(result.n_used, expected_usize(&fixture, "n_used"));
        assert_cells(&result.cells, &fixture["expected"]["cells"]);
        approx(
            result.odds_ratio,
            expected_f64(&fixture, "odds_ratio"),
            1e-12,
        );
        approx(
            result.or_ci_lower,
            expected_f64(&fixture, "or_ci_lower"),
            1e-10,
        );
        approx(
            result.or_ci_upper,
            expected_f64(&fixture, "or_ci_upper"),
            1e-10,
        );
        approx(
            result.relative_risk,
            expected_f64(&fixture, "relative_risk"),
            1e-12,
        );
        approx(
            result.rr_ci_lower,
            expected_f64(&fixture, "rr_ci_lower"),
            1e-10,
        );
        approx(
            result.rr_ci_upper,
            expected_f64(&fixture, "rr_ci_upper"),
            1e-10,
        );
        approx(
            result.chi_square,
            expected_f64(&fixture, "chi_square"),
            1e-12,
        );
        approx(
            result.chi_p_value,
            expected_f64(&fixture, "chi_p_value"),
            1e-12,
        );
        assert_eq!(
            result.continuity_correction,
            fixture["expected"]["continuity_correction"]
                .as_bool()
                .unwrap()
        );
        assert!(result.mh_or.is_none());
        assert!(result.homogeneity_p.is_none());
    }

    #[test]
    fn or_rr_stratified_matches_statsmodels_gold_reference() {
        let fixture = load_fixture("tests/fixtures/r/or_rr_stratified.json");
        let (rows, headers, strata) = or_rr_rows_from_fixture(&fixture);

        let result = or_rr_csv(
            &rows,
            &headers,
            "exposure",
            "outcome",
            &strata,
            None,
            None,
            0.05,
            NaStrategy::Drop,
        )
        .unwrap();

        assert_eq!(result.n_total, expected_usize(&fixture, "n_total"));
        assert_eq!(result.n_used, expected_usize(&fixture, "n_used"));
        assert_cells(&result.cells, &fixture["expected"]["cells"]);
        approx(
            result.odds_ratio,
            expected_f64(&fixture, "odds_ratio"),
            1e-12,
        );
        approx(
            result.relative_risk,
            expected_f64(&fixture, "relative_risk"),
            1e-12,
        );
        approx(
            result.chi_p_value,
            expected_f64(&fixture, "chi_p_value"),
            1e-12,
        );
        approx(
            result.mh_or.unwrap(),
            expected_f64(&fixture, "mh_or"),
            1e-12,
        );
        approx(
            result.mh_or_ci_lower.unwrap(),
            expected_f64(&fixture, "mh_or_ci_lower"),
            1e-8,
        );
        approx(
            result.mh_or_ci_upper.unwrap(),
            expected_f64(&fixture, "mh_or_ci_upper"),
            1e-8,
        );
        approx(
            result.mh_rr.unwrap(),
            expected_f64(&fixture, "mh_rr"),
            1e-12,
        );
        approx(
            result.mh_rr_ci_lower.unwrap(),
            expected_f64(&fixture, "mh_rr_ci_lower"),
            1e-8,
        );
        approx(
            result.mh_rr_ci_upper.unwrap(),
            expected_f64(&fixture, "mh_rr_ci_upper"),
            1e-8,
        );
        approx(
            result.homogeneity_chi_square.unwrap(),
            expected_f64(&fixture, "homogeneity_chi_square"),
            1e-12,
        );
        approx(
            result.homogeneity_p.unwrap(),
            expected_f64(&fixture, "homogeneity_p"),
            1e-12,
        );
        assert_eq!(
            result.continuity_correction,
            fixture["expected"]["continuity_correction"]
                .as_bool()
                .unwrap()
        );

        let expected_strata = fixture["expected"]["mh_strata"].as_array().unwrap();
        assert_eq!(result.mh_strata.len(), expected_strata.len());
        for expected in expected_strata {
            let label = expected["label"].as_str().unwrap();
            let actual = result
                .mh_strata
                .iter()
                .find(|stratum| stratum.label == label)
                .unwrap_or_else(|| panic!("missing MH stratum {label}"));
            assert_cells(&actual.cells, &expected["cells"]);
            approx(
                actual.or_stratum,
                expected["or_stratum"].as_f64().unwrap(),
                1e-12,
            );
            approx(
                actual.rr_stratum,
                expected["rr_stratum"].as_f64().unwrap(),
                1e-12,
            );
        }
    }

    #[test]
    fn or_rr_stratified_zero_cells_stay_finite() {
        let headers = csv::StringRecord::from(vec!["exposure", "outcome", "stratum"]);
        let mut rows = Vec::new();
        push_2x2_rows(&mut rows, "s1", "1", "1", 1);
        push_2x2_rows(&mut rows, "s2", "1", "0", 3);
        push_2x2_rows(&mut rows, "s2", "0", "1", 2);

        let result = or_rr_csv(
            &rows,
            &headers,
            "exposure",
            "outcome",
            &["stratum".to_string()],
            None,
            None,
            0.05,
            NaStrategy::Drop,
        )
        .unwrap();

        assert!(result.continuity_correction);
        assert!(result.mh_or.unwrap().is_finite());
        assert!(result.mh_rr.unwrap().is_finite());
        assert!(result.homogeneity_p.unwrap().is_finite());
        assert!(result
            .warnings
            .iter()
            .any(|warning| warning.contains("continuity correction")));
    }
}
