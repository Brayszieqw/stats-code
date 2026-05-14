use super::contract::AnalysisSpec;
use super::types::{AnalysisKind, ModelKind, VariableRole};

pub fn validate_study_context(spec: &AnalysisSpec) -> Vec<String> {
    let mut issues = Vec::new();
    let has_declared_analyses = !spec.analyses.is_empty();
    let needs_estimand = spec
        .analyses
        .iter()
        .any(|step| !matches!(step.kind, AnalysisKind::Inspect));
    let needs_outcome = spec.variables.iter().any(|variable| {
        variable.roles.contains(&VariableRole::Outcome)
            || variable.roles.contains(&VariableRole::Event)
    }) || spec
        .analyses
        .iter()
        .any(|step| step.outcome.is_some() || step.event.is_some());
    let needs_exposure = spec
        .variables
        .iter()
        .any(|variable| variable.roles.contains(&VariableRole::Exposure))
        || spec.study.design.to_ascii_lowercase().contains("trial");
    let needs_comparator = needs_exposure;
    let needs_time_anchor = spec.analyses.iter().any(|step| {
        matches!(step.model, Some(ModelKind::Cox))
            || matches!(step.kind, AnalysisKind::Rate)
            || step.time.is_some()
            || step.event.is_some()
            || step.person_time.is_some()
    });
    let needs_clustering = spec
        .survey
        .as_ref()
        .and_then(|survey| survey.cluster.as_ref())
        .is_some()
        || spec
            .variables
            .iter()
            .any(|variable| variable.roles.contains(&VariableRole::Cluster));

    if needs_estimand && is_blank_option(spec.study_context.estimand.as_deref()) {
        issues.push(
            "study_context.estimand is required for declared analyses beyond inspect".to_string(),
        );
    }
    if needs_outcome && is_blank_option(spec.study_context.outcome.as_deref()) {
        issues.push(
            "study_context.outcome is required because outcomes/events are declared".to_string(),
        );
    }
    if needs_exposure && is_blank_option(spec.study_context.exposure.as_deref()) {
        issues.push(
            "study_context.exposure is required because an exposure or intervention is declared"
                .to_string(),
        );
    }
    if needs_comparator && is_blank_option(spec.study_context.comparator.as_deref()) {
        issues.push(
            "study_context.comparator is required because a comparison strategy is declared"
                .to_string(),
        );
    }
    if needs_time_anchor && is_blank_option(spec.study_context.time_zero.as_deref()) {
        issues.push(
            "study_context.time_zero is required for rate or time-to-event analyses".to_string(),
        );
    }
    if needs_time_anchor && is_blank_option(spec.study_context.follow_up.as_deref()) {
        issues.push(
            "study_context.follow_up is required for rate or time-to-event analyses".to_string(),
        );
    }
    if needs_time_anchor && is_blank_option(spec.study_context.censoring.as_deref()) {
        issues.push(
            "study_context.censoring is required for rate or time-to-event analyses".to_string(),
        );
    }
    if has_declared_analyses && is_blank_option(spec.study_context.missing_data_strategy.as_deref())
    {
        issues
            .push("study_context.missing_data_strategy is required for analysis runs".to_string());
    }
    if needs_clustering && is_blank_option(spec.study_context.clustering.as_deref()) {
        issues.push("study_context.clustering is required because clustered or survey structure is declared".to_string());
    }
    if has_declared_analyses && is_blank_option(spec.study_context.reporting_guideline.as_deref()) {
        issues.push(format!(
            "study_context.reporting_guideline is required (recommended: {})",
            recommended_reporting_guideline(&spec.study.design)
        ));
    }

    issues
}

pub fn recommended_reporting_guideline(design: &str) -> &'static str {
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

fn is_blank_option(value: Option<&str>) -> bool {
    value.map(str::trim).is_none_or(str::is_empty)
}
