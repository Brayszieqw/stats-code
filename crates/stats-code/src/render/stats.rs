use std::fmt::Write as _;

use crate::schema::{
    CoxResult, DiagnosticRocResult, LinearResult, LogisticResult,
};

use super::writer::TextReportWriter;
use super::{format_optional_number, format_p_value};

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
        format!("events={} nonevents={}", result.n_events, result.n_nonevents),
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
            result.converged, result.r_squared, result.adjusted_r_squared, result.residual_std_error
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
