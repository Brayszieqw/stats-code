use super::{validate_study_context, AnalysisSpec, Path, VariableRole};
pub(crate) fn ensure_study_context_ready(
    analysis_path: &Path,
    spec: &AnalysisSpec,
) -> Result<(), String> {
    let issues = validate_study_context(spec);
    if issues.is_empty() {
        return Ok(());
    }

    Err(format!(
        "Analysis spec `{}` is not ready for analysis-driven commands because required `study_context` fields are missing:\n- {}\n\nSuggested template for `{}`:\n{}",
        analysis_path.display(),
        issues.join("\n- "),
        analysis_path.display(),
        build_study_context_template(spec),
    ))
}

fn build_study_context_template(spec: &AnalysisSpec) -> String {
    let outcome = first_variable_with_role(spec, VariableRole::Outcome)
        .or_else(|| first_variable_with_role(spec, VariableRole::Event))
        .or_else(|| {
            spec.analyses
                .iter()
                .find_map(|step| step.outcome.clone().or_else(|| step.event.clone()))
        })
        .unwrap_or_else(|| "<fill in outcome>".to_string());
    let exposure = first_variable_with_role(spec, VariableRole::Exposure)
        .unwrap_or_else(|| "<fill in exposure>".to_string());
    let clustering = spec
        .survey
        .as_ref()
        .and_then(|survey| survey.cluster.clone())
        .or_else(|| first_variable_with_role(spec, VariableRole::Cluster))
        .unwrap_or_else(|| "<if clustered, fill in cluster variable>".to_string());
    let guideline = crate::schema::recommended_reporting_guideline(&spec.study.design);

    let mut lines = vec!["study_context:".to_string()];
    lines.push(format!(
        "  estimand: {}",
        quote_yaml_placeholder("<fill in target effect measure>")
    ));
    lines.push(format!("  exposure: {}", quote_yaml_placeholder(&exposure)));
    lines.push(format!(
        "  comparator: {}",
        quote_yaml_placeholder("<fill in comparator>")
    ));
    lines.push(format!("  outcome: {}", quote_yaml_placeholder(&outcome)));
    if requires_time_anchor_template(spec) {
        lines.push(format!(
            "  time_zero: {}",
            quote_yaml_placeholder("<fill in index date>")
        ));
        lines.push(format!(
            "  follow_up: {}",
            quote_yaml_placeholder("<fill in follow-up window>")
        ));
        lines.push(format!(
            "  censoring: {}",
            quote_yaml_placeholder("<fill in censoring rule>")
        ));
    }
    lines.push(format!(
        "  missing_data_strategy: {}",
        quote_yaml_placeholder("<fill in missing-data handling>")
    ));
    if requires_clustering_template(spec) {
        lines.push(format!(
            "  clustering: {}",
            quote_yaml_placeholder(&clustering)
        ));
    }
    lines.push(format!(
        "  sensitivity_analyses: {}",
        quote_yaml_placeholder("<optional robustness analyses>")
    ));
    lines.push(format!(
        "  reporting_guideline: {}",
        quote_yaml_placeholder(guideline)
    ));
    lines.join("\n")
}

fn first_variable_with_role(spec: &AnalysisSpec, role: VariableRole) -> Option<String> {
    spec.variables
        .iter()
        .find(|variable| variable.roles.contains(&role))
        .map(|variable| variable.name.clone())
}

fn requires_time_anchor_template(spec: &AnalysisSpec) -> bool {
    spec.analyses.iter().any(|step| {
        step.time.is_some()
            || step.person_time.is_some()
            || matches!(step.kind, crate::schema::AnalysisKind::Rate)
            || matches!(step.model, Some(crate::schema::ModelKind::Cox))
    })
}

fn requires_clustering_template(spec: &AnalysisSpec) -> bool {
    spec.survey
        .as_ref()
        .and_then(|survey| survey.cluster.as_ref())
        .is_some()
        || spec
            .variables
            .iter()
            .any(|variable| variable.roles.contains(&VariableRole::Cluster))
}

fn quote_yaml_placeholder(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\\\""))
}

// ---------------------------------------------------------------------------
// Data path resolution
// ---------------------------------------------------------------------------
