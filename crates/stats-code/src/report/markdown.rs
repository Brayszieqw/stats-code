use std::fmt::Write as _;
use std::path::Path;

use crate::render::format_p_value;
use crate::schema::{
    AnalysisKind, AnalysisSpec, CoxResult, LinearResult, LogisticResult, ModelKind, RateResult,
    TableOneResult,
};

use super::scaffold::build_tables_readme;
use super::{fs, stringify_error, ReportEvidence};
pub(super) const MODEL_SCIENTIFIC_NOTATION_ABS: f64 = 1.0e6;
pub(super) const MODEL_SMALL_SCIENTIFIC_NOTATION_ABS: f64 = 1.0e-4;
pub(super) const MODEL_UNSTABLE_INTERVAL_ABS: f64 = 1.0e100;

pub(super) fn format_model_number(value: f64, precision: usize) -> String {
    if !value.is_finite() {
        return "NA".to_string();
    }
    let abs = value.abs();
    if abs != 0.0
        && !(MODEL_SMALL_SCIENTIFIC_NOTATION_ABS..MODEL_SCIENTIFIC_NOTATION_ABS).contains(&abs)
    {
        format!("{value:.precision$e}")
    } else {
        format!("{value:.precision$}")
    }
}

pub(super) fn has_unstable_model_interval(lower: f64, upper: f64) -> bool {
    !lower.is_finite()
        || !upper.is_finite()
        || lower.abs() >= MODEL_UNSTABLE_INTERVAL_ABS
        || upper.abs() >= MODEL_UNSTABLE_INTERVAL_ABS
        || lower > upper
}

pub(super) fn format_model_ci(lower: f64, upper: f64, precision: usize) -> String {
    if has_unstable_model_interval(lower, upper) {
        "unstable".to_string()
    } else {
        format!(
            "[{}, {}]",
            format_model_number(lower, precision),
            format_model_number(upper, precision)
        )
    }
}

pub(super) fn format_model_ci_phrase(lower: f64, upper: f64, precision: usize) -> String {
    if has_unstable_model_interval(lower, upper) {
        "(CI unstable)".to_string()
    } else {
        format_model_ci(lower, upper, precision)
    }
}

pub(super) fn write_report_warnings(out: &mut String, label: &str, warnings: &[String]) {
    if !warnings.is_empty() {
        let _ = writeln!(out, "- {label} warnings: {}.", warnings.join(", "));
    }
}

pub(super) fn write_model_table_warnings(out: &mut String, warnings: &[String]) {
    if !warnings.is_empty() {
        let _ = writeln!(out, "- Warnings: {}.", warnings.join(", "));
    }
}

pub(super) fn build_report_markdown_from_evidence(
    spec: &AnalysisSpec,
    evidence: &ReportEvidence,
) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "# Analysis Report");
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "This report was scaffolded from `analysis.yaml` for `{}`.",
        spec.study.title
    );
    let _ = writeln!(out);
    let _ = writeln!(out, "## Evidence");
    let _ = writeln!(out, "- Artifacts source: {}", evidence.source_dir.display());
    let _ = writeln!(
        out,
        "- Discovered commands: {}",
        if evidence.discovered_runs.is_empty() {
            "<none>".to_string()
        } else {
            evidence
                .discovered_runs
                .iter()
                .map(|run| run.command.clone())
                .collect::<Vec<_>>()
                .join(", ")
        }
    );
    if let Some(inspect) = &evidence.inspect {
        let _ = writeln!(
            out,
            "- Dataset inspection: rows={}, columns={}, variables with missingness={}.",
            inspect.rows.unwrap_or(0),
            inspect.columns,
            inspect
                .variables
                .iter()
                .filter(|variable| variable.missing_count > 0)
                .count()
        );
    }
    let _ = writeln!(out);
    let _ = writeln!(out, "## Observed Results");
    if let Some(tableone) = &evidence.tableone {
        let _ = writeln!(
            out,
            "- Table 1 available for `{}` with {} group(s) and {} row(s).",
            tableone.by,
            tableone.group_levels.len(),
            tableone.rows.len()
        );
    } else if declares_tableone(spec) {
        let _ = writeln!(out, "- Table 1: no observed result found.");
    }
    if let Some(rate) = &evidence.rate {
        let top_rows = rate
            .rows
            .iter()
            .take(3)
            .map(|row| format!("{} = {:.2}/1000", row.stratum, row.rate_per_1000))
            .collect::<Vec<_>>()
            .join("; ");
        let _ = writeln!(
            out,
            "- Rate summary for `{}` / `{}`: {}.",
            rate.event,
            rate.person_time,
            if top_rows.is_empty() {
                "<no rows>".to_string()
            } else {
                top_rows
            }
        );
    } else if declares_rate(spec) {
        let _ = writeln!(out, "- Rate analysis: no observed result found.");
    }
    if let Some(logistic) = &evidence.logistic {
        let top_terms = logistic
            .coefficients
            .iter()
            .filter(|coefficient| coefficient.term != "Intercept")
            .take(3)
            .map(|coefficient| {
                format!(
                    "{} OR {} {}",
                    coefficient.term,
                    format_model_number(coefficient.odds_ratio, 2),
                    format_model_ci_phrase(coefficient.ci_lower, coefficient.ci_upper, 2)
                )
            })
            .collect::<Vec<_>>()
            .join("; ");
        let _ = writeln!(
            out,
            "- Logistic model: outcome `{}`, n={}, events={}, {}.",
            logistic.outcome,
            logistic.n_used,
            logistic.n_events,
            if top_terms.is_empty() {
                "no coefficient summary".to_string()
            } else {
                top_terms
            }
        );
        write_report_warnings(&mut out, "Logistic model", &logistic.warnings);
    } else if declares_model(spec, ModelKind::Logistic) {
        let _ = writeln!(out, "- Logistic model: no observed result found.");
    }
    if let Some(cox) = &evidence.cox {
        let top_terms = cox
            .coefficients
            .iter()
            .take(3)
            .map(|coefficient| {
                format!(
                    "{} HR {} {}",
                    coefficient.term,
                    format_model_number(coefficient.hazard_ratio, 2),
                    format_model_ci_phrase(coefficient.ci_lower, coefficient.ci_upper, 2)
                )
            })
            .collect::<Vec<_>>()
            .join("; ");
        let _ = writeln!(
            out,
            "- Cox model: time `{}`, event `{}`, n={}, events={}, {}.",
            cox.time,
            cox.event,
            cox.n_used,
            cox.n_events,
            if top_terms.is_empty() {
                "no coefficient summary".to_string()
            } else {
                top_terms
            }
        );
        write_report_warnings(&mut out, "Cox model", &cox.warnings);
    } else if declares_model(spec, ModelKind::Cox) {
        let _ = writeln!(out, "- Cox model: no observed result found.");
    }
    if let Some(linear) = &evidence.linear {
        let top_terms = linear
            .coefficients
            .iter()
            .take(3)
            .map(|coefficient| {
                format!(
                    "{} β {:.4} [{:.4}, {:.4}]",
                    coefficient.term, coefficient.beta, coefficient.ci_lower, coefficient.ci_upper
                )
            })
            .collect::<Vec<_>>()
            .join("; ");
        let _ = writeln!(
            out,
            "- Linear model: outcome `{}`, n={}, R²={:.4}, {}.",
            linear.outcome,
            linear.n_used,
            linear.r_squared,
            if top_terms.is_empty() {
                "no coefficient summary".to_string()
            } else {
                top_terms
            }
        );
    }
    let _ = writeln!(out);
    let _ = writeln!(out, "## Interpretation Notes");
    let _ = writeln!(out, "- Generated tables in `tables/` are derived from stored command results, not free-text guesses.");
    if let Some(threshold) = small_cell_threshold(spec) {
        let _ = writeln!(
            out,
            "- Small-cell suppression was applied to report markdown tables for positive cells below {threshold}."
        );
    }
    let _ = writeln!(out, "- Carry effect sizes, confidence intervals, and warnings forward into manuscript-facing text.");
    let _ = writeln!(
        out,
        "- Re-run analyses if the data fingerprint or command parameters change."
    );
    out
}

pub(super) fn build_tables_readme_from_evidence(
    spec: &AnalysisSpec,
    evidence: &ReportEvidence,
) -> String {
    let mut out = build_tables_readme(spec);
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "Observed table artifacts from `{}`:",
        evidence.source_dir.display()
    );
    if evidence.tableone.is_some() {
        let _ = writeln!(out, "- `tableone.md`");
    }
    if evidence.rate.is_some() {
        let _ = writeln!(out, "- `rate-summary.md`");
    }
    if evidence.logistic.is_some() {
        let _ = writeln!(out, "- `model-logistic-summary.md`");
    }
    if evidence.cox.is_some() {
        let _ = writeln!(out, "- `model-cox-summary.md`");
    }
    if evidence.linear.is_some() {
        let _ = writeln!(out, "- `model-linear-summary.md`");
    }
    if !evidence.has_any_results() {
        let _ = writeln!(out, "- No observed result files were discovered.");
    }
    out
}

pub(super) fn declared_inspect_step_index(spec: &AnalysisSpec) -> Option<usize> {
    spec.analyses
        .iter()
        .position(|step| matches!(step.kind, AnalysisKind::Inspect))
}

pub(super) fn declares_tableone(spec: &AnalysisSpec) -> bool {
    spec.analyses
        .iter()
        .any(|step| matches!(step.kind, AnalysisKind::TableOne))
}

pub(super) fn declares_rate(spec: &AnalysisSpec) -> bool {
    spec.analyses
        .iter()
        .any(|step| matches!(step.kind, AnalysisKind::Rate))
}

pub(super) fn declares_model(spec: &AnalysisSpec, model: ModelKind) -> bool {
    spec.analyses
        .iter()
        .any(|step| matches!(step.kind, AnalysisKind::Model) && step.model == Some(model))
}

pub(super) fn tableone_declared_step_index(
    spec: &AnalysisSpec,
    result: &TableOneResult,
) -> Option<usize> {
    spec.analyses.iter().position(|step| {
        matches!(step.kind, AnalysisKind::TableOne)
            && optional_string_matches(step.by.as_deref(), &result.by)
    })
}

pub(super) fn rate_declared_step_index(spec: &AnalysisSpec, result: &RateResult) -> Option<usize> {
    spec.analyses.iter().position(|step| {
        matches!(step.kind, AnalysisKind::Rate)
            && optional_string_matches(step.event.as_deref(), &result.event)
            && optional_string_matches(step.person_time.as_deref(), &result.person_time)
            && declared_list_matches(&step.strata, &result.strata)
    })
}

pub(super) fn model_declared_step_index(
    spec: &AnalysisSpec,
    model: ModelKind,
    outcome: Option<&str>,
    time: Option<&str>,
    event: Option<&str>,
    predictors: &[String],
) -> Option<usize> {
    spec.analyses.iter().position(|step| {
        matches!(step.kind, AnalysisKind::Model)
            && step.model == Some(model)
            && optional_string_matches(step.outcome.as_deref(), outcome.unwrap_or_default())
            && optional_string_matches(step.time.as_deref(), time.unwrap_or_default())
            && optional_string_matches(step.event.as_deref(), event.unwrap_or_default())
            && declared_predictors_match(step, predictors)
    })
}

pub(super) fn optional_string_matches(expected: Option<&str>, actual: &str) -> bool {
    expected.is_none_or(|expected| expected == actual)
}

pub(super) fn declared_list_matches(expected: &[String], actual: &[String]) -> bool {
    expected.is_empty()
        || expected
            .iter()
            .all(|expected| actual.iter().any(|value| value == expected))
}

pub(super) fn declared_predictors_match(
    step: &crate::schema::AnalysisStepSpec,
    actual_predictors: &[String],
) -> bool {
    step.predictors
        .iter()
        .chain(step.adjust.iter())
        .all(|expected| actual_predictors.iter().any(|value| value == expected))
}

pub(super) fn small_cell_threshold(spec: &AnalysisSpec) -> Option<usize> {
    spec.privacy
        .as_ref()
        .and_then(|privacy| privacy.small_cell_threshold)
        .filter(|threshold| *threshold > 1)
}

pub(super) fn build_tableone_markdown(
    result: &TableOneResult,
    small_cell_threshold: Option<usize>,
) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "# Table 1");
    let _ = writeln!(out);
    if let Some(threshold) = small_cell_threshold {
        let _ = writeln!(
            out,
            "Positive cells with n < {threshold} are suppressed in this markdown table."
        );
        let _ = writeln!(out);
    }
    let _ = writeln!(
        out,
        "| Variable | Overall | {} |",
        result.group_levels.join(" | ")
    );
    let _ = writeln!(
        out,
        "| --- | --- | {} |",
        result
            .group_levels
            .iter()
            .map(|_| "---")
            .collect::<Vec<_>>()
            .join(" | ")
    );
    for row in &result.rows {
        let label = row.label.as_deref().unwrap_or(&row.variable);
        let name = row
            .level
            .as_ref()
            .map_or_else(|| label.to_string(), |level| format!("{label} = {level}"));
        let group_cells = result
            .group_levels
            .iter()
            .map(|group| {
                row.groups
                    .iter()
                    .find(|cell| &cell.group == group)
                    .map_or_else(
                        || "NA".to_string(),
                        |cell| format_tableone_cell(&cell.cell, small_cell_threshold),
                    )
            })
            .collect::<Vec<_>>()
            .join(" | ");
        let _ = writeln!(
            out,
            "| {name} | {} | {} |",
            format_tableone_cell(&row.overall, small_cell_threshold),
            group_cells
        );
    }
    out
}

pub(super) fn format_tableone_cell(
    cell: &crate::schema::TableOneCell,
    threshold: Option<usize>,
) -> String {
    if let Some(threshold) = threshold {
        if is_small_positive_cell(cell, threshold) {
            return format!("suppressed (<{threshold})");
        }
    }
    cell.display.clone()
}

pub(super) fn is_small_positive_cell(cell: &crate::schema::TableOneCell, threshold: usize) -> bool {
    let count = cell.count.unwrap_or(cell.n_non_missing);
    count > 0 && count < threshold
}

pub(super) fn build_rate_markdown(result: &RateResult) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "# Rate Summary");
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "| Stratum | Events | Person-time | Rate | Rate per 1000 | 95% CI per 1000 |"
    );
    let _ = writeln!(out, "| --- | ---: | ---: | ---: | ---: | --- |");
    for row in &result.rows {
        let _ = writeln!(
            out,
            "| {} | {:.3} | {:.3} | {:.6} | {:.3} | [{:.3}, {:.3}] |",
            row.stratum,
            row.events,
            row.person_time,
            row.rate,
            row.rate_per_1000,
            row.lower_ci_per_1000,
            row.upper_ci_per_1000
        );
    }
    out
}

pub(super) fn build_logistic_markdown(result: &LogisticResult) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "# Logistic Model Summary");
    let _ = writeln!(out);
    let _ = writeln!(out, "- Formula: `{}`", result.formula);
    let _ = writeln!(out, "- Rows used: {}", result.n_used);
    let _ = writeln!(out, "- Events: {}", result.n_events);
    write_model_table_warnings(&mut out, &result.warnings);
    let _ = writeln!(out);
    let _ = writeln!(out, "| Term | OR | 95% CI | p-value |");
    let _ = writeln!(out, "| --- | ---: | --- | ---: |");
    for coefficient in &result.coefficients {
        let p_value = format_p_value(coefficient.p_value);
        let _ = writeln!(
            out,
            "| {} | {} | {} | {} |",
            coefficient.term,
            format_model_number(coefficient.odds_ratio, 4),
            format_model_ci(coefficient.ci_lower, coefficient.ci_upper, 4),
            p_value
        );
    }
    out
}

pub(super) fn build_cox_markdown(result: &CoxResult) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "# Cox Model Summary");
    let _ = writeln!(out);
    let _ = writeln!(out, "- Formula: `{}`", result.formula);
    let _ = writeln!(out, "- Rows used: {}", result.n_used);
    let _ = writeln!(out, "- Events: {}", result.n_events);
    write_model_table_warnings(&mut out, &result.warnings);
    let _ = writeln!(out);
    let _ = writeln!(out, "| Term | HR | 95% CI | p-value |");
    let _ = writeln!(out, "| --- | ---: | --- | ---: |");
    for coefficient in &result.coefficients {
        let p_value = format_p_value(coefficient.p_value);
        let _ = writeln!(
            out,
            "| {} | {} | {} | {} |",
            coefficient.term,
            format_model_number(coefficient.hazard_ratio, 4),
            format_model_ci(coefficient.ci_lower, coefficient.ci_upper, 4),
            p_value
        );
    }
    out
}

pub(super) fn build_linear_markdown(result: &LinearResult) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "# Linear Model Summary");
    let _ = writeln!(out);
    let _ = writeln!(out, "- Formula: `{}`", result.formula);
    let _ = writeln!(out, "- Rows used: {}", result.n_used);
    let _ = writeln!(
        out,
        "- R²: {:.4}, Adjusted R²: {:.4}",
        result.r_squared, result.adjusted_r_squared
    );
    if let Some(f) = result.f_statistic {
        let p_text = result
            .f_p_value
            .map(|p| format!(", p={}", format_p_value(p)))
            .unwrap_or_default();
        let _ = writeln!(out, "- F-statistic: {f:.4}{p_text}");
    }
    let _ = writeln!(out);
    let _ = writeln!(out, "| Term | β | SE | t | p-value | 95% CI |");
    let _ = writeln!(out, "| --- | ---: | ---: | ---: | ---: | --- |");
    for coefficient in &result.coefficients {
        let p_value = format_p_value(coefficient.p_value);
        let _ = writeln!(
            out,
            "| {} | {:.4} | {:.4} | {:.4} | {} | [{:.4}, {:.4}] |",
            coefficient.term,
            coefficient.beta,
            coefficient.standard_error,
            coefficient.t_statistic,
            p_value,
            coefficient.ci_lower,
            coefficient.ci_upper,
        );
    }
    out
}

// ---------------------------------------------------------------------------
// File writing / artifact persistence
// ---------------------------------------------------------------------------

pub(super) fn write_report_file(
    path: &Path,
    content: &str,
    written_files: &mut Vec<String>,
) -> Result<(), String> {
    fs::write(path, content).map_err(stringify_error)?;
    written_files.push(path.display().to_string());
    Ok(())
}
