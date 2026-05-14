use std::fmt::Write as _;

use crate::schema::{
    format_variable_kind, ColumnInspection, InspectResult, PowerResult, RateResult,
    SurvivalKmResult, TableOneResult,
};

use super::format_p_value;
use super::writer::TextReportWriter;

pub fn render_inspect_text(result: &InspectResult) -> String {
    let mut w = TextReportWriter::new();
    w.title("Inspect");
    w.field("Status", &result.status);
    w.field("Data path", &result.data_path);
    w.field("Format", format!("{:?}", result.format));
    w.field_opt("Rows", result.rows);
    w.field("Columns", result.columns);

    // Variables section uses custom formatting
    let mut out = w.finish();
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

pub fn render_tableone_text(result: &TableOneResult) -> String {
    let mut w = TextReportWriter::new();
    w.title("Table 1");
    w.field("Status", &result.status);
    w.field("Data path", &result.data_path);
    w.field_opt("Analysis", result.analysis_path.as_deref());
    w.field("By", &result.by);
    w.field(
        "Groups",
        if result.group_levels.is_empty() {
            "<none>".to_string()
        } else {
            result.group_levels.join(", ")
        },
    );

    // Rows section uses custom formatting
    let mut out = w.finish();
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

pub fn render_rate_text(result: &RateResult) -> String {
    let mut w = TextReportWriter::new();
    w.title("Rate");
    w.field("Status", &result.status);
    w.field("Data path", &result.data_path);
    w.field_opt("Analysis", result.analysis_path.as_deref());
    w.field("Event", &result.event);
    w.field("Person-time", &result.person_time);
    w.field(
        "Strata",
        if result.strata.is_empty() {
            "<overall>".to_string()
        } else {
            result.strata.join(", ")
        },
    );

    // Rows section uses custom formatting
    let mut out = w.finish();
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

pub fn render_survival_km_text(result: &SurvivalKmResult) -> String {
    let mut w = TextReportWriter::new();
    w.title("Kaplan-Meier Survival");
    w.field("Status", &result.status);
    w.field("Data path", &result.data_path);
    w.field_opt("Analysis", result.analysis_path.as_deref());
    w.field("Time", &result.time);
    w.field("Event", &result.event);
    w.field("Group", result.group.as_deref().unwrap_or("<overall>"));
    w.field(
        "Rows",
        format!(
            "total={} used={} excluded_missing={} excluded_invalid={}",
            result.n_total, result.n_used, result.n_excluded_missing, result.n_excluded_invalid
        ),
    );
    w.field("Groups", result.groups.join(", "));

    // Log-rank and steps use custom formatting
    let mut out = w.finish();
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

pub fn render_power_text(result: &PowerResult) -> String {
    let mut w = TextReportWriter::new();
    w.title("Power / Sample Size");
    w.field("Status", &result.status);
    w.field("Method", &result.method);
    w.field("Alpha", format!("{:.4}", result.alpha));
    w.field_opt("Power", result.power.map(|p| format!("{p:.4}")));
    w.field_opt(
        "Allocation",
        result.allocation_ratio.map(|r| format!("n2/n1={r:.4}")),
    );
    w.field("Total N", result.total_n);

    let mut out = w.finish();
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
