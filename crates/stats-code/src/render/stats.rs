use std::fmt::Write as _;

use serde::de::DeserializeOwned;

use crate::schema::{
    AttributableRiskResult, CochranArmitageResult, CorrelationResult, CoxResult,
    DiagnosticRocResult, LifeTableResult, LinearResult, LogisticResult, MannWhitneyResult,
    McNemarResult, NormalityResult, OneWayAnovaResult, OrRrResult, PlannedCommandResult,
    RbdAnovaResult, StandardizationResult, TtestOneSampleResult, TtestPairedResult,
    VarianceHomogeneityResult, WilcoxonSignedRankResult,
};

use super::writer::TextReportWriter;
use super::{format_optional_number, format_p_value};

pub fn render_stats_planned_text(result: &PlannedCommandResult) -> Option<String> {
    match result.command.as_str() {
        "stats.ttest.paired" => planned_result::<TtestPairedResult>(result)
            .map(|value| render_ttest_paired_text(&value)),
        "stats.ttest.one_sample" => planned_result::<TtestOneSampleResult>(result)
            .map(|value| render_ttest_one_sample_text(&value)),
        "stats.anova.oneway" => planned_result::<OneWayAnovaResult>(result)
            .map(|value| render_oneway_anova_text(&value))
            .or_else(|| {
                planned_result::<RbdAnovaResult>(result).map(|value| render_rbd_anova_text(&value))
            }),
        "stats.nonparam.cochran_armitage" => planned_result::<CochranArmitageResult>(result)
            .map(|value| render_cochran_armitage_text(&value)),
        "stats.nonparam.mcnemar" => {
            planned_result::<McNemarResult>(result).map(|value| render_mcnemar_text(&value))
        }
        "stats.nonparam.wilcoxon" => planned_result::<WilcoxonSignedRankResult>(result)
            .map(|value| render_wilcoxon_text(&value)),
        "stats.nonparam.mannwhitney" => planned_result::<MannWhitneyResult>(result)
            .map(|value| render_mann_whitney_text(&value)),
        "stats.correlation" => {
            planned_result::<CorrelationResult>(result).map(|value| render_correlation_text(&value))
        }
        "stats.epi.or_rr" => {
            planned_result::<OrRrResult>(result).map(|value| render_or_rr_text(&value))
        }
        "stats.epi.standardize" => planned_result::<StandardizationResult>(result)
            .map(|value| render_standardization_text(&value)),
        "stats.epi.attributable" => planned_result::<AttributableRiskResult>(result)
            .map(|value| render_attributable_risk_text(&value)),
        "stats.diagnostic.normality" => {
            planned_result::<NormalityResult>(result).map(|value| render_normality_text(&value))
        }
        "stats.diagnostic.variance" => planned_result::<VarianceHomogeneityResult>(result)
            .map(|value| render_variance_homogeneity_text(&value)),
        "stats.survival.lifetable" => {
            planned_result::<LifeTableResult>(result).map(|value| render_lifetable_text(&value))
        }
        _ => None,
    }
}

fn planned_result<T: DeserializeOwned>(result: &PlannedCommandResult) -> Option<T> {
    result
        .result
        .as_ref()
        .and_then(|value| serde_json::from_value::<T>(value.clone()).ok())
}

fn render_standard_fields(
    w: &mut TextReportWriter,
    status: &str,
    data_path: &str,
    analysis_path: Option<&str>,
    n_total: usize,
    n_used: usize,
    n_excluded_missing: usize,
) {
    w.field("Status", status);
    w.field("Data path", data_path);
    w.field_opt("Analysis", analysis_path);
    w.field(
        "Rows",
        format!("total={n_total} used={n_used} excluded_missing={n_excluded_missing}"),
    );
}

fn append_notes_and_warnings(out: &mut String, notes: &[String], warnings: &[String]) {
    if !warnings.is_empty() {
        let _ = writeln!(out, "  Warnings");
        for warning in warnings {
            let _ = writeln!(out, "  - {warning}");
        }
    }
    if !notes.is_empty() {
        let _ = writeln!(out, "  Notes");
        for note in notes {
            let _ = writeln!(out, "  - {note}");
        }
    }
}

pub fn render_ttest_paired_text(result: &TtestPairedResult) -> String {
    let mut w = TextReportWriter::new();
    w.title("Paired t-test");
    render_standard_fields(
        &mut w,
        &result.status,
        &result.data_path,
        result.analysis_path.as_deref(),
        result.n_total,
        result.n_used,
        result.n_excluded_missing,
    );
    w.field("Before", &result.before_variable);
    w.field("After", &result.after_variable);
    w.field("Pairs", result.n_pairs);
    w.field(
        "Mean diff",
        format!("{:.4} (sd={:.4})", result.mean_diff, result.sd_diff),
    );
    w.field(
        "t-test",
        format!(
            "t={:.4} df={:.1} p={} CI95=[{:.4}, {:.4}]",
            result.t_statistic,
            result.df,
            format_p_value(result.p_value),
            result.ci_lower,
            result.ci_upper
        ),
    );
    let mut out = w.finish();
    append_notes_and_warnings(&mut out, &result.notes, &result.warnings);
    out
}

pub fn render_ttest_one_sample_text(result: &TtestOneSampleResult) -> String {
    let mut w = TextReportWriter::new();
    w.title("One-sample t-test");
    render_standard_fields(
        &mut w,
        &result.status,
        &result.data_path,
        result.analysis_path.as_deref(),
        result.n_total,
        result.n_used,
        result.n_excluded_missing,
    );
    w.field("Variable", &result.variable);
    w.field("Hypothesis", format!("mu={:.4}", result.hypothesized_mean));
    w.field(
        "Sample",
        format!(
            "n={} mean={:.4} sd={:.4}",
            result.n, result.sample_mean, result.sample_sd
        ),
    );
    w.field(
        "t-test",
        format!(
            "t={:.4} df={:.1} p={} CI95=[{:.4}, {:.4}]",
            result.t_statistic,
            result.df,
            format_p_value(result.p_value),
            result.ci_lower,
            result.ci_upper
        ),
    );
    let mut out = w.finish();
    append_notes_and_warnings(&mut out, &result.notes, &result.warnings);
    out
}

pub fn render_oneway_anova_text(result: &OneWayAnovaResult) -> String {
    let mut w = TextReportWriter::new();
    w.title("One-way ANOVA");
    render_standard_fields(
        &mut w,
        &result.status,
        &result.data_path,
        result.analysis_path.as_deref(),
        result.n_total,
        result.n_used,
        result.n_excluded_missing,
    );
    w.field("Variable", &result.variable);
    w.field("Group", &result.group);
    w.field(
        "F-test",
        format!(
            "F({}, {})={:.4} p={}",
            result.df_between,
            result.df_within,
            result.f_statistic,
            format_p_value(result.p_value)
        ),
    );
    w.field(
        "SS",
        format!(
            "between={:.4} within={:.4} total={:.4}",
            result.ss_between, result.ss_within, result.ss_total
        ),
    );
    let mut out = w.finish();
    if !result.groups.is_empty() {
        let _ = writeln!(out, "  Groups");
        for group in &result.groups {
            let _ = writeln!(
                out,
                "  - {} n={} mean={:.4} sd={:.4}",
                group.group, group.n, group.mean, group.sd
            );
        }
    }
    append_notes_and_warnings(&mut out, &result.notes, &result.warnings);
    out
}

pub fn render_rbd_anova_text(result: &RbdAnovaResult) -> String {
    let mut w = TextReportWriter::new();
    w.title("Randomized-block ANOVA");
    render_standard_fields(
        &mut w,
        &result.status,
        &result.data_path,
        result.analysis_path.as_deref(),
        result.n_total,
        result.n_used,
        result.n_excluded_missing,
    );
    w.field("Variable", &result.variable);
    w.field("Group", &result.group);
    w.field("Block", &result.block);
    w.field(
        "Treatment",
        format!(
            "F({}, {})={:.4} p={}",
            result.treatment_df1,
            result.treatment_df2,
            result.treatment_f,
            format_p_value(result.treatment_p)
        ),
    );
    w.field(
        "Block F",
        format!(
            "F({}, {})={:.4} p={}",
            result.block_df1,
            result.block_df2,
            result.block_f,
            format_p_value(result.block_p)
        ),
    );
    let mut out = w.finish();
    append_notes_and_warnings(&mut out, &result.notes, &result.warnings);
    out
}

pub fn render_cochran_armitage_text(result: &CochranArmitageResult) -> String {
    let mut w = TextReportWriter::new();
    w.title("Cochran-Armitage Trend Test");
    render_standard_fields(
        &mut w,
        &result.status,
        &result.data_path,
        result.analysis_path.as_deref(),
        result.n_total,
        result.n_used,
        result.n_excluded_missing,
    );
    w.field("Exposure", &result.exposure);
    w.field("Outcome", &result.outcome);
    w.field(
        "Trend",
        format!(
            "z={:.4} p={}",
            result.trend_statistic,
            format_p_value(result.p_value)
        ),
    );
    let mut out = w.finish();
    for category in &result.categories {
        let _ = writeln!(
            out,
            "  - {} score={:.3} events={} n={} proportion={:.4}",
            category.category, category.score, category.events, category.n, category.proportion
        );
    }
    append_notes_and_warnings(&mut out, &result.notes, &result.warnings);
    out
}

pub fn render_mcnemar_text(result: &McNemarResult) -> String {
    let mut w = TextReportWriter::new();
    w.title("McNemar Test");
    render_standard_fields(
        &mut w,
        &result.status,
        &result.data_path,
        result.analysis_path.as_deref(),
        result.n_total,
        result.n_used,
        result.n_excluded_missing,
    );
    w.field("Variables", format!("{} vs {}", result.var1, result.var2));
    w.field("Discordant", format!("b={} c={}", result.b, result.c));
    w.field(
        "Chi-square",
        format!(
            "{:.4} p={} exact={}",
            result.chi_square,
            format_p_value(result.p_value),
            format_optional_number(result.exact_p_value)
        ),
    );
    let mut out = w.finish();
    append_notes_and_warnings(&mut out, &result.notes, &result.warnings);
    out
}

pub fn render_wilcoxon_text(result: &WilcoxonSignedRankResult) -> String {
    let mut w = TextReportWriter::new();
    w.title("Wilcoxon Signed-rank Test");
    render_standard_fields(
        &mut w,
        &result.status,
        &result.data_path,
        result.analysis_path.as_deref(),
        result.n_total,
        result.n_used,
        result.n_excluded_missing,
    );
    w.field("Variables", format!("{} vs {}", result.var1, result.var2));
    w.field(
        "Statistic",
        format!(
            "W+={:.4} z={:.4} p={}",
            result.w_plus,
            result.z_statistic,
            format_p_value(result.p_value)
        ),
    );
    w.field(
        "Corrections",
        format!(
            "zero_pairs_excluded={} ties_corrected={}",
            result.n_zero_pairs_excluded, result.n_ties_corrected
        ),
    );
    let mut out = w.finish();
    append_notes_and_warnings(&mut out, &result.notes, &result.warnings);
    out
}

pub fn render_mann_whitney_text(result: &MannWhitneyResult) -> String {
    let mut w = TextReportWriter::new();
    w.title("Mann-Whitney U Test");
    render_standard_fields(
        &mut w,
        &result.status,
        &result.data_path,
        result.analysis_path.as_deref(),
        result.n_total,
        result.n_used,
        result.n_excluded_missing,
    );
    w.field("Variable", &result.variable);
    w.field("Group", &result.group);
    w.field(
        "Groups",
        format!(
            "{} n={} median={:.4}; {} n={} median={:.4}",
            result.group_a_label,
            result.n_a,
            result.median_a,
            result.group_b_label,
            result.n_b,
            result.median_b
        ),
    );
    w.field(
        "Statistic",
        format!(
            "U={:.4} z={:.4} p={}",
            result.u_statistic,
            result.z_statistic,
            format_p_value(result.p_value)
        ),
    );
    let mut out = w.finish();
    append_notes_and_warnings(&mut out, &result.notes, &result.warnings);
    out
}

pub fn render_correlation_text(result: &CorrelationResult) -> String {
    let mut w = TextReportWriter::new();
    w.title("Correlation");
    render_standard_fields(
        &mut w,
        &result.status,
        &result.data_path,
        result.analysis_path.as_deref(),
        result.n_total,
        result.n_used,
        result.n_excluded_missing,
    );
    w.field("Method", &result.method);
    w.field(
        "Variables",
        format!("{} ~ {}", result.y_variable, result.x_variable),
    );
    w.field(
        "Pearson",
        format!(
            "r={:.4} R2={:.4} t={:.4} df={:.1} p={} CI95=[{:.4}, {:.4}]",
            result.r,
            result.r_squared,
            result.t_statistic,
            result.df,
            format_p_value(result.p_value),
            result.ci_lower,
            result.ci_upper
        ),
    );
    if let Some(rho) = result.spearman_rho {
        w.field(
            "Spearman",
            format!(
                "rho={:.4} p={}",
                rho,
                result
                    .spearman_p_value
                    .map(format_p_value)
                    .unwrap_or_else(|| "NA".to_string())
            ),
        );
    }
    let mut out = w.finish();
    append_notes_and_warnings(&mut out, &result.notes, &result.warnings);
    out
}

pub fn render_or_rr_text(result: &OrRrResult) -> String {
    let mut w = TextReportWriter::new();
    w.title("Odds Ratio / Relative Risk");
    w.field("Status", &result.status);
    w.field("Data path", &result.data_path);
    w.field_opt("Analysis", result.analysis_path.as_deref());
    w.field("Exposure", &result.exposure);
    w.field("Outcome", &result.outcome);
    w.field(
        "Rows",
        format!(
            "total={} used={} excluded_missing={}",
            result.n_total, result.n_used, result.n_excluded_missing
        ),
    );
    w.field(
        "2x2 cells",
        format!(
            "a={:.3} b={:.3} c={:.3} d={:.3}",
            result.cells.a, result.cells.b, result.cells.c, result.cells.d
        ),
    );
    w.field(
        "OR",
        format!(
            "{:.4} ({:.4}, {:.4})",
            result.odds_ratio, result.or_ci_lower, result.or_ci_upper
        ),
    );
    w.field(
        "RR",
        format!(
            "{:.4} ({:.4}, {:.4})",
            result.relative_risk, result.rr_ci_lower, result.rr_ci_upper
        ),
    );
    w.field(
        "Chi-square",
        format!(
            "{:.4}; p={}",
            result.chi_square,
            format_p_value(result.chi_p_value)
        ),
    );
    w.field("Continuity corr.", result.continuity_correction);
    if let Some(mh_or) = result.mh_or {
        w.field(
            "MH OR",
            format!(
                "{:.4} ({}, {})",
                mh_or,
                format_optional_number(result.mh_or_ci_lower),
                format_optional_number(result.mh_or_ci_upper)
            ),
        );
    }
    if let Some(mh_rr) = result.mh_rr {
        w.field(
            "MH RR",
            format!(
                "{:.4} ({}, {})",
                mh_rr,
                format_optional_number(result.mh_rr_ci_lower),
                format_optional_number(result.mh_rr_ci_upper)
            ),
        );
    }
    if let Some(statistic) = result.homogeneity_chi_square {
        w.field(
            "Breslow-Day",
            format!(
                "{:.4}; p={}",
                statistic,
                result
                    .homogeneity_p
                    .map(format_p_value)
                    .unwrap_or_else(|| "NA".to_string())
            ),
        );
    }

    let mut out = w.finish();
    if !result.mh_strata.is_empty() {
        let _ = writeln!(out, "  Strata");
        for stratum in &result.mh_strata {
            let _ = writeln!(
                out,
                "  - {}: OR={:.4} RR={:.4} cells a={:.3} b={:.3} c={:.3} d={:.3}",
                stratum.label,
                stratum.or_stratum,
                stratum.rr_stratum,
                stratum.cells.a,
                stratum.cells.b,
                stratum.cells.c,
                stratum.cells.d
            );
        }
    }
    if !result.warnings.is_empty() {
        let _ = writeln!(out, "  Warnings");
        for warning in &result.warnings {
            let _ = writeln!(out, "  - {warning}");
        }
    }
    if !result.notes.is_empty() {
        let _ = writeln!(out, "  Notes");
        for note in &result.notes {
            let _ = writeln!(out, "  - {note}");
        }
    }
    out
}

pub fn render_standardization_text(result: &StandardizationResult) -> String {
    let mut w = TextReportWriter::new();
    w.title("Rate Standardization");
    render_standard_fields(
        &mut w,
        &result.status,
        &result.data_path,
        result.analysis_path.as_deref(),
        result.n_total,
        result.n_used,
        result.n_excluded_missing,
    );
    w.field("Method", &result.method);
    w.field_opt(
        "Direct rate",
        result.standardized_rate.map(|value| {
            format!(
                "{value:.6} ({}, {})",
                format_optional_number(result.direct_ci_lower),
                format_optional_number(result.direct_ci_upper)
            )
        }),
    );
    w.field_opt(
        "SMR",
        result.smr.map(|value| {
            format!(
                "{value:.4} ({}, {})",
                format_optional_number(result.smr_ci_lower),
                format_optional_number(result.smr_ci_upper)
            )
        }),
    );
    let mut out = w.finish();
    if !result.strata.is_empty() {
        let _ = writeln!(out, "  Strata");
        for stratum in &result.strata {
            let _ = writeln!(
                out,
                "  - {} observed={:.3} expected={:.3} weight={:.4} rate={:.6}",
                stratum.age_group,
                stratum.observed,
                stratum.expected,
                stratum.weight,
                stratum.stratum_rate
            );
        }
    }
    append_notes_and_warnings(&mut out, &result.notes, &result.warnings);
    out
}

pub fn render_attributable_risk_text(result: &AttributableRiskResult) -> String {
    let mut w = TextReportWriter::new();
    w.title("Attributable Risk");
    render_standard_fields(
        &mut w,
        &result.status,
        &result.data_path,
        result.analysis_path.as_deref(),
        result.n_total,
        result.n_used,
        result.n_excluded_missing,
    );
    w.field("Exposure", &result.exposure);
    w.field("Outcome", &result.outcome);
    w.field(
        "Rates",
        format!(
            "exposed={:.6} unexposed={:.6}",
            result.rate_exposed, result.rate_unexposed
        ),
    );
    w.field(
        "AR",
        format!(
            "{:.6} ({:.6}, {:.6}); AR%={:.2}",
            result.ar, result.ar_ci_lower, result.ar_ci_upper, result.ar_percent
        ),
    );
    w.field_opt("PAR", result.par.map(|value| format!("{value:.6}")));
    w.field_opt(
        "PAR%",
        result.par_percent.map(|value| format!("{value:.2}")),
    );
    let mut out = w.finish();
    append_notes_and_warnings(&mut out, &result.notes, &result.warnings);
    out
}

pub fn render_normality_text(result: &NormalityResult) -> String {
    let mut w = TextReportWriter::new();
    w.title("Normality Diagnostics");
    render_standard_fields(
        &mut w,
        &result.status,
        &result.data_path,
        result.analysis_path.as_deref(),
        result.n_total,
        result.n_used,
        result.n_excluded_missing,
    );
    w.field("Variable", &result.variable);
    w.field(
        "Moments",
        format!(
            "skew={:.4} kurtosis={:.4}",
            result.skewness, result.kurtosis
        ),
    );
    w.field(
        "Shapiro-Wilk",
        format!(
            "W={} p={} unreliable={}",
            format_optional_number(result.shapiro_w),
            result
                .shapiro_p
                .map(format_p_value)
                .unwrap_or_else(|| "NA".to_string()),
            result.shapiro_p_unreliable
        ),
    );
    w.field(
        "K-S",
        format!(
            "D={:.4} p={} lilliefors={}",
            result.ks_d,
            format_p_value(result.ks_p),
            result.lilliefors_used
        ),
    );
    let mut out = w.finish();
    append_notes_and_warnings(&mut out, &result.notes, &result.warnings);
    out
}

pub fn render_variance_homogeneity_text(result: &VarianceHomogeneityResult) -> String {
    let mut w = TextReportWriter::new();
    w.title("Variance Homogeneity");
    render_standard_fields(
        &mut w,
        &result.status,
        &result.data_path,
        result.analysis_path.as_deref(),
        result.n_total,
        result.n_used,
        result.n_excluded_missing,
    );
    w.field("Variable", &result.variable);
    w.field("Group", &result.group);
    w.field(
        "Levene",
        format!(
            "{:.4} p={}",
            result.levene_statistic,
            format_p_value(result.levene_p)
        ),
    );
    w.field(
        "Bartlett",
        format!(
            "{:.4} p={}",
            result.bartlett_statistic,
            format_p_value(result.bartlett_p)
        ),
    );
    let mut out = w.finish();
    if !result.groups.is_empty() {
        let _ = writeln!(out, "  Groups");
        for group in &result.groups {
            let _ = writeln!(
                out,
                "  - {} n={} variance={:.4} sd={:.4}",
                group.group, group.n, group.variance, group.sd
            );
        }
    }
    append_notes_and_warnings(&mut out, &result.notes, &result.warnings);
    out
}

pub fn render_lifetable_text(result: &LifeTableResult) -> String {
    let mut w = TextReportWriter::new();
    w.title("Actuarial Life Table");
    render_standard_fields(
        &mut w,
        &result.status,
        &result.data_path,
        result.analysis_path.as_deref(),
        result.n_total,
        result.n_used,
        result.n_excluded_missing,
    );
    w.field("Time", &result.time);
    let mut out = w.finish();
    if !result.intervals.is_empty() {
        let _ = writeln!(out, "  Intervals");
        for row in &result.intervals {
            let _ = writeln!(
                out,
                "  - [{:.3}, {:.3}] entering={} events={} withdrawals={} survival={:.4} CI95=[{:.4}, {:.4}] hazard={:.4}",
                row.start,
                row.end,
                row.entering,
                row.events,
                row.withdrawals,
                row.cumulative_survival,
                row.ci_lower,
                row.ci_upper,
                row.hazard_rate
            );
        }
    }
    append_notes_and_warnings(&mut out, &result.notes, &result.warnings);
    out
}

pub fn render_logistic_text(result: &LogisticResult) -> String {
    let mut w = TextReportWriter::new();
    w.title("Logistic Model");
    w.field("Status", &result.status);
    w.field("Data path", &result.data_path);
    w.field_opt("Analysis", result.analysis_path.as_deref());
    w.field("Formula", &result.formula);
    w.field("Outcome", &result.outcome);
    w.field(
        "Predictors",
        if result.predictors.is_empty() {
            "<none>".to_string()
        } else {
            result.predictors.join(", ")
        },
    );
    w.field(
        "Rows",
        format!(
            "total={} used={} excluded_missing={} excluded_invalid={}",
            result.n_total, result.n_used, result.n_excluded_missing, result.n_excluded_invalid
        ),
    );
    w.field(
        "Outcome counts",
        format!(
            "events={} nonevents={}",
            result.n_events, result.n_nonevents
        ),
    );
    w.field(
        "Fit",
        format!(
            "converged={} iterations={} logLik={:.4}",
            result.converged, result.iterations, result.log_likelihood
        ),
    );
    w.field_opt(
        "Null logLik",
        result.null_log_likelihood.map(|v| format!("{v:.4}")),
    );

    // Diagnostics section — use raw buffer for sub-items
    let buf = &mut w;
    buf.field("Diagnostics", "");

    // We need direct access to the buffer for list items
    let mut out = w.finish();
    if let Some(r2) = result.pseudo_r2_nagelkerke {
        let _ = writeln!(out, "  - Nagelkerke R²  {r2:.4}");
    }
    if let Some(aic) = result.aic {
        let _ = writeln!(out, "  - AIC            {aic:.2}");
    }
    if let Some(bic) = result.bic {
        let _ = writeln!(out, "  - BIC            {bic:.2}");
    }
    if let Some(c) = result.c_statistic {
        let _ = writeln!(out, "  - C-statistic    {c:.4}");
    }
    let _ = writeln!(out, "  Coefficients");
    for coefficient in &result.coefficients {
        let p_value = format_p_value(coefficient.p_value);
        let level = coefficient
            .level
            .as_ref()
            .map(|level| format!(" level={level}"))
            .unwrap_or_default();
        let reference = coefficient
            .reference
            .as_ref()
            .map(|reference| format!(" ref={reference}"))
            .unwrap_or_default();
        let _ = writeln!(
            out,
            "  - {} OR={:.4} CI95=[{:.4}, {:.4}] p={} beta={:.4} se={:.4}{}{}",
            coefficient.term,
            coefficient.odds_ratio,
            coefficient.ci_lower,
            coefficient.ci_upper,
            p_value,
            coefficient.beta,
            coefficient.standard_error,
            level,
            reference
        );
    }
    if !result.warnings.is_empty() {
        let _ = writeln!(out, "  Warnings");
        for warning in &result.warnings {
            let _ = writeln!(out, "  - {warning}");
        }
    }
    if !result.notes.is_empty() {
        let _ = writeln!(out, "  Notes");
        for note in &result.notes {
            let _ = writeln!(out, "  - {note}");
        }
    }
    out
}

pub fn render_cox_text(result: &CoxResult) -> String {
    let mut w = TextReportWriter::new();
    w.title("Cox Model");
    w.field("Status", &result.status);
    w.field("Data path", &result.data_path);
    w.field_opt("Analysis", result.analysis_path.as_deref());
    w.field("Formula", &result.formula);
    w.field("Time", &result.time);
    w.field("Event", &result.event);
    w.field(
        "Predictors",
        if result.predictors.is_empty() {
            "<none>".to_string()
        } else {
            result.predictors.join(", ")
        },
    );
    w.field(
        "Rows",
        format!(
            "total={} used={} excluded_missing={} excluded_invalid={}",
            result.n_total, result.n_used, result.n_excluded_missing, result.n_excluded_invalid
        ),
    );
    w.field(
        "Event counts",
        format!(
            "events={} censored={} tied_event_times={}",
            result.n_events, result.n_censored, result.tied_event_times
        ),
    );
    w.field(
        "Fit",
        format!(
            "converged={} iterations={} logPartialLik={:.4}",
            result.converged, result.iterations, result.log_partial_likelihood
        ),
    );
    w.field_opt("Concordance", result.concordance.map(|c| format!("{c:.4}")));

    let mut out = w.finish();
    let _ = writeln!(out, "  Coefficients");
    for coefficient in &result.coefficients {
        let p_value = format_p_value(coefficient.p_value);
        let level = coefficient
            .level
            .as_ref()
            .map(|level| format!(" level={level}"))
            .unwrap_or_default();
        let reference = coefficient
            .reference
            .as_ref()
            .map(|reference| format!(" ref={reference}"))
            .unwrap_or_default();
        let _ = writeln!(
            out,
            "  - {} HR={:.4} CI95=[{:.4}, {:.4}] p={} beta={:.4} se={:.4}{}{}",
            coefficient.term,
            coefficient.hazard_ratio,
            coefficient.ci_lower,
            coefficient.ci_upper,
            p_value,
            coefficient.beta,
            coefficient.standard_error,
            level,
            reference
        );
    }
    if !result.ph_diagnostics.is_empty() {
        let _ = writeln!(
            out,
            "  PH diagnostics   Schoenfeld-style residual correlation with log(time)"
        );
        for diagnostic in &result.ph_diagnostics {
            let _ = writeln!(
                out,
                "  - {} corr={:.4} chi_square={:.4} p={} events={}",
                diagnostic.term,
                diagnostic.correlation,
                diagnostic.chi_square,
                format_p_value(diagnostic.p_value),
                diagnostic.event_count
            );
        }
    }
    if !result.warnings.is_empty() {
        let _ = writeln!(out, "  Warnings");
        for warning in &result.warnings {
            let _ = writeln!(out, "  - {warning}");
        }
    }
    if !result.notes.is_empty() {
        let _ = writeln!(out, "  Notes");
        for note in &result.notes {
            let _ = writeln!(out, "  - {note}");
        }
    }
    out
}

pub fn render_linear_text(result: &LinearResult) -> String {
    let mut w = TextReportWriter::new();
    w.title("Linear Model");
    w.field("Status", &result.status);
    w.field("Data path", &result.data_path);
    w.field_opt("Analysis", result.analysis_path.as_deref());
    w.field("Formula", &result.formula);
    w.field("Outcome", &result.outcome);
    w.field(
        "Predictors",
        if result.predictors.is_empty() {
            "<none>".to_string()
        } else {
            result.predictors.join(", ")
        },
    );
    w.field(
        "Rows",
        format!(
            "total={} used={} excluded_missing={} excluded_invalid={}",
            result.n_total, result.n_used, result.n_excluded_missing, result.n_excluded_invalid
        ),
    );
    w.field(
        "Fit",
        format!(
            "converged={} R²={:.4} adj_R²={:.4} RSE={:.4}",
            result.converged,
            result.r_squared,
            result.adjusted_r_squared,
            result.residual_std_error
        ),
    );
    if let Some(f) = result.f_statistic {
        let p_text = result
            .f_p_value
            .map(|p| format!(" p={}", format_p_value(p)))
            .unwrap_or_default();
        w.field("F-statistic", format!("{f:.4}{p_text}"));
    }

    let mut out = w.finish();
    let _ = writeln!(out, "  Diagnostics");
    if let Some(aic) = result.aic {
        let _ = writeln!(out, "  - AIC            {aic:.2}");
    }
    if let Some(bic) = result.bic {
        let _ = writeln!(out, "  - BIC            {bic:.2}");
    }
    let _ = writeln!(out, "  Coefficients");
    for coefficient in &result.coefficients {
        let p_value = format_p_value(coefficient.p_value);
        let level = coefficient
            .level
            .as_ref()
            .map(|level| format!(" level={level}"))
            .unwrap_or_default();
        let reference = coefficient
            .reference
            .as_ref()
            .map(|reference| format!(" ref={reference}"))
            .unwrap_or_default();
        let _ = writeln!(
            out,
            "  - {} beta={:.4} se={:.4} t={:.4} p={} CI95=[{:.4}, {:.4}]{}{}",
            coefficient.term,
            coefficient.beta,
            coefficient.standard_error,
            coefficient.t_statistic,
            p_value,
            coefficient.ci_lower,
            coefficient.ci_upper,
            level,
            reference
        );
    }
    if !result.warnings.is_empty() {
        let _ = writeln!(out, "  Warnings");
        for warning in &result.warnings {
            let _ = writeln!(out, "  - {warning}");
        }
    }
    if !result.notes.is_empty() {
        let _ = writeln!(out, "  Notes");
        for note in &result.notes {
            let _ = writeln!(out, "  - {note}");
        }
    }
    out
}

pub fn render_diagnostic_roc_text(result: &DiagnosticRocResult) -> String {
    let mut w = TextReportWriter::new();
    w.title("Diagnostic ROC");
    w.field("Status", &result.status);
    w.field("Data path", &result.data_path);
    w.field_opt("Analysis", result.analysis_path.as_deref());
    w.field("Truth", &result.truth);
    w.field("Score", &result.score);
    w.field(
        "Rows",
        format!(
            "total={} used={} excluded_missing={} excluded_invalid={}",
            result.n_total, result.n_used, result.n_excluded_missing, result.n_excluded_invalid
        ),
    );
    w.field(
        "Classes",
        format!("cases={} controls={}", result.n_cases, result.n_controls),
    );
    w.field("AUC", format!("{:.4}", result.auc));
    w.field(
        "Youden threshold",
        format!(
            "{:.4} J={:.4} sensitivity={:.4} specificity={:.4}",
            result.youden.threshold,
            result.youden.youden_j,
            result.youden.sensitivity,
            result.youden.specificity
        ),
    );

    let mut out = w.finish();
    if let Some(metrics) = &result.threshold_metrics {
        let _ = writeln!(out, "  Threshold metrics");
        let _ = writeln!(
            out,
            "  - threshold={:.4} TP={} FP={} TN={} FN={} sensitivity={:.4} specificity={:.4} PPV={:.4} NPV={:.4} accuracy={:.4} balanced_accuracy={:.4} F1={:.4}",
            metrics.threshold,
            metrics.tp,
            metrics.fp,
            metrics.tn,
            metrics.fn_count,
            metrics.sensitivity,
            metrics.specificity,
            metrics.ppv,
            metrics.npv,
            metrics.accuracy,
            metrics.balanced_accuracy,
            metrics.f1_score
        );
        let _ = writeln!(
            out,
            "  - LR+={} LR-={} DOR={}",
            format_optional_number(metrics.positive_likelihood_ratio),
            format_optional_number(metrics.negative_likelihood_ratio),
            format_optional_number(metrics.diagnostic_odds_ratio)
        );
    }
    let _ = writeln!(out, "  ROC points");
    for point in &result.roc_points {
        let _ = writeln!(
            out,
            "  - threshold={:.4} sensitivity={:.4} specificity={:.4} FPR={:.4} TPR={:.4}",
            point.threshold,
            point.sensitivity,
            point.specificity,
            point.false_positive_rate,
            point.true_positive_rate
        );
    }
    if !result.warnings.is_empty() {
        let _ = writeln!(out, "  Warnings");
        for warning in &result.warnings {
            let _ = writeln!(out, "  - {warning}");
        }
    }
    if !result.notes.is_empty() {
        let _ = writeln!(out, "  Notes");
        for note in &result.notes {
            let _ = writeln!(out, "  - {note}");
        }
    }
    out
}
