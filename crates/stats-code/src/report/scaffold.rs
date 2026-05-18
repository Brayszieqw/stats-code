use std::fmt::Write as _;
use std::path::Path;

use serde_json::{json, Value};

use crate::schema::{format_variable_kind, AnalysisKind, AnalysisSpec, ModelKind, VariableRole};
pub fn build_command_log(spec: &AnalysisSpec) -> Value {
    let commands = spec
        .analyses
        .iter()
        .map(|step| match step.kind {
            AnalysisKind::Inspect => {
                json!({ "command": "stats-code inspect", "status": "planned" })
            }
            AnalysisKind::TableOne => json!({
                "command": "stats-code tableone",
                "by": step.by,
                "status": "planned"
            }),
            AnalysisKind::Rate => json!({
                "command": "stats-code rate",
                "event": step.event,
                "person_time": step.person_time,
                "status": "planned"
            }),
            AnalysisKind::TtestPaired => json!({
                "command": "stats-code stats ttest paired",
                "before": step.before,
                "after": step.after,
                "status": "planned"
            }),
            AnalysisKind::TtestOneSample => json!({
                "command": "stats-code stats ttest one-sample",
                "var": step.var,
                "mu": step.mu,
                "status": "planned"
            }),
            AnalysisKind::AnovaOneway => json!({
                "command": "stats-code stats anova oneway",
                "var": step.var,
                "group": step.group,
                "block": step.block,
                "status": "planned"
            }),
            AnalysisKind::NonparamCochranArmitage => json!({
                "command": "stats-code stats nonparam cochran-armitage",
                "exposure": step.exposure,
                "outcome": step.outcome,
                "scores": step.scores,
                "status": "planned"
            }),
            AnalysisKind::NonparamMcnemar => json!({
                "command": "stats-code stats nonparam mcnemar",
                "var1": step.var1,
                "var2": step.var2,
                "status": "planned"
            }),
            AnalysisKind::NonparamWilcoxon => json!({
                "command": "stats-code stats nonparam wilcoxon",
                "var1": step.var1,
                "var2": step.var2,
                "status": "planned"
            }),
            AnalysisKind::NonparamMannwhitney => json!({
                "command": "stats-code stats nonparam mannwhitney",
                "var": step.var,
                "group": step.group,
                "status": "planned"
            }),
            AnalysisKind::Correlation => json!({
                "command": "stats-code stats correlation",
                "x": step.x,
                "y": step.y,
                "method": step.method,
                "status": "planned"
            }),
            AnalysisKind::EpiOrRr => json!({
                "command": "stats-code stats epi or-rr",
                "exposure": step.exposure,
                "outcome": step.outcome,
                "strata": step.strata,
                "status": "planned"
            }),
            AnalysisKind::EpiStandardize => json!({
                "command": "stats-code stats epi standardize",
                "event": step.event,
                "person_time": step.person_time,
                "age_group": step.age_group,
                "standard_pop": step.standard_pop,
                "status": "planned"
            }),
            AnalysisKind::EpiAttributable => json!({
                "command": "stats-code stats epi attributable",
                "exposure": step.exposure,
                "outcome": step.outcome,
                "person_time": step.person_time,
                "status": "planned"
            }),
            AnalysisKind::DiagnosticNormality => json!({
                "command": "stats-code stats diagnostic normality",
                "var": step.var,
                "status": "planned"
            }),
            AnalysisKind::DiagnosticVariance => json!({
                "command": "stats-code stats diagnostic variance",
                "var": step.var,
                "group": step.group,
                "center": step.center,
                "status": "planned"
            }),
            AnalysisKind::SurvivalLifetable => json!({
                "command": "stats-code stats survival lifetable",
                "input_format": step.input_format,
                "time": step.time,
                "status_column": step.status,
                "intervals": step.intervals,
                "status": "planned"
            }),
            AnalysisKind::Model => json!({
                "command": "stats-code model",
                "model": step.model,
                "outcome": step.outcome,
                "time": step.time,
                "event": step.event,
                "predictors": step.predictors,
                "adjust": step.adjust,
                "status": "planned"
            }),
            _ => json!({ "command": format!("stats-code {:?}", step.kind), "status": "planned" }),
        })
        .collect::<Vec<_>>();
    Value::Array(commands)
}

pub fn build_analysis_manifest(
    spec: &AnalysisSpec,
    analysis_path: &Path,
    data_path: &Path,
    analysis_fingerprint: Option<&str>,
    data_fingerprint: Option<&str>,
) -> Value {
    let checklist = study_context_checklist(spec);
    json!({
        "schema_version": spec.schema_version.as_deref().unwrap_or("stats-code.v0"),
        "stats_code_version": env!("CARGO_PKG_VERSION"),
        "analysis_path": analysis_path.display().to_string(),
        "analysis_fingerprint_fnv1a64": analysis_fingerprint,
        "data_path": data_path.display().to_string(),
        "data_fingerprint_fnv1a64": data_fingerprint,
        "study": {
            "title": &spec.study.title,
            "design": &spec.study.design,
            "population": &spec.study.population,
        },
        "study_context": {
            "estimand": &spec.study_context.estimand,
            "exposure": &spec.study_context.exposure,
            "comparator": &spec.study_context.comparator,
            "outcome": &spec.study_context.outcome,
            "time_zero": &spec.study_context.time_zero,
            "follow_up": &spec.study_context.follow_up,
            "censoring": &spec.study_context.censoring,
            "missing_data_strategy": &spec.study_context.missing_data_strategy,
            "clustering": &spec.study_context.clustering,
            "sensitivity_analyses": &spec.study_context.sensitivity_analyses,
            "reporting_guideline": &spec.study_context.reporting_guideline,
        },
        "reporting": {
            "recommended_guideline": recommended_reporting_guideline(&spec.study.design),
            "declared_guideline": &spec.study_context.reporting_guideline,
            "summary": {
                "present": checklist.iter().filter(|item| item.status == "present").count(),
                "missing": checklist.iter().filter(|item| item.status == "missing").count(),
                "recommended": checklist.iter().filter(|item| item.status == "recommended").count(),
            },
            "checklist": checklist.into_iter().map(|item| json!({
                "field": item.field,
                "status": item.status,
                "value": item.value,
                "note": item.note,
            })).collect::<Vec<_>>(),
        }
    })
}

pub fn build_study_context_markdown(spec: &AnalysisSpec) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "# Study Context");
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "- Recommended reporting guideline: {}",
        recommended_reporting_guideline(&spec.study.design)
    );
    if let Some(guideline) = &spec.study_context.reporting_guideline {
        let _ = writeln!(out, "- Declared reporting guideline: {guideline}");
    }

    for item in study_context_checklist(spec) {
        let _ = writeln!(
            out,
            "- {}: {}{}",
            item.field,
            item.value.unwrap_or_else(|| format!("<{}>", item.status)),
            if item.note.is_empty() {
                String::new()
            } else {
                format!(" ({})", item.note)
            }
        );
    }
    out
}

pub fn build_reporting_checklist_markdown(spec: &AnalysisSpec) -> String {
    let checklist = study_context_checklist(spec);
    let present = checklist
        .iter()
        .filter(|item| item.status == "present")
        .count();
    let missing = checklist
        .iter()
        .filter(|item| item.status == "missing")
        .count();
    let recommended = checklist
        .iter()
        .filter(|item| item.status == "recommended")
        .count();
    let mut out = String::new();
    let _ = writeln!(out, "# Reporting Checklist");
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "- Recommended guideline: {}",
        recommended_reporting_guideline(&spec.study.design)
    );
    let _ = writeln!(
        out,
        "- Declared guideline: {}",
        spec.study_context
            .reporting_guideline
            .as_deref()
            .unwrap_or("<missing>")
    );
    let _ = writeln!(
        out,
        "- Summary: present={present}, missing={missing}, recommended={recommended}"
    );
    let _ = writeln!(out);
    let _ = writeln!(out, "| Item | Status | Value | Note |");
    let _ = writeln!(out, "| --- | --- | --- | --- |");
    for item in checklist {
        let _ = writeln!(
            out,
            "| {} | {} | {} | {} |",
            item.field,
            item.status,
            item.value.unwrap_or_else(|| "<none>".to_string()),
            item.note
        );
    }
    out
}

pub fn build_methods_markdown(spec: &AnalysisSpec) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "# Methods");
    let _ = writeln!(out);
    let _ = writeln!(out, "- Study: {}", spec.study.title);
    let _ = writeln!(out, "- Design: {}", spec.study.design);
    if let Some(population) = &spec.study.population {
        let _ = writeln!(out, "- Population: {population}");
    }
    for item in study_context_checklist(spec)
        .into_iter()
        .filter(|item| item.status == "present")
    {
        let _ = writeln!(out, "- {}: {}", item.field, item.value.unwrap_or_default());
    }
    let _ = writeln!(out, "- Data source: {}", spec.data.path.display());
    let _ = writeln!(out, "- Data format: {:?}", spec.data.format);
    if let Some(dictionary_path) = &spec.data.dictionary_path {
        let _ = writeln!(out, "- Variable dictionary: {}", dictionary_path.display());
    }
    if let Some(survey) = &spec.survey {
        let _ = writeln!(out, "- Survey design:");
        if let Some(weight) = &survey.weight {
            let _ = writeln!(out, "  - Weight: `{weight}`");
        }
        if let Some(strata) = &survey.strata {
            let _ = writeln!(out, "  - Strata: `{strata}`");
        }
        if let Some(cluster) = &survey.cluster {
            let _ = writeln!(out, "  - Cluster: `{cluster}`");
        }
        if let Some(estimator) = &survey.variance_estimator {
            let _ = writeln!(out, "  - Variance estimator: `{estimator}`");
        }
        let _ = writeln!(
            out,
            "  - Note: supported deterministic Rust engines apply survey weights to point estimates; complex-survey variance still requires explicit review."
        );
    }
    if let Some(privacy) = &spec.privacy {
        let _ = writeln!(
            out,
            "- Privacy controls: deidentify={}, direct_identifiers=[{}], quasi_identifiers=[{}]",
            privacy.deidentify,
            privacy.direct_identifiers.join(", "),
            privacy.quasi_identifiers.join(", ")
        );
        let _ = writeln!(
            out,
            "  - Note: report markdown applies small-cell suppression when configured; de-identification and identifier removal still require explicit review."
        );
    }
    let _ = writeln!(out);
    let _ = writeln!(out, "## Planned Analyses");
    for step in &spec.analyses {
        match step.kind {
            AnalysisKind::Inspect => {
                let _ = writeln!(
                    out,
                    "- Dataset inspection with missingness and coding checks."
                );
            }
            AnalysisKind::TableOne => {
                let _ = writeln!(
                    out,
                    "- Table 1 baseline summary stratified by `{}`.",
                    step.by.as_deref().unwrap_or("<unspecified>")
                );
            }
            AnalysisKind::Rate => {
                let _ = writeln!(
                    out,
                    "- Rate analysis using event `{}` and person-time `{}`.",
                    step.event.as_deref().unwrap_or("<unspecified>"),
                    step.person_time.as_deref().unwrap_or("<unspecified>")
                );
            }
            AnalysisKind::TtestPaired => {
                let _ = writeln!(
                    out,
                    "- Paired t-test comparing `{}` and `{}`.",
                    step.before.as_deref().unwrap_or("<unspecified>"),
                    step.after.as_deref().unwrap_or("<unspecified>")
                );
            }
            AnalysisKind::TtestOneSample => {
                let _ = writeln!(
                    out,
                    "- One-sample t-test for `{}` against mu={}.",
                    step.var.as_deref().unwrap_or("<unspecified>"),
                    step.mu
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| "<unspecified>".to_string())
                );
            }
            AnalysisKind::AnovaOneway => {
                let _ = writeln!(
                    out,
                    "- One-way ANOVA for `{}` grouped by `{}`.",
                    step.var.as_deref().unwrap_or("<unspecified>"),
                    step.group.as_deref().unwrap_or("<unspecified>")
                );
            }
            AnalysisKind::NonparamCochranArmitage => {
                let _ = writeln!(
                    out,
                    "- Cochran-Armitage trend test for exposure `{}` and outcome `{}`.",
                    step.exposure.as_deref().unwrap_or("<unspecified>"),
                    step.outcome.as_deref().unwrap_or("<unspecified>")
                );
            }
            AnalysisKind::NonparamMcnemar => {
                let _ = writeln!(
                    out,
                    "- McNemar test comparing paired binary variables `{}` and `{}`.",
                    step.var1.as_deref().unwrap_or("<unspecified>"),
                    step.var2.as_deref().unwrap_or("<unspecified>")
                );
            }
            AnalysisKind::NonparamWilcoxon => {
                let _ = writeln!(
                    out,
                    "- Wilcoxon signed-rank test comparing `{}` and `{}`.",
                    step.var1.as_deref().unwrap_or("<unspecified>"),
                    step.var2.as_deref().unwrap_or("<unspecified>")
                );
            }
            AnalysisKind::NonparamMannwhitney => {
                let _ = writeln!(
                    out,
                    "- Mann-Whitney U test for `{}` grouped by `{}`.",
                    step.var.as_deref().unwrap_or("<unspecified>"),
                    step.group.as_deref().unwrap_or("<unspecified>")
                );
            }
            AnalysisKind::Correlation => {
                let _ = writeln!(
                    out,
                    "- Correlation between `{}` and `{}`.",
                    step.x.as_deref().unwrap_or("<unspecified>"),
                    step.y.as_deref().unwrap_or("<unspecified>")
                );
            }
            AnalysisKind::EpiOrRr => {
                let _ = writeln!(
                    out,
                    "- Odds ratio / relative risk for exposure `{}` and outcome `{}` stratified by `{}`.",
                    step.exposure.as_deref().unwrap_or("<unspecified>"),
                    step.outcome.as_deref().unwrap_or("<unspecified>"),
                    if step.strata.is_empty() {
                        "<none>".to_string()
                    } else {
                        step.strata.join(", ")
                    }
                );
            }
            AnalysisKind::EpiStandardize => {
                let _ = writeln!(
                    out,
                    "- Direct/indirect standardization for event `{}` by `{}`.",
                    step.event.as_deref().unwrap_or("<unspecified>"),
                    step.age_group.as_deref().unwrap_or("<unspecified>")
                );
            }
            AnalysisKind::EpiAttributable => {
                let _ = writeln!(
                    out,
                    "- Attributable risk for exposure `{}` and outcome `{}`.",
                    step.exposure.as_deref().unwrap_or("<unspecified>"),
                    step.outcome.as_deref().unwrap_or("<unspecified>")
                );
            }
            AnalysisKind::DiagnosticNormality => {
                let _ = writeln!(
                    out,
                    "- Normality diagnostics for `{}`.",
                    step.var.as_deref().unwrap_or("<unspecified>")
                );
            }
            AnalysisKind::DiagnosticVariance => {
                let _ = writeln!(
                    out,
                    "- Variance homogeneity diagnostics for `{}` grouped by `{}`.",
                    step.var.as_deref().unwrap_or("<unspecified>"),
                    step.group.as_deref().unwrap_or("<unspecified>")
                );
            }
            AnalysisKind::SurvivalLifetable => {
                let _ = writeln!(
                    out,
                    "- Actuarial life table using `{}` input.",
                    step.input_format.as_deref().unwrap_or("grouped")
                );
            }
            AnalysisKind::Model => match step.model {
                Some(ModelKind::Logistic) => {
                    let _ = writeln!(
                        out,
                        "- Logistic regression for `{}` with predictors `{}`.",
                        step.outcome.as_deref().unwrap_or("<unspecified>"),
                        if step.predictors.is_empty() {
                            "<none>".to_string()
                        } else {
                            step.predictors.join(", ")
                        }
                    );
                }
                Some(ModelKind::Cox) => {
                    let _ = writeln!(
                        out,
                        "- Cox proportional hazards model with time `{}` and event `{}`.",
                        step.time.as_deref().unwrap_or("<unspecified>"),
                        step.event.as_deref().unwrap_or("<unspecified>")
                    );
                }
                Some(ModelKind::Linear) => {
                    let _ = writeln!(
                        out,
                        "- Linear regression (OLS) for `{}` with predictors `{}`.",
                        step.outcome.as_deref().unwrap_or("<unspecified>"),
                        if step.predictors.is_empty() {
                            "<none>".to_string()
                        } else {
                            step.predictors.join(", ")
                        }
                    );
                }
                None => {
                    let _ = writeln!(out, "- Generic model step declared without model type.");
                }
            },
            _ => {
                let _ = writeln!(out, "- {:?} step (analysis details TBD).", step.kind);
            }
        }
    }
    out
}

pub fn build_variables_markdown(spec: &AnalysisSpec) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "# Variable Dictionary");
    let _ = writeln!(out);
    for variable in &spec.variables {
        let roles = if variable.roles.is_empty() {
            "none".to_string()
        } else {
            variable
                .roles
                .iter()
                .map(|role| format_variable_role(*role))
                .collect::<Vec<_>>()
                .join(", ")
        };
        let levels = variable
            .coding
            .as_ref()
            .map(|coding| {
                if coding.levels.is_empty() {
                    String::new()
                } else {
                    format!(", levels=[{}]", coding.levels.join(", "))
                }
            })
            .unwrap_or_default();
        let missing = variable
            .missing
            .as_ref()
            .map(|missing| {
                format!(
                    ", missing_codes=[{}], missing_strategy={}",
                    missing.codes.join(", "),
                    missing.strategy.as_deref().unwrap_or("unspecified")
                )
            })
            .unwrap_or_default();
        let _ = writeln!(
            out,
            "- `{}`: kind=`{}`, roles=`{}`{}{}{}",
            variable.name,
            format_variable_kind(variable.kind),
            roles,
            variable
                .label
                .as_ref()
                .map(|label| format!(", label=\"{label}\""))
                .unwrap_or_default(),
            levels,
            missing
        );
    }
    out
}

pub fn build_report_markdown(spec: &AnalysisSpec) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "# Analysis Report");
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "This report was scaffolded from `analysis.yaml` for `{}`.",
        spec.study.title
    );
    let _ = writeln!(out);
    let _ = writeln!(out, "## Study Context");
    for item in study_context_checklist(spec)
        .into_iter()
        .filter(|item| item.status == "present")
    {
        let _ = writeln!(out, "- {}: {}", item.field, item.value.unwrap_or_default());
    }
    if spec
        .study_context
        .reporting_guideline
        .as_deref()
        .unwrap_or_default()
        .is_empty()
    {
        let _ = writeln!(
            out,
            "- Reporting guideline: <missing>; complete `reporting-checklist.md` before drafting manuscript text."
        );
    }
    let _ = writeln!(out);
    let _ = writeln!(out, "## Results Placeholders");
    let _ = writeln!(out, "- Table 1: baseline characteristics.");
    let _ = writeln!(
        out,
        "- Rate analysis: effect measures and confidence intervals."
    );
    let _ = writeln!(out, "- Regression models: adjusted effect estimates.");
    let _ = writeln!(out);
    let _ = writeln!(out, "## Interpretation Notes");
    let _ = writeln!(
        out,
        "- Replace placeholder text only after CLI outputs are attached."
    );
    let _ = writeln!(out, "- Keep effect sizes, confidence intervals, and assumption checks linked to generated evidence files.");
    let _ = writeln!(out, "- Carry run metadata, data fingerprint, and software versions into manuscript-facing outputs.");
    out
}

pub fn build_assumptions_markdown(spec: &AnalysisSpec) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "# Assumption Checks");
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "- Missingness: {}.",
        spec.study_context
            .missing_data_strategy
            .as_deref()
            .unwrap_or("document complete-case or imputation strategy")
    );
    let _ = writeln!(
        out,
        "- Coding: verify reference levels and ordinal direction."
    );
    if let Some(censoring) = &spec.study_context.censoring {
        let _ = writeln!(
            out,
            "- Censoring: verify `{censoring}` is implemented consistently."
        );
    }
    if let Some(clustering) = &spec.study_context.clustering {
        let _ = writeln!(
            out,
            "- Clustering: confirm analytic handling for `{clustering}`."
        );
    }
    if spec.survey.is_some() {
        let _ = writeln!(out, "- Survey design: confirm weights were applied where supported and review strata, cluster, replicate-weight, and variance-estimator handling before inference.");
    }
    for step in &spec.analyses {
        if step.kind != AnalysisKind::Model {
            continue;
        }
        match step.model {
            Some(ModelKind::Logistic) => {
                let _ = writeln!(
                    out,
                    "- Logistic model: check separation, EPV, collinearity, calibration, ROC."
                );
            }
            Some(ModelKind::Cox) => {
                let _ = writeln!(out, "- Cox model: check proportional hazards, influential observations, functional form.");
            }
            Some(ModelKind::Linear) => {
                let _ = writeln!(out, "- Linear model: check normality of residuals, homoscedasticity, multicollinearity (VIF), influential observations.");
            }
            None => {}
        }
    }
    out
}

pub fn build_audit_trail_markdown(spec: &AnalysisSpec) -> String {
    let mut out = String::new();
    let checklist = study_context_checklist(spec);
    let _ = writeln!(out, "# Audit Trail");
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "- Schema version: {}",
        spec.schema_version.as_deref().unwrap_or("stats-code.v0")
    );
    let _ = writeln!(out, "- Study: {}", spec.study.title);
    let _ = writeln!(out, "- Data path: {}", spec.data.path.display());
    let _ = writeln!(out, "- Data format: {:?}", spec.data.format);
    let _ = writeln!(
        out,
        "- Declared analyses: {}",
        spec.analyses
            .iter()
            .map(|step| match step.kind {
                AnalysisKind::Inspect => "inspect".to_string(),
                AnalysisKind::TableOne => "tableone".to_string(),
                AnalysisKind::Rate => "rate".to_string(),
                AnalysisKind::TtestPaired => "stats_ttest_paired".to_string(),
                AnalysisKind::TtestOneSample => "stats_ttest_one_sample".to_string(),
                AnalysisKind::AnovaOneway => "stats_anova_oneway".to_string(),
                AnalysisKind::NonparamCochranArmitage => {
                    "stats_nonparam_cochran_armitage".to_string()
                }
                AnalysisKind::NonparamMcnemar => "stats_nonparam_mcnemar".to_string(),
                AnalysisKind::NonparamWilcoxon => "stats_nonparam_wilcoxon".to_string(),
                AnalysisKind::NonparamMannwhitney => "stats_nonparam_mannwhitney".to_string(),
                AnalysisKind::Correlation => "stats_correlation".to_string(),
                AnalysisKind::EpiOrRr => "stats_epi_or_rr".to_string(),
                AnalysisKind::EpiStandardize => "stats_epi_standardize".to_string(),
                AnalysisKind::EpiAttributable => "stats_epi_attributable".to_string(),
                AnalysisKind::DiagnosticNormality => "stats_diagnostic_normality".to_string(),
                AnalysisKind::DiagnosticVariance => "stats_diagnostic_variance".to_string(),
                AnalysisKind::SurvivalLifetable => "stats_survival_lifetable".to_string(),
                AnalysisKind::Model => match step.model {
                    Some(ModelKind::Logistic) => "model_logistic".to_string(),
                    Some(ModelKind::Cox) => "model_cox".to_string(),
                    Some(ModelKind::Linear) => "model_linear".to_string(),
                    None => "model".to_string(),
                },
                _ => format!("{:?}", step.kind).to_ascii_lowercase(),
            })
            .collect::<Vec<_>>()
            .join(", ")
    );
    let _ = writeln!(
        out,
        "- Reporting guideline: recommended={}, declared={}",
        recommended_reporting_guideline(&spec.study.design),
        spec.study_context
            .reporting_guideline
            .as_deref()
            .unwrap_or("<missing>")
    );
    let _ = writeln!(
        out,
        "- Study context completeness: present={}, missing={}, recommended={}",
        checklist
            .iter()
            .filter(|item| item.status == "present")
            .count(),
        checklist
            .iter()
            .filter(|item| item.status == "missing")
            .count(),
        checklist
            .iter()
            .filter(|item| item.status == "recommended")
            .count()
    );
    if let Some(audit) = &spec.audit {
        let _ = writeln!(
            out,
            "- Audit policy: save_commands={}, save_inputs={}, save_outputs={}, save_environment={}, save_decisions={}",
            audit.save_commands,
            audit.save_inputs,
            audit.save_outputs,
            audit.save_environment,
            audit.save_decisions
        );
    }
    if let Some(privacy) = &spec.privacy {
        let _ = writeln!(
            out,
            "- Privacy policy: deidentify={}, direct_identifiers=[{}], quasi_identifiers=[{}], small_cell_threshold={}",
            privacy.deidentify,
            privacy.direct_identifiers.join(", "),
            privacy.quasi_identifiers.join(", "),
            privacy
                .small_cell_threshold.map_or_else(|| "unspecified".to_string(), |value| value.to_string())
        );
    }
    let _ = writeln!(
        out,
        "- Execution policy: deterministic CLI first, agent layer optional and off by default."
    );
    let _ = writeln!(out, "- Safety policy: no network access or arbitrary command execution is assumed for statistical runs.");
    out
}

pub fn build_tables_readme(spec: &AnalysisSpec) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "# Tables");
    let _ = writeln!(out);
    let _ = writeln!(out, "Expected table outputs for `{}`:", spec.study.title);
    for step in &spec.analyses {
        match step.kind {
            AnalysisKind::TableOne => {
                let _ = writeln!(out, "- `tableone.csv`");
            }
            AnalysisKind::Rate => {
                let _ = writeln!(out, "- `rate-summary.csv`");
            }
            AnalysisKind::TtestPaired => {
                let _ = writeln!(out, "- `stats-ttest-paired.json`");
            }
            AnalysisKind::TtestOneSample => {
                let _ = writeln!(out, "- `stats-ttest-one-sample.json`");
            }
            AnalysisKind::AnovaOneway => {
                let _ = writeln!(out, "- `stats-anova-oneway.json`");
            }
            AnalysisKind::NonparamCochranArmitage => {
                let _ = writeln!(out, "- `stats-nonparam-cochran-armitage.json`");
            }
            AnalysisKind::NonparamMcnemar => {
                let _ = writeln!(out, "- `stats-nonparam-mcnemar.json`");
            }
            AnalysisKind::NonparamWilcoxon => {
                let _ = writeln!(out, "- `stats-nonparam-wilcoxon.json`");
            }
            AnalysisKind::NonparamMannwhitney => {
                let _ = writeln!(out, "- `stats-nonparam-mannwhitney.json`");
            }
            AnalysisKind::Correlation => {
                let _ = writeln!(out, "- `stats-correlation.json`");
            }
            AnalysisKind::EpiOrRr => {
                let _ = writeln!(out, "- `stats-epi-or-rr.json`");
            }
            AnalysisKind::EpiStandardize => {
                let _ = writeln!(out, "- `stats-epi-standardize.json`");
            }
            AnalysisKind::EpiAttributable => {
                let _ = writeln!(out, "- `stats-epi-attributable.json`");
            }
            AnalysisKind::DiagnosticNormality => {
                let _ = writeln!(out, "- `stats-diagnostic-normality.json`");
            }
            AnalysisKind::DiagnosticVariance => {
                let _ = writeln!(out, "- `stats-diagnostic-variance.json`");
            }
            AnalysisKind::SurvivalLifetable => {
                let _ = writeln!(out, "- `stats-survival-lifetable.json`");
            }
            AnalysisKind::Model => match step.model {
                Some(ModelKind::Logistic) => {
                    let _ = writeln!(out, "- `model-logistic-coefficients.csv`");
                }
                Some(ModelKind::Cox) => {
                    let _ = writeln!(out, "- `model-cox-coefficients.csv`");
                }
                Some(ModelKind::Linear) => {
                    let _ = writeln!(out, "- `model-linear-coefficients.csv`");
                }
                None => {}
            },
            AnalysisKind::Inspect => {}
            _ => {}
        }
    }
    out
}

fn format_variable_role(role: VariableRole) -> &'static str {
    match role {
        VariableRole::Outcome => "outcome",
        VariableRole::Exposure => "exposure",
        VariableRole::Covariate => "covariate",
        VariableRole::Strata => "strata",
        VariableRole::Time => "time",
        VariableRole::Event => "event",
        VariableRole::Id => "id",
        VariableRole::Weight => "weight",
        VariableRole::Cluster => "cluster",
    }
}

#[derive(Clone)]
struct ChecklistItem {
    field: &'static str,
    status: &'static str,
    value: Option<String>,
    note: &'static str,
}

fn study_context_checklist(spec: &AnalysisSpec) -> Vec<ChecklistItem> {
    let needs_time_anchor = requires_time_anchor(spec);
    let needs_comparator = requires_comparator(spec);
    let needs_clustering = requires_clustering(spec);
    let context = &spec.study_context;
    vec![
        checklist_item(
            "estimand",
            context.estimand.clone(),
            true,
            "Target effect measure or quantity of interest.",
        ),
        checklist_item(
            "exposure",
            context.exposure.clone(),
            true,
            "Primary exposure or intervention.",
        ),
        checklist_item(
            "comparator",
            context.comparator.clone(),
            needs_comparator,
            "Comparator arm or reference strategy.",
        ),
        checklist_item(
            "outcome",
            context.outcome.clone(),
            true,
            "Outcome definition aligned with analysis outputs.",
        ),
        checklist_item(
            "time_zero",
            context.time_zero.clone(),
            needs_time_anchor,
            "Index date or start of follow-up.",
        ),
        checklist_item(
            "follow_up",
            context.follow_up.clone(),
            needs_time_anchor,
            "Follow-up window or stopping rule.",
        ),
        checklist_item(
            "censoring",
            context.censoring.clone(),
            needs_time_anchor,
            "Administrative or informative censoring rules.",
        ),
        checklist_item(
            "missing_data_strategy",
            context.missing_data_strategy.clone(),
            true,
            "Complete-case, imputation, or other handling plan.",
        ),
        checklist_item(
            "clustering",
            context.clustering.clone(),
            needs_clustering,
            "Clustered, repeated, or survey-aware analysis structure.",
        ),
        checklist_item(
            "sensitivity_analyses",
            context.sensitivity_analyses.clone(),
            false,
            "Planned robustness or bias analyses.",
        ),
        checklist_item(
            "reporting_guideline",
            context.reporting_guideline.clone(),
            true,
            "STROBE, RECORD, CONSORT, TRIPOD, or another declared guideline.",
        ),
    ]
}

fn checklist_item(
    field: &'static str,
    value: Option<String>,
    required: bool,
    note: &'static str,
) -> ChecklistItem {
    let normalized = value.and_then(|value| {
        let trimmed = value.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_string())
    });
    ChecklistItem {
        field,
        status: match (normalized.is_some(), required) {
            (true, _) => "present",
            (false, true) => "missing",
            (false, false) => "recommended",
        },
        value: normalized,
        note,
    }
}

fn recommended_reporting_guideline(design: &str) -> &'static str {
    let normalized = design.to_ascii_lowercase();
    if normalized.contains("trial") || normalized.contains("random") {
        "CONSORT"
    } else if normalized.contains("prediction")
        || normalized.contains("prognostic")
        || normalized.contains("diagnostic")
    {
        "TRIPOD"
    } else {
        "STROBE"
    }
}

fn requires_time_anchor(spec: &AnalysisSpec) -> bool {
    spec.analyses.iter().any(|step| {
        matches!(step.model, Some(ModelKind::Cox))
            || matches!(step.kind, AnalysisKind::Rate)
            || matches!(step.kind, AnalysisKind::EpiOrRr)
            || step.time.is_some()
            || step.event.is_some()
            || step.person_time.is_some()
    })
}

fn requires_comparator(spec: &AnalysisSpec) -> bool {
    spec.variables
        .iter()
        .any(|variable| variable.roles.contains(&VariableRole::Exposure))
        || spec.study.design.to_ascii_lowercase().contains("trial")
}

fn requires_clustering(spec: &AnalysisSpec) -> bool {
    spec.survey
        .as_ref()
        .and_then(|survey| survey.cluster.as_ref())
        .is_some()
        || spec
            .variables
            .iter()
            .any(|variable| variable.roles.contains(&VariableRole::Cluster))
}
