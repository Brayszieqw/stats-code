use std::fmt::Write as _;

use crate::schema::{
    format_variable_kind, AiAskResult, AnalysisCheckLevel, AnalysisCheckResult,
    AuditExplainArtifact, AuditExplainResult, AuthDoctorResult, AuthSetResult, ColumnInspection,
    ConfigResult, CoxResult, DiagnosticRocResult, DoctorResult, InitProjectResult, InspectResult,
    LinearResult, LogisticResult, OpenReportResult, PlannedCommandResult, PowerResult, RateResult,
    ReportBuildResult, ReportVerifyResult, SurvivalKmResult, TableOneResult, WorkflowRunResult,
};

pub(crate) fn format_p_value(p: f64) -> String {
    if !p.is_finite() {
        return "NA".to_string();
    }
    if p < 0.001 {
        "<0.001".to_string()
    } else {
        format!("{p:.4}")
    }
}

pub fn render_inspect_text(result: &InspectResult) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "Inspect");
    let _ = writeln!(out, "  Status           {}", result.status);
    let _ = writeln!(out, "  Data path        {}", result.data_path);
    let _ = writeln!(out, "  Format           {:?}", result.format);
    if let Some(rows) = result.rows {
        let _ = writeln!(out, "  Rows             {rows}");
    }
    let _ = writeln!(out, "  Columns          {}", result.columns);
    let _ = writeln!(out, "  Variables");
    for ColumnInspection {
        name,
        inferred_kind,
        missing_count,
        distinct_count,
        sample_values,
        numeric_summary,
        warnings,
        ..
    } in &result.variables
    {
        let numeric_summary = numeric_summary
            .as_ref()
            .map(|summary| {
                format!(
                    " min={:.4} mean={:.4} max={:.4} zeroes={}",
                    summary.min, summary.mean, summary.max, summary.zero_count
                )
            })
            .unwrap_or_default();
        let warning_text = if warnings.is_empty() {
            String::new()
        } else {
            format!(" warnings={}", warnings.join("|"))
        };
        let _ = writeln!(
            out,
            "  - {} [{}] missing={} distinct={} sample={}{}{}",
            name,
            format_variable_kind(*inferred_kind),
            missing_count,
            distinct_count,
            if sample_values.is_empty() {
                "<none>".to_string()
            } else {
                sample_values.join(", ")
            },
            numeric_summary,
            warning_text
        );
    }
    if !result.notes.is_empty() {
        let _ = writeln!(out, "  Notes");
        for note in &result.notes {
            let _ = writeln!(out, "  - {note}");
        }
    }
    out
}

pub fn render_planned_text(result: &PlannedCommandResult) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "Plan");
    let _ = writeln!(out, "  Status           {}", result.status);
    let _ = writeln!(out, "  Command          {}", result.command);
    let _ = writeln!(out, "  Data path        {}", result.data_path);
    if let Some(path) = &result.analysis_path {
        let _ = writeln!(out, "  Analysis         {path}");
    }
    if let Some(formula) = &result.formula {
        let _ = writeln!(out, "  Formula          {formula}");
    }
    let _ = writeln!(out, "  Outputs");
    for output in &result.expected_outputs {
        let _ = writeln!(out, "  - {output}");
    }
    if !result.notes.is_empty() {
        let _ = writeln!(out, "  Notes");
        for note in &result.notes {
            let _ = writeln!(out, "  - {note}");
        }
    }
    out
}

pub fn render_tableone_text(result: &TableOneResult) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "Table 1");
    let _ = writeln!(out, "  Status           {}", result.status);
    let _ = writeln!(out, "  Data path        {}", result.data_path);
    if let Some(path) = &result.analysis_path {
        let _ = writeln!(out, "  Analysis         {path}");
    }
    let _ = writeln!(out, "  By               {}", result.by);
    let _ = writeln!(
        out,
        "  Groups           {}",
        if result.group_levels.is_empty() {
            "<none>".to_string()
        } else {
            result.group_levels.join(", ")
        }
    );
    let _ = writeln!(out, "  Rows");
    for row in &result.rows {
        let label = row.label.as_deref().unwrap_or(&row.variable);
        let row_name = row
            .level
            .as_ref()
            .map_or_else(|| label.to_string(), |level| format!("{label} = {level}"));
        let group_cells = row
            .groups
            .iter()
            .map(|group| format!("{}: {}", group.group, group.cell.display))
            .collect::<Vec<_>>()
            .join(" | ");
        let p_text = match (&row.test_name, row.p_value) {
            (Some(test), Some(p)) => format!(" p={} ({test})", format_p_value(p)),
            _ => String::new(),
        };
        let warnings = if row.warnings.is_empty() {
            String::new()
        } else {
            format!(" warnings={}", row.warnings.join("|"))
        };
        let _ = writeln!(
            out,
            "  - {} [{}] overall={}{}{}",
            row_name,
            format_variable_kind(row.kind),
            row.overall.display,
            p_text,
            warnings
        );
        if !group_cells.is_empty() {
            let _ = writeln!(out, "    {group_cells}");
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
    let mut out = String::new();
    let _ = writeln!(out, "Diagnostic ROC");
    let _ = writeln!(out, "  Status           {}", result.status);
    let _ = writeln!(out, "  Data path        {}", result.data_path);
    if let Some(path) = &result.analysis_path {
        let _ = writeln!(out, "  Analysis         {path}");
    }
    let _ = writeln!(out, "  Truth            {}", result.truth);
    let _ = writeln!(out, "  Score            {}", result.score);
    let _ = writeln!(
        out,
        "  Rows             total={} used={} excluded_missing={} excluded_invalid={}",
        result.n_total, result.n_used, result.n_excluded_missing, result.n_excluded_invalid
    );
    let _ = writeln!(
        out,
        "  Classes          cases={} controls={}",
        result.n_cases, result.n_controls
    );
    let _ = writeln!(out, "  AUC              {:.4}", result.auc);
    let _ = writeln!(
        out,
        "  Youden threshold {:.4} J={:.4} sensitivity={:.4} specificity={:.4}",
        result.youden.threshold,
        result.youden.youden_j,
        result.youden.sensitivity,
        result.youden.specificity
    );
    if let Some(metrics) = &result.threshold_metrics {
        let _ = writeln!(out, "  Threshold metrics");
        let _ = writeln!(
            out,
            "  - threshold={:.4} TP={} FP={} TN={} FN={} sensitivity={:.4} specificity={:.4} PPV={:.4} NPV={:.4} accuracy={:.4}",
            metrics.threshold,
            metrics.tp,
            metrics.fp,
            metrics.tn,
            metrics.fn_count,
            metrics.sensitivity,
            metrics.specificity,
            metrics.ppv,
            metrics.npv,
            metrics.accuracy
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

pub fn render_power_text(result: &PowerResult) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "Power / Sample Size");
    let _ = writeln!(out, "  Status           {}", result.status);
    let _ = writeln!(out, "  Method           {}", result.method);
    let _ = writeln!(out, "  Alpha            {:.4}", result.alpha);
    if let Some(power) = result.power {
        let _ = writeln!(out, "  Power            {power:.4}");
    }
    if let Some(ratio) = result.allocation_ratio {
        let _ = writeln!(out, "  Allocation       n2/n1={ratio:.4}");
    }
    let _ = writeln!(out, "  Total N          {}", result.total_n);
    if let (Some(group1), Some(group2)) = (result.group1_n, result.group2_n) {
        let _ = writeln!(out, "  Groups           n1={group1} n2={group2}");
    }
    if let Some(effect_size) = result.effect_size {
        let _ = writeln!(out, "  Effect size      {effect_size:.4}");
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

pub fn render_survival_km_text(result: &SurvivalKmResult) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "Kaplan-Meier Survival");
    let _ = writeln!(out, "  Status           {}", result.status);
    let _ = writeln!(out, "  Data path        {}", result.data_path);
    if let Some(path) = &result.analysis_path {
        let _ = writeln!(out, "  Analysis         {path}");
    }
    let _ = writeln!(out, "  Time             {}", result.time);
    let _ = writeln!(out, "  Event            {}", result.event);
    let _ = writeln!(
        out,
        "  Group            {}",
        result.group.as_deref().unwrap_or("<overall>")
    );
    let _ = writeln!(
        out,
        "  Rows             total={} used={} excluded_missing={} excluded_invalid={}",
        result.n_total, result.n_used, result.n_excluded_missing, result.n_excluded_invalid
    );
    let _ = writeln!(out, "  Groups           {}", result.groups.join(", "));
    if let Some(log_rank) = &result.log_rank {
        let _ = writeln!(
            out,
            "  Log-rank         chi_square={:.4} df={} p={}",
            log_rank.chi_square,
            log_rank.degrees_freedom,
            format_p_value(log_rank.p_value)
        );
    }
    let _ = writeln!(out, "  Steps");
    for step in &result.steps {
        let _ = writeln!(
            out,
            "  - group={} time={:.4} risk={} events={} censored={} survival={:.4} se={:.4} ci95=[{:.4}, {:.4}]",
            step.group,
            step.time,
            step.n_risk,
            step.n_event,
            step.n_censored,
            step.survival,
            step.standard_error,
            step.ci_lower,
            step.ci_upper
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

pub fn render_rate_text(result: &RateResult) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "Rate");
    let _ = writeln!(out, "  Status           {}", result.status);
    let _ = writeln!(out, "  Data path        {}", result.data_path);
    if let Some(path) = &result.analysis_path {
        let _ = writeln!(out, "  Analysis         {path}");
    }
    let _ = writeln!(out, "  Event            {}", result.event);
    let _ = writeln!(out, "  Person-time      {}", result.person_time);
    let _ = writeln!(
        out,
        "  Strata           {}",
        if result.strata.is_empty() {
            "<overall>".to_string()
        } else {
            result.strata.join(", ")
        }
    );
    let _ = writeln!(out, "  Rows");
    for row in &result.rows {
        let _ = writeln!(
            out,
            "  - {} records={}/{} events={:.3} pt={:.3} rate={:.6} per_1000={:.3} ci95=[{:.3}, {:.3}]",
            row.stratum,
            row.included_records,
            row.total_records,
            row.events,
            row.person_time,
            row.rate,
            row.rate_per_1000,
            row.lower_ci_per_1000,
            row.upper_ci_per_1000
        );
    }
    if !result.notes.is_empty() {
        let _ = writeln!(out, "  Notes");
        for note in &result.notes {
            let _ = writeln!(out, "  - {note}");
        }
    }
    out
}

pub fn render_logistic_text(result: &LogisticResult) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "Logistic Model");
    let _ = writeln!(out, "  Status           {}", result.status);
    let _ = writeln!(out, "  Data path        {}", result.data_path);
    if let Some(path) = &result.analysis_path {
        let _ = writeln!(out, "  Analysis         {path}");
    }
    let _ = writeln!(out, "  Formula          {}", result.formula);
    let _ = writeln!(out, "  Outcome          {}", result.outcome);
    let _ = writeln!(
        out,
        "  Predictors       {}",
        if result.predictors.is_empty() {
            "<none>".to_string()
        } else {
            result.predictors.join(", ")
        }
    );
    let _ = writeln!(
        out,
        "  Rows             total={} used={} excluded_missing={} excluded_invalid={}",
        result.n_total, result.n_used, result.n_excluded_missing, result.n_excluded_invalid
    );
    let _ = writeln!(
        out,
        "  Outcome counts   events={} nonevents={}",
        result.n_events, result.n_nonevents
    );
    let _ = writeln!(
        out,
        "  Fit              converged={} iterations={} logLik={:.4}",
        result.converged, result.iterations, result.log_likelihood
    );
    if let Some(null_ll) = result.null_log_likelihood {
        let _ = writeln!(out, "  Null logLik      {null_ll:.4}");
    }
    let _ = writeln!(out, "  Diagnostics");
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
    let mut out = String::new();
    let _ = writeln!(out, "Cox Model");
    let _ = writeln!(out, "  Status           {}", result.status);
    let _ = writeln!(out, "  Data path        {}", result.data_path);
    if let Some(path) = &result.analysis_path {
        let _ = writeln!(out, "  Analysis         {path}");
    }
    let _ = writeln!(out, "  Formula          {}", result.formula);
    let _ = writeln!(out, "  Time             {}", result.time);
    let _ = writeln!(out, "  Event            {}", result.event);
    let _ = writeln!(
        out,
        "  Predictors       {}",
        if result.predictors.is_empty() {
            "<none>".to_string()
        } else {
            result.predictors.join(", ")
        }
    );
    let _ = writeln!(
        out,
        "  Rows             total={} used={} excluded_missing={} excluded_invalid={}",
        result.n_total, result.n_used, result.n_excluded_missing, result.n_excluded_invalid
    );
    let _ = writeln!(
        out,
        "  Event counts     events={} censored={} tied_event_times={}",
        result.n_events, result.n_censored, result.tied_event_times
    );
    let _ = writeln!(
        out,
        "  Fit              converged={} iterations={} logPartialLik={:.4}",
        result.converged, result.iterations, result.log_partial_likelihood
    );
    if let Some(c) = result.concordance {
        let _ = writeln!(out, "  Concordance      {c:.4}");
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
    let mut out = String::new();
    let _ = writeln!(out, "Linear Model");
    let _ = writeln!(out, "  Status           {}", result.status);
    let _ = writeln!(out, "  Data path        {}", result.data_path);
    if let Some(path) = &result.analysis_path {
        let _ = writeln!(out, "  Analysis         {path}");
    }
    let _ = writeln!(out, "  Formula          {}", result.formula);
    let _ = writeln!(out, "  Outcome          {}", result.outcome);
    let _ = writeln!(
        out,
        "  Predictors       {}",
        if result.predictors.is_empty() {
            "<none>".to_string()
        } else {
            result.predictors.join(", ")
        }
    );
    let _ = writeln!(
        out,
        "  Rows             total={} used={} excluded_missing={} excluded_invalid={}",
        result.n_total, result.n_used, result.n_excluded_missing, result.n_excluded_invalid
    );
    let _ = writeln!(
        out,
        "  Fit              converged={} R²={:.4} adj_R²={:.4} RSE={:.4}",
        result.converged, result.r_squared, result.adjusted_r_squared, result.residual_std_error
    );
    if let Some(f) = result.f_statistic {
        let p_text = result
            .f_p_value
            .map(|p| format!(" p={}", format_p_value(p)))
            .unwrap_or_default();
        let _ = writeln!(out, "  F-statistic      {f:.4}{p_text}");
    }
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

pub fn render_report_build_text(result: &ReportBuildResult) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "Report Build");
    let _ = writeln!(out, "  Status           {}", result.status);
    let _ = writeln!(out, "  Analysis         {}", result.analysis_path);
    let _ = writeln!(out, "  Output dir       {}", result.output_dir);
    let _ = writeln!(out, "  Files");
    for file in &result.written_files {
        let _ = writeln!(out, "  - {file}");
    }
    if !result.notes.is_empty() {
        let _ = writeln!(out, "  Notes");
        for note in &result.notes {
            let _ = writeln!(out, "  - {note}");
        }
    }
    out
}

pub fn render_report_verify_text(result: &ReportVerifyResult) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "Report Verify");
    let _ = writeln!(out, "  Status           {}", result.status);
    let _ = writeln!(out, "  Artifacts        {}", result.artifacts_dir);
    let _ = writeln!(
        out,
        "  Summary          accepted={} rejected={} errors={} warnings={}",
        result.accepted_count, result.rejected_count, result.error_count, result.warning_count
    );
    let _ = writeln!(out, "  Checks");
    for item in &result.items {
        let _ = writeln!(
            out,
            "  - {} {}: {}",
            analysis_check_level_label(item.level),
            item.code,
            item.message
        );
    }
    if !result.notes.is_empty() {
        let _ = writeln!(out, "  Notes");
        for note in &result.notes {
            let _ = writeln!(out, "  - {note}");
        }
    }
    out
}

pub fn render_analysis_check_text(result: &AnalysisCheckResult) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "Analysis Check");
    let _ = writeln!(out, "  Status           {}", result.status);
    let _ = writeln!(out, "  Analysis         {}", result.analysis_path);
    let _ = writeln!(out, "  Data path        {}", result.data_path);
    let _ = writeln!(
        out,
        "  Summary          errors={} warnings={}",
        result.error_count, result.warning_count
    );
    let _ = writeln!(out, "  Checks");
    for item in &result.items {
        let _ = writeln!(
            out,
            "  - {} {}: {}",
            analysis_check_level_label(item.level),
            item.code,
            item.message
        );
    }
    if !result.notes.is_empty() {
        let _ = writeln!(out, "  Notes");
        for note in &result.notes {
            let _ = writeln!(out, "  - {note}");
        }
    }
    out
}

fn analysis_check_level_label(level: AnalysisCheckLevel) -> &'static str {
    match level {
        AnalysisCheckLevel::Ok => "OK",
        AnalysisCheckLevel::Warning => "WARNING",
        AnalysisCheckLevel::Error => "ERROR",
    }
}

pub fn render_workflow_run_text(result: &WorkflowRunResult) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "Workflow Run");
    let _ = writeln!(out, "  Status           {}", result.status);
    let _ = writeln!(out, "  Run ID           {}", result.run_id);
    let _ = writeln!(out, "  Analysis         {}", result.analysis_path);
    let _ = writeln!(out, "  Data path        {}", result.data_path);
    let _ = writeln!(out, "  Artifacts        {}", result.artifacts_dir);
    let _ = writeln!(out, "  Report           {}", result.report_output_dir);
    let _ = writeln!(out, "  Steps");
    for step in &result.steps {
        let _ = writeln!(
            out,
            "  - #{} {} status={} artifact={}",
            step.step_index, step.command, step.status, step.artifact_dir
        );
    }
    if !result.notes.is_empty() {
        let _ = writeln!(out, "  Notes");
        for note in &result.notes {
            let _ = writeln!(out, "  - {note}");
        }
    }
    out
}

pub fn render_init_project_text(result: &InitProjectResult) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "Init Project");
    let _ = writeln!(out, "  Status           {}", result.status);
    let _ = writeln!(out, "  Project          {}", result.project_dir);
    let _ = writeln!(out, "  Analysis         {}", result.analysis_path);
    let _ = writeln!(out, "  Data dir         {}", result.data_dir);
    let _ = writeln!(out, "  Written files");
    for file in &result.written_files {
        let _ = writeln!(out, "  - {file}");
    }
    if !result.next_steps.is_empty() {
        let _ = writeln!(out, "  Next steps");
        for step in &result.next_steps {
            let _ = writeln!(out, "  - {step}");
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

pub fn render_doctor_text(result: &DoctorResult) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "Doctor");
    let _ = writeln!(out, "  Status           {}", result.status);
    let _ = writeln!(out, "  Version          {}", result.version);
    let _ = writeln!(out, "  Current dir      {}", result.current_dir);
    let _ = writeln!(out, "  Executable       {}", result.executable);
    let _ = writeln!(
        out,
        "  Summary          errors={} warnings={}",
        result.error_count, result.warning_count
    );
    let _ = writeln!(out, "  Checks");
    for item in &result.items {
        let _ = writeln!(
            out,
            "  - {} {}: {}",
            analysis_check_level_label(item.level),
            item.code,
            item.message
        );
    }
    if !result.notes.is_empty() {
        let _ = writeln!(out, "  Notes");
        for note in &result.notes {
            let _ = writeln!(out, "  - {note}");
        }
    }
    out
}

pub fn render_audit_explain_text(result: &AuditExplainResult) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "Audit Explain");
    let _ = writeln!(out, "  Status           {}", result.status);
    let _ = writeln!(out, "  Artifacts        {}", result.artifacts_dir);
    let _ = writeln!(out, "  Evidence index   {}", result.evidence_index_path);
    let _ = writeln!(
        out,
        "  Summary          accepted={} rejected={} policy_exceptions={}",
        result.accepted_count, result.rejected_count, result.policy_exception_count
    );
    write_audit_artifact_group(&mut out, "Accepted artifacts", &result.accepted_artifacts);
    write_audit_artifact_group(&mut out, "Rejected artifacts", &result.rejected_artifacts);
    if !result.policy_exceptions.is_empty() {
        let _ = writeln!(out, "  Policy exceptions");
        for exception in &result.policy_exceptions {
            let _ = writeln!(out, "  - {exception}");
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

pub fn render_open_report_text(result: &OpenReportResult) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "Open Report");
    let _ = writeln!(out, "  Status           {}", result.status);
    let _ = writeln!(out, "  Artifacts        {}", result.artifacts_dir);
    let _ = writeln!(out, "  Report           {}", result.report_path);
    let _ = writeln!(out, "  Opened           {}", result.opened);
    if !result.notes.is_empty() {
        let _ = writeln!(out, "  Notes");
        for note in &result.notes {
            let _ = writeln!(out, "  - {note}");
        }
    }
    out
}

fn write_audit_artifact_group(out: &mut String, title: &str, artifacts: &[AuditExplainArtifact]) {
    let _ = writeln!(out, "  {title}");
    if artifacts.is_empty() {
        let _ = writeln!(out, "  - <none>");
        return;
    }
    for artifact in artifacts {
        let step = artifact
            .analysis_step_index
            .map(|index| format!(" step=#{index}"))
            .unwrap_or_default();
        let decision = artifact
            .report_decision
            .as_ref()
            .map(|value| format!(" decision={value}"))
            .unwrap_or_default();
        let _ = writeln!(
            out,
            "  - {} status={}{}{} reason={}",
            artifact.command, artifact.status, decision, step, artifact.reason
        );
    }
}

pub fn render_auth_set_text(result: &AuthSetResult) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "Auth Set");
    let _ = writeln!(out, "  Status           {}", result.status);
    let _ = writeln!(out, "  Provider         {}", result.provider);
    let _ = writeln!(out, "  Config path      {}", result.config_path);
    let _ = writeln!(out, "  API key env      {}", result.api_key_env);
    if let Some(base_url_env) = &result.base_url_env {
        let _ = writeln!(out, "  Base URL env     {base_url_env}");
    }
    if !result.notes.is_empty() {
        let _ = writeln!(out, "  Notes");
        for note in &result.notes {
            let _ = writeln!(out, "  - {note}");
        }
    }
    out
}

pub fn render_auth_doctor_text(result: &AuthDoctorResult) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "Auth Doctor");
    let _ = writeln!(out, "  Status           {}", result.status);
    let _ = writeln!(out, "  Config path      {}", result.config_path);
    let _ = writeln!(out, "  Providers");
    for provider in &result.providers {
        let _ = writeln!(
            out,
            "  - {} model={} source={} api_key_present={} base_url_present={}",
            provider.provider,
            provider.model_hint,
            provider.credential_source,
            provider.api_key_present,
            provider.base_url_present
        );
        let _ = writeln!(
            out,
            "    env={}{}",
            provider.api_key_env,
            provider
                .base_url_env
                .as_ref()
                .map(|value| format!(" base_url_env={value}"))
                .unwrap_or_default()
        );
        if let Some(base_url) = &provider.configured_base_url {
            let _ = writeln!(out, "    configured_base_url={base_url}");
        }
        for note in &provider.notes {
            let _ = writeln!(out, "    note={note}");
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

pub fn render_ai_ask_text(result: &AiAskResult) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "AI Ask");
    let _ = writeln!(out, "  Status           {}", result.status);
    let _ = writeln!(out, "  Provider         {}", result.provider);
    let _ = writeln!(out, "  Credential       {}", result.credential_source);
    let _ = writeln!(out, "  Model            {}", result.model);
    let _ = writeln!(
        out,
        "  Tokens           in={} out={} total={}",
        result.input_tokens, result.output_tokens, result.total_tokens
    );
    if let Some(request_id) = &result.request_id {
        let _ = writeln!(out, "  Request ID       {request_id}");
    }
    let _ = writeln!(out, "  Prompt           {}", result.prompt);
    let _ = writeln!(out, "  Response");
    for line in result.response_text.lines() {
        let _ = writeln!(out, "  {line}");
    }
    if !result.notes.is_empty() {
        let _ = writeln!(out, "  Notes");
        for note in &result.notes {
            let _ = writeln!(out, "  - {note}");
        }
    }
    out
}

pub fn render_config_text(result: &ConfigResult) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "Config");
    let _ = writeln!(out, "  Status           {}", result.status);
    let _ = writeln!(out, "  Action           {}", result.action);
    let _ = writeln!(out, "  Config path      {}", result.config_path);
    let _ = writeln!(
        out,
        "  Default model    {}",
        result.default_model.as_deref().unwrap_or("<none>")
    );
    let _ = writeln!(
        out,
        "  Saved models     {}",
        if result.saved_models.is_empty() {
            "<none>".to_string()
        } else {
            result.saved_models.join(", ")
        }
    );
    let _ = writeln!(out, "  Message          {}", result.message);
    if !result.notes.is_empty() {
        let _ = writeln!(out, "  Notes");
        for note in &result.notes {
            let _ = writeln!(out, "  - {note}");
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::format_p_value;

    #[test]
    fn format_p_value_uses_threshold_for_tiny_values() {
        assert_eq!(format_p_value(0.0), "<0.001");
        assert_eq!(format_p_value(0.0009), "<0.001");
        assert_eq!(format_p_value(0.001), "0.0010");
        assert_eq!(format_p_value(0.8355440287990006), "0.8355");
    }
}
