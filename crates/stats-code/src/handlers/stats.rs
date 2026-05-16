use std::path::PathBuf;

use serde::Serialize;
use serde_json::Value;

use crate::bridge::{
    bridge_to_competing_risks, bridge_to_lda, bridge_to_mixed_lmm, bridge_to_multinomial_logit,
    bridge_to_ordinal_logit, execute_bridge, BridgeConfig, BridgeRequest, Engine,
};
use crate::cli::{
    AgreementCommand, AnovaCommand, Cli, DiagnosticStatsCommand, EpiStatsCommand,
    MultivariateCommand, NonparamCommand, SampleSizeCommand, StatsCommand, StatsModelCommand,
    StatsSurvivalCommand, TtestCommand,
};
use crate::helpers::stringify_error;
use crate::report::resolve_data_path;
use crate::schema::PlannedCommandResult;
use crate::stats::basic::{
    attributable_csv, bland_altman_csv, cluster_csv, cochran_armitage_csv, dose_response_csv,
    kappa_csv, lifetable_csv, lifetable_individual_csv, logrank_sample_size, mann_whitney_csv,
    mcnemar_csv, meta_analysis_csv, normality_csv, oneway_anova_csv, or_rr_csv, pca_csv,
    poisson_glm_csv, posthoc_csv, psm_csv, rbd_anova_csv, repeated_anova_csv, standardize_csv,
    variance_homogeneity_csv, wilcoxon_csv,
};
use crate::stats::correlation::correlation_csv;
use crate::stats::ttest::{one_sample_ttest_csv, paired_ttest_csv};

pub(crate) fn handle_stats(
    cli: &Cli,
    command: &StatsCommand,
) -> Result<PlannedCommandResult, String> {
    match command {
        StatsCommand::Ttest { command } => match command {
            TtestCommand::Paired(args) => {
                let input = load_rows(args.data.as_ref(), args.analysis.as_ref())?;
                let result = paired_ttest_csv(
                    &input.rows,
                    &input.headers,
                    &args.before,
                    &args.after,
                    cli.alpha,
                )?;
                planned(
                    "stats.ttest.paired",
                    &input,
                    Some(format!("paired t-test: {} vs {}", args.before, args.after)),
                    &result,
                )
            }
            TtestCommand::OneSample(args) => {
                let input = load_rows(args.data.as_ref(), args.analysis.as_ref())?;
                let result = one_sample_ttest_csv(
                    &input.rows,
                    &input.headers,
                    &args.var,
                    args.mu,
                    cli.alpha,
                )?;
                planned(
                    "stats.ttest.one_sample",
                    &input,
                    Some(format!("one-sample t-test: {} mu={}", args.var, args.mu)),
                    &result,
                )
            }
        },
        StatsCommand::Anova { command } => match command {
            AnovaCommand::Oneway(args) => {
                let input = load_rows(args.data.as_ref(), args.analysis.as_ref())?;
                if let Some(block) = &args.block {
                    let result = rbd_anova_csv(
                        &input.rows,
                        &input.headers,
                        &args.var,
                        &args.group,
                        block,
                        cli.na_strategy,
                    )?;
                    planned(
                        "stats.anova.oneway",
                        &input,
                        Some("randomized-block ANOVA".into()),
                        &result,
                    )
                } else {
                    let result = oneway_anova_csv(
                        &input.rows,
                        &input.headers,
                        &args.var,
                        &args.group,
                        cli.na_strategy,
                    )?;
                    planned(
                        "stats.anova.oneway",
                        &input,
                        Some("one-way ANOVA".into()),
                        &result,
                    )
                }
            }
            AnovaCommand::Repeated(args) => {
                let input = load_rows(args.data.as_ref(), args.analysis.as_ref())?;
                let result = repeated_anova_csv(
                    &input.rows,
                    &input.headers,
                    &args.var,
                    &args.subject,
                    &args.time,
                    cli.na_strategy,
                )?;
                planned(
                    "stats.anova.repeated",
                    &input,
                    Some("repeated-measures ANOVA".into()),
                    &result,
                )
            }
            AnovaCommand::Posthoc(args) => {
                let input = load_rows(args.data.as_ref(), args.analysis.as_ref())?;
                let result = posthoc_csv(
                    &input.rows,
                    &input.headers,
                    &args.var,
                    &args.group,
                    &args.method,
                    cli.alpha,
                    cli.na_strategy,
                )?;
                planned(
                    "stats.anova.posthoc",
                    &input,
                    Some("post-hoc pairwise comparisons".into()),
                    &result,
                )
            }
        },
        StatsCommand::Nonparam { command } => match command {
            NonparamCommand::Mcnemar(args) => {
                let input = load_rows(args.data.as_ref(), args.analysis.as_ref())?;
                let result = mcnemar_csv(
                    &input.rows,
                    &input.headers,
                    &args.var1,
                    &args.var2,
                    args.exact_threshold,
                    cli.na_strategy,
                )?;
                planned(
                    "stats.nonparam.mcnemar",
                    &input,
                    Some("McNemar test".into()),
                    &result,
                )
            }
            NonparamCommand::Wilcoxon(args) => {
                let input = load_rows(args.data.as_ref(), args.analysis.as_ref())?;
                let result = wilcoxon_csv(
                    &input.rows,
                    &input.headers,
                    &args.var1,
                    &args.var2,
                    cli.na_strategy,
                )?;
                planned(
                    "stats.nonparam.wilcoxon",
                    &input,
                    Some("Wilcoxon signed-rank test".into()),
                    &result,
                )
            }
            NonparamCommand::Mannwhitney(args) => {
                let input = load_rows(args.data.as_ref(), args.analysis.as_ref())?;
                let result = mann_whitney_csv(
                    &input.rows,
                    &input.headers,
                    &args.var,
                    &args.group,
                    cli.na_strategy,
                )?;
                planned(
                    "stats.nonparam.mannwhitney",
                    &input,
                    Some("Mann-Whitney U test".into()),
                    &result,
                )
            }
            NonparamCommand::CochranArmitage(args) => {
                let input = load_rows(args.data.as_ref(), args.analysis.as_ref())?;
                let result = cochran_armitage_csv(
                    &input.rows,
                    &input.headers,
                    &args.exposure,
                    &args.outcome,
                    &args.scores,
                    cli.na_strategy,
                )?;
                planned(
                    "stats.nonparam.cochran_armitage",
                    &input,
                    Some("Cochran-Armitage trend test".into()),
                    &result,
                )
            }
        },
        StatsCommand::Correlation(args) => {
            let input = load_rows(args.data.as_ref(), args.analysis.as_ref())?;
            let method = if args.method.eq_ignore_ascii_case("both") {
                "spearman"
            } else {
                args.method.as_str()
            };
            let mut result = correlation_csv(
                &input.rows,
                &input.headers,
                &args.x,
                &args.y,
                cli.alpha,
                method,
            )?;
            if args.method.eq_ignore_ascii_case("both") {
                result.method = "both".to_string();
            }
            planned(
                "stats.correlation",
                &input,
                Some(format!("{} ~ {}", args.y, args.x)),
                &result,
            )
        }
        StatsCommand::Diagnostic { command } => match command {
            DiagnosticStatsCommand::Normality(args) => {
                let input = load_rows(args.data.as_ref(), args.analysis.as_ref())?;
                let result =
                    normality_csv(&input.rows, &input.headers, &args.var, cli.na_strategy)?;
                planned(
                    "stats.diagnostic.normality",
                    &input,
                    Some("normality diagnostics".into()),
                    &result,
                )
            }
            DiagnosticStatsCommand::Variance(args) => {
                let input = load_rows(args.data.as_ref(), args.analysis.as_ref())?;
                let result = variance_homogeneity_csv(
                    &input.rows,
                    &input.headers,
                    &args.var,
                    &args.group,
                    &args.center,
                    cli.na_strategy,
                )?;
                planned(
                    "stats.diagnostic.variance",
                    &input,
                    Some("variance homogeneity".into()),
                    &result,
                )
            }
        },
        StatsCommand::Epi { command } => match command {
            EpiStatsCommand::OrRr(args) => {
                let input = load_rows(args.data.as_ref(), args.analysis.as_ref())?;
                let result = or_rr_csv(
                    &input.rows,
                    &input.headers,
                    &args.exposure,
                    &args.outcome,
                    &args.strata,
                    args.exposure_event.as_deref(),
                    args.outcome_event.as_deref(),
                    cli.alpha,
                    cli.na_strategy,
                )?;
                planned("stats.epi.or_rr", &input, Some("OR/RR".into()), &result)
            }
            EpiStatsCommand::Standardize(args) => {
                let input = load_rows(args.data.as_ref(), args.analysis.as_ref())?;
                let result = standardize_csv(
                    &input.rows,
                    &input.headers,
                    &args.method,
                    &args.event,
                    &args.person_time,
                    &args.age_group,
                    &args.standard_pop,
                    cli.alpha,
                    cli.na_strategy,
                )?;
                planned(
                    "stats.epi.standardize",
                    &input,
                    Some("rate standardization".into()),
                    &result,
                )
            }
            EpiStatsCommand::Attributable(args) => {
                let input = load_rows(args.data.as_ref(), args.analysis.as_ref())?;
                let result = attributable_csv(
                    &input.rows,
                    &input.headers,
                    &args.exposure,
                    &args.outcome,
                    args.person_time.as_deref(),
                    args.exposure_prevalence,
                    cli.alpha,
                    cli.na_strategy,
                )?;
                planned(
                    "stats.epi.attributable",
                    &input,
                    Some("attributable risk".into()),
                    &result,
                )
            }
            EpiStatsCommand::DoseResponse(args) => {
                let input = load_rows(args.data.as_ref(), args.analysis.as_ref())?;
                let result = dose_response_csv(
                    &input.rows,
                    &input.headers,
                    &args.exposure,
                    &args.outcome,
                    &args.person_time,
                    &args.scores,
                    cli.alpha,
                    cli.na_strategy,
                )?;
                planned(
                    "stats.epi.dose_response",
                    &input,
                    Some("dose-response".into()),
                    &result,
                )
            }
        },
        StatsCommand::Agreement { command } => match command {
            AgreementCommand::Kappa(args) => {
                let input = load_rows(args.data.as_ref(), args.analysis.as_ref())?;
                let result = kappa_csv(
                    &input.rows,
                    &input.headers,
                    args,
                    cli.alpha,
                    cli.na_strategy,
                )?;
                planned(
                    "stats.agreement.kappa",
                    &input,
                    Some("Cohen kappa".into()),
                    &result,
                )
            }
            AgreementCommand::BlandAltman(args) => {
                let input = load_rows(args.data.as_ref(), args.analysis.as_ref())?;
                let result = bland_altman_csv(
                    &input.rows,
                    &input.headers,
                    &args.method1,
                    &args.method2,
                    cli.alpha,
                    cli.na_strategy,
                )?;
                planned(
                    "stats.agreement.bland_altman",
                    &input,
                    Some("Bland-Altman".into()),
                    &result,
                )
            }
        },
        StatsCommand::Multivariate { command } => match command {
            MultivariateCommand::Pca(args) => {
                let input = load_rows(args.data.as_ref(), args.analysis.as_ref())?;
                let result = pca_csv(
                    &input.rows,
                    &input.headers,
                    &args.vars,
                    args.n_components,
                    &args.matrix,
                    cli.na_strategy,
                )?;
                planned(
                    "stats.multivariate.pca",
                    &input,
                    Some("PCA".into()),
                    &result,
                )
            }
            MultivariateCommand::Cluster(args) => {
                let input = load_rows(args.data.as_ref(), args.analysis.as_ref())?;
                let result = cluster_csv(
                    &input.rows,
                    &input.headers,
                    &args.vars,
                    args.k,
                    &args.method,
                    args.seed,
                    cli.na_strategy,
                )?;
                planned(
                    "stats.multivariate.cluster",
                    &input,
                    Some("cluster analysis".into()),
                    &result,
                )
            }
            MultivariateCommand::Lda(args) => {
                let input = load_rows(args.data.as_ref(), args.analysis.as_ref())?;
                require_python_engine("multivariate.lda", cli.engine)?;
                let response = run_python_bridge(
                    "lda",
                    &input,
                    serde_json::json!({
                        "group": &args.group,
                        "vars": &args.vars,
                        "ci_level": 1.0 - cli.alpha,
                    }),
                )?;
                let result = bridge_to_lda(&response)?;
                planned(
                    "stats.multivariate.lda",
                    &input,
                    Some("linear discriminant analysis".into()),
                    &result,
                )
            }
        },
        StatsCommand::Meta(args) => {
            let input = load_rows(args.data.as_ref(), args.analysis.as_ref())?;
            let result = meta_analysis_csv(
                &input.rows,
                &input.headers,
                &args.effect,
                &args.se,
                args.study_label.as_deref(),
                cli.alpha,
                cli.na_strategy,
            )?;
            planned("stats.meta", &input, Some("meta-analysis".into()), &result)
        }
        StatsCommand::Psm(args) => {
            let input = load_rows(args.data.as_ref(), args.analysis.as_ref())?;
            let output_path = args.output.clone().unwrap_or_else(|| {
                cli.artifacts_dir
                    .clone()
                    .unwrap_or_else(|| PathBuf::from("stats-code-artifacts"))
                    .join("psm_matched.csv")
            });
            let result = psm_csv(
                &input.rows,
                &input.headers,
                &args.treatment,
                &args.covariates,
                args.caliper,
                args.ratio,
                args.seed,
                cli.na_strategy,
                Some(&output_path),
            )?;
            planned(
                "stats.psm",
                &input,
                Some("propensity score matching".into()),
                &result,
            )
        }
        StatsCommand::SampleSize { command } => match command {
            SampleSizeCommand::LogRank(args) => {
                let result = logrank_sample_size(
                    args.median1,
                    args.median2,
                    args.accrual,
                    args.followup,
                    args.power,
                    cli.alpha,
                    args.allocation_ratio,
                    args.dropout_rate,
                )?;
                planned_no_data(
                    "stats.sample_size.log_rank",
                    Some("log-rank sample size".into()),
                    &result,
                )
            }
        },
        StatsCommand::Survival { command } => match command {
            StatsSurvivalCommand::Lifetable(args) => {
                let input = load_rows(args.data.as_ref(), args.analysis.as_ref())?;
                let result = if args.input_format.eq_ignore_ascii_case("individual") {
                    let time = args.time.as_deref().ok_or_else(|| {
                        "life table --input-format individual requires --time.".to_string()
                    })?;
                    let status = args.status.as_deref().ok_or_else(|| {
                        "life table --input-format individual requires --status.".to_string()
                    })?;
                    lifetable_individual_csv(
                        &input.rows,
                        &input.headers,
                        time,
                        status,
                        &args.intervals,
                        cli.alpha,
                        cli.na_strategy,
                    )?
                } else {
                    let entering = args.entering.as_deref().ok_or_else(|| {
                        "life table grouped input requires --entering.".to_string()
                    })?;
                    let events = args
                        .events
                        .as_deref()
                        .ok_or_else(|| "life table grouped input requires --events.".to_string())?;
                    let withdrawals = args.withdrawals.as_deref().ok_or_else(|| {
                        "life table grouped input requires --withdrawals.".to_string()
                    })?;
                    lifetable_csv(
                        &input.rows,
                        &input.headers,
                        &args.intervals,
                        entering,
                        events,
                        withdrawals,
                        cli.alpha,
                        cli.na_strategy,
                    )?
                };
                planned(
                    "stats.survival.lifetable",
                    &input,
                    Some("actuarial life table".into()),
                    &result,
                )
            }
            StatsSurvivalCommand::Competing(args) => {
                let input = load_rows(args.data.as_ref(), args.analysis.as_ref())?;
                require_python_engine("survival.competing", cli.engine)?;
                let response = run_python_bridge(
                    "competing_risks",
                    &input,
                    serde_json::json!({
                        "time": &args.time,
                        "event_type": &args.event_type,
                        "cause": &args.cause,
                        "x": &args.x,
                        "point_estimate_only": args.point_estimate_only,
                        "ci_level": 1.0 - cli.alpha,
                    }),
                )?;
                let result = bridge_to_competing_risks(&response)?;
                planned(
                    "stats.survival.competing",
                    &input,
                    Some("competing-risks analysis".into()),
                    &result,
                )
            }
        },
        StatsCommand::Model { command } => match command {
            StatsModelCommand::Poisson(args) => {
                let input = load_rows(args.data.as_ref(), args.analysis.as_ref())?;
                let result = poisson_glm_csv(
                    &input.rows,
                    &input.headers,
                    &args.outcome,
                    &args.predictors,
                    args.offset.as_deref(),
                    args.exposure.as_deref(),
                    cli.alpha,
                    cli.na_strategy,
                )?;
                planned(
                    "stats.model.poisson",
                    &input,
                    Some("Poisson GLM".into()),
                    &result,
                )
            }
            StatsModelCommand::Ordinal(args) => {
                let input = load_rows(args.data.as_ref(), args.analysis.as_ref())?;
                require_python_engine("model.ordinal", cli.engine)?;
                let response = run_python_bridge(
                    "ordinal_logit",
                    &input,
                    serde_json::json!({
                        "outcome": &args.outcome,
                        "predictors": &args.predictors,
                        "ci_level": 1.0 - cli.alpha,
                    }),
                )?;
                let result = bridge_to_ordinal_logit(&response)?;
                planned(
                    "stats.model.ordinal",
                    &input,
                    Some("ordinal logistic regression".into()),
                    &result,
                )
            }
            StatsModelCommand::Multinomial(args) => {
                let input = load_rows(args.data.as_ref(), args.analysis.as_ref())?;
                require_python_engine("model.multinomial", cli.engine)?;
                let response = run_python_bridge(
                    "multinomial_logit",
                    &input,
                    serde_json::json!({
                        "outcome": &args.outcome,
                        "predictors": &args.predictors,
                        "reference": &args.reference,
                        "ci_level": 1.0 - cli.alpha,
                    }),
                )?;
                let result = bridge_to_multinomial_logit(&response)?;
                planned(
                    "stats.model.multinomial",
                    &input,
                    Some("multinomial logistic regression".into()),
                    &result,
                )
            }
        },
        StatsCommand::Mixed(args) => {
            let input = load_rows(args.data.as_ref(), args.analysis.as_ref())?;
            require_python_engine("mixed", cli.engine)?;
            let response = run_python_bridge(
                "mixed_effects",
                &input,
                serde_json::json!({
                    "outcome": &args.outcome,
                    "predictors": &args.predictors,
                    "random": &args.random,
                    "ci_level": 1.0 - cli.alpha,
                }),
            )?;
            let result = bridge_to_mixed_lmm(&response)?;
            planned(
                "stats.mixed",
                &input,
                Some("linear mixed-effects model".into()),
                &result,
            )
        }
    }
}

struct StatsInput {
    data_path: PathBuf,
    analysis_path: Option<PathBuf>,
    headers: csv::StringRecord,
    rows: Vec<csv::StringRecord>,
}

fn load_rows(data: Option<&PathBuf>, analysis: Option<&PathBuf>) -> Result<StatsInput, String> {
    let (data_path, analysis_path) = resolve_data_path(data, analysis)?;
    read_rows(data_path, analysis_path)
}

fn read_rows(data_path: PathBuf, analysis_path: Option<PathBuf>) -> Result<StatsInput, String> {
    let mut reader = csv::Reader::from_path(&data_path).map_err(stringify_error)?;
    let headers = reader.headers().map_err(stringify_error)?.clone();
    let rows = reader
        .records()
        .collect::<Result<Vec<_>, _>>()
        .map_err(stringify_error)?;
    Ok(StatsInput {
        data_path,
        analysis_path,
        headers,
        rows,
    })
}

fn planned<T: Serialize>(
    command: &str,
    input: &StatsInput,
    formula: Option<String>,
    result: &T,
) -> Result<PlannedCommandResult, String> {
    let value = serde_json::to_value(result).map_err(stringify_error)?;
    Ok(PlannedCommandResult {
        status: "ok".to_string(),
        command: command.to_string(),
        data_path: input.data_path.display().to_string(),
        analysis_path: input
            .analysis_path
            .as_ref()
            .map(|p| p.display().to_string()),
        formula,
        expected_outputs: summarize_value(&value),
        notes: notes_from_value(&value),
        result: Some(value),
    })
}

fn planned_no_data<T: Serialize>(
    command: &str,
    formula: Option<String>,
    result: &T,
) -> Result<PlannedCommandResult, String> {
    let value = serde_json::to_value(result).map_err(stringify_error)?;
    Ok(PlannedCommandResult {
        status: "ok".to_string(),
        command: command.to_string(),
        data_path: String::new(),
        analysis_path: None,
        formula,
        expected_outputs: summarize_value(&value),
        notes: notes_from_value(&value),
        result: Some(value),
    })
}

fn summarize_value(value: &Value) -> Vec<String> {
    let mut out = Vec::new();
    for key in [
        "n_used",
        "p_value",
        "f_statistic",
        "t_statistic",
        "z_statistic",
        "odds_ratio",
        "relative_risk",
        "standardized_rate",
        "smr",
        "kappa",
        "bias",
        "total_n",
    ] {
        if let Some(v) = value.get(key) {
            out.push(format!("{key}={}", render_scalar(v)));
        }
    }
    if out.is_empty() {
        out.push("result available in JSON payload".to_string());
    }
    out
}

fn notes_from_value(value: &Value) -> Vec<String> {
    value
        .get("notes")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn render_scalar(value: &Value) -> String {
    if let Some(number) = value.as_f64() {
        if number.is_finite() {
            return format!("{number:.6}");
        }
    }
    value.to_string()
}

fn require_python_engine(method: &str, engine: Engine) -> Result<(), String> {
    match engine {
        Engine::Python => Ok(()),
        Engine::Rust => Err(
            "This method requires --engine python. Native Rust implementation is planned but not yet available."
                .to_string(),
        ),
        Engine::R => Err(format!("R engine not yet implemented for {method}")),
    }
}

fn run_python_bridge(
    command: &str,
    input: &StatsInput,
    params: Value,
) -> Result<crate::bridge::BridgeResponse, String> {
    execute_bridge(
        &BridgeRequest {
            command: command.to_string(),
            data_path: input.data_path.display().to_string(),
            params,
            output_format: "statscode_v1".to_string(),
        },
        &BridgeConfig::default(),
    )
}

#[cfg(test)]
fn stats_method_name(command: &StatsCommand) -> &'static str {
    match command {
        StatsCommand::Ttest { command } => match command {
            TtestCommand::Paired(_) => "ttest.paired",
            TtestCommand::OneSample(_) => "ttest.one_sample",
        },
        StatsCommand::Anova { command } => match command {
            AnovaCommand::Oneway(_) => "anova.oneway",
            AnovaCommand::Repeated(_) => "anova.repeated",
            AnovaCommand::Posthoc(_) => "anova.posthoc",
        },
        StatsCommand::Nonparam { command } => match command {
            NonparamCommand::Mcnemar(_) => "nonparam.mcnemar",
            NonparamCommand::Wilcoxon(_) => "nonparam.wilcoxon",
            NonparamCommand::Mannwhitney(_) => "nonparam.mannwhitney",
            NonparamCommand::CochranArmitage(_) => "nonparam.cochran_armitage",
        },
        StatsCommand::Diagnostic { command } => match command {
            DiagnosticStatsCommand::Normality(_) => "diagnostic.normality",
            DiagnosticStatsCommand::Variance(_) => "diagnostic.variance",
        },
        StatsCommand::Epi { command } => match command {
            EpiStatsCommand::OrRr(_) => "epi.or_rr",
            EpiStatsCommand::Standardize(_) => "epi.standardize",
            EpiStatsCommand::Attributable(_) => "epi.attributable",
            EpiStatsCommand::DoseResponse(_) => "epi.dose_response",
        },
        StatsCommand::Agreement { command } => match command {
            AgreementCommand::Kappa(_) => "agreement.kappa",
            AgreementCommand::BlandAltman(_) => "agreement.bland_altman",
        },
        StatsCommand::Multivariate { command } => match command {
            MultivariateCommand::Pca(_) => "multivariate.pca",
            MultivariateCommand::Lda(_) => "multivariate.lda",
            MultivariateCommand::Cluster(_) => "multivariate.cluster",
        },
        StatsCommand::SampleSize { command } => match command {
            SampleSizeCommand::LogRank(_) => "sample_size.log_rank",
        },
        StatsCommand::Survival { command } => match command {
            StatsSurvivalCommand::Lifetable(_) => "survival.lifetable",
            StatsSurvivalCommand::Competing(_) => "survival.competing",
        },
        StatsCommand::Model { command } => match command {
            StatsModelCommand::Poisson(_) => "model.poisson",
            StatsModelCommand::Ordinal(_) => "model.ordinal",
            StatsModelCommand::Multinomial(_) => "model.multinomial",
        },
        StatsCommand::Correlation(_) => "correlation",
        StatsCommand::Meta(_) => "meta",
        StatsCommand::Mixed(_) => "mixed",
        StatsCommand::Psm(_) => "psm",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::{StatsCorrelationArgs, TtestPairedArgs};

    #[test]
    fn stats_method_name_covers_basic_leaves() {
        let paired = StatsCommand::Ttest {
            command: TtestCommand::Paired(TtestPairedArgs {
                data: None,
                analysis: None,
                before: "pre".into(),
                after: "post".into(),
            }),
        };
        assert_eq!(stats_method_name(&paired), "ttest.paired");

        let corr = StatsCommand::Correlation(StatsCorrelationArgs {
            data: None,
            analysis: None,
            x: "x".into(),
            y: "y".into(),
            method: "both".into(),
        });
        assert_eq!(stats_method_name(&corr), "correlation");
    }
}
