use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::cli::{CheckArgs, PlanArgs};
use crate::helpers::{read_excel_records, require_column, stringify_error};
use crate::render::render_analysis_check_text;
use crate::report::resolve_relative_to_analysis;
use crate::schema::{
    is_missing_value_for_column, load_analysis_spec, AnalysisCheckItem, AnalysisCheckLevel,
    AnalysisCheckResult, AnalysisKind, AnalysisSpec, DataFormat, ModelKind, PlannedCommandResult,
    VariableKind,
};

use super::common::push_check;
pub(crate) fn handle_analysis_plan(args: &PlanArgs) -> Result<PlannedCommandResult, String> {
    let analysis_path = args.analysis.canonicalize().map_err(|error| {
        format!(
            "Cannot read analysis spec `{}`: {error}",
            args.analysis.display()
        )
    })?;
    let spec = load_analysis_spec(&analysis_path)?;
    let check = validate_analysis_contract(&analysis_path, &spec);
    if check.has_errors() {
        return Err(render_analysis_check_text(&check));
    }

    let data_path = resolve_relative_to_analysis(&analysis_path, &spec.data.path);
    let out_dir = args
        .out
        .as_ref()
        .map(|path| resolve_relative_to_analysis(&analysis_path, path))
        .or_else(|| {
            spec.report
                .as_ref()
                .map(|report| resolve_relative_to_analysis(&analysis_path, &report.out_dir))
        })
        .unwrap_or_else(|| {
            analysis_path.parent().map_or_else(
                || PathBuf::from("stats-code-artifacts"),
                |parent| parent.join("stats-code-artifacts"),
            )
        });

    let mut expected_outputs = Vec::new();
    for (index, step) in spec.analyses.iter().enumerate() {
        expected_outputs.push(describe_plan_step(index, step));
    }
    expected_outputs.push(format!(
        "report build -> `{}`",
        out_dir.join("report").join("report.md").display()
    ));
    expected_outputs.push(format!(
        "audit evidence-index -> `{}`",
        out_dir.join("audit").join("evidence-index.json").display()
    ));

    let workflow_command = format!(
        "stats-code workflow run {} --out {} --no-chat{}{}{}{}{}{}",
        analysis_path.display(),
        out_dir.display(),
        if args.strict { " --strict" } else { "" },
        if args.allow_warnings {
            " --allow-warnings"
        } else {
            ""
        },
        if args.allow_unenforced_survey {
            " --allow-unenforced-survey"
        } else {
            ""
        },
        if args.allow_unenforced_privacy {
            " --allow-unenforced-privacy"
        } else {
            ""
        },
        if args.include_exploratory {
            " --include-exploratory"
        } else {
            ""
        },
        args.explore_out.as_ref().map_or_else(String::new, |path| {
            format!(
                " --explore-out {}",
                resolve_relative_to_analysis(&analysis_path, path).display()
            )
        })
    );

    let mut notes = vec![
        "Plan validates the analysis contract and previews the deterministic workflow without running statistics.".to_string(),
        format!("Formal artifact output directory: `{}`.", out_dir.display()),
    ];
    if let Some(survey) = &spec.survey {
        if survey.weight.is_some() {
            notes.push("Survey weight metadata is declared and will be applied by supported deterministic engines.".to_string());
        }
        if survey_requires_policy_exception(survey) {
            notes.push("Complex survey variance metadata is declared; strata, clusters, replicate weights, and linearized variance still require explicit review.".to_string());
        }
    }
    if let Some(privacy) = &spec.privacy {
        if privacy.small_cell_threshold.is_some() {
            notes.push("Small-cell suppression metadata is declared and will be applied to report markdown tables.".to_string());
        }
        if privacy_requires_policy_exception(privacy) {
            notes.push("Privacy metadata requiring de-identification or identifier handling is declared; explicit policy review is still required.".to_string());
        }
    }
    if args.strict {
        notes.push("Strict policy preview is enabled.".to_string());
    }
    if args.include_exploratory {
        notes.push("Exploratory artifacts would be eligible for report build only because --include-exploratory was set.".to_string());
    }
    if let Some(path) = &args.explore_out {
        notes.push(format!(
            "Exploratory artifact directory preview: `{}`.",
            resolve_relative_to_analysis(&analysis_path, path).display()
        ));
    }

    Ok(PlannedCommandResult {
        status: "ok".to_string(),
        command: workflow_command,
        data_path: data_path.display().to_string(),
        analysis_path: Some(analysis_path.display().to_string()),
        formula: None,
        expected_outputs,
        notes,
    })
}

fn describe_plan_step(index: usize, step: &crate::schema::AnalysisStepSpec) -> String {
    let id = step
        .id
        .as_deref()
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .unwrap_or("<missing-id>");
    match step.kind {
        AnalysisKind::Inspect => format!("#{index} {id}: inspect"),
        AnalysisKind::TableOne => format!(
            "#{index} {id}: tableone by `{}`",
            step.by.as_deref().unwrap_or("<missing-by>")
        ),
        AnalysisKind::Rate => format!(
            "#{index} {id}: rate event=`{}` person_time=`{}` strata={}",
            step.event.as_deref().unwrap_or("<missing-event>"),
            step.person_time
                .as_deref()
                .unwrap_or("<missing-person-time>"),
            plan_list_or_none(&step.strata)
        ),
        AnalysisKind::Model => match step.model {
            Some(ModelKind::Logistic) => format!(
                "#{index} {id}: logistic outcome=`{}` predictors={}",
                step.outcome.as_deref().unwrap_or("<missing-outcome>"),
                plan_list_or_none(&step.predictors)
            ),
            Some(ModelKind::Cox) => format!(
                "#{index} {id}: cox time=`{}` event=`{}` predictors={}",
                step.time.as_deref().unwrap_or("<missing-time>"),
                step.event.as_deref().unwrap_or("<missing-event>"),
                plan_list_or_none(&step.predictors)
            ),
            Some(ModelKind::Linear) => format!(
                "#{index} {id}: linear outcome=`{}` predictors={}",
                step.outcome.as_deref().unwrap_or("<missing-outcome>"),
                plan_list_or_none(&step.predictors)
            ),
            None => format!("#{index} {id}: model <missing-model>"),
        },
    }
}

fn plan_list_or_none(values: &[String]) -> String {
    if values.is_empty() {
        "<none>".to_string()
    } else {
        values.join(",")
    }
}

pub(crate) fn handle_analysis_check(args: &CheckArgs) -> Result<AnalysisCheckResult, String> {
    let analysis_path = args.analysis.canonicalize().map_err(|error| {
        format!(
            "Cannot read analysis spec `{}`: {error}",
            args.analysis.display()
        )
    })?;
    let spec = load_analysis_spec(&analysis_path)?;
    Ok(validate_analysis_contract(&analysis_path, &spec))
}

pub(super) fn validate_analysis_contract(
    analysis_path: &Path,
    spec: &AnalysisSpec,
) -> AnalysisCheckResult {
    let mut items = Vec::new();
    push_check(
        &mut items,
        AnalysisCheckLevel::Ok,
        "analysis_yaml_loaded",
        format!(
            "analysis spec `{}` parsed successfully",
            analysis_path.display()
        ),
    );

    if spec
        .schema_version
        .as_deref()
        .is_none_or(|value| value.trim().is_empty())
    {
        push_check(
            &mut items,
            AnalysisCheckLevel::Error,
            "schema_version_missing",
            "`schema_version` is required for audit/replay compatibility",
        );
    } else {
        push_check(
            &mut items,
            AnalysisCheckLevel::Ok,
            "schema_version_present",
            format!(
                "schema_version={}",
                spec.schema_version.as_deref().unwrap_or_default()
            ),
        );
    }

    for issue in crate::schema::validate_study_context(spec) {
        push_check(
            &mut items,
            AnalysisCheckLevel::Error,
            "study_context_missing",
            issue,
        );
    }

    let data_path = resolve_relative_to_analysis(analysis_path, &spec.data.path);
    let mut snapshot = None;
    if data_path.is_file() {
        push_check(
            &mut items,
            AnalysisCheckLevel::Ok,
            "data_file_found",
            format!("data file found at `{}`", data_path.display()),
        );
        match read_data_snapshot(&data_path, spec.data.format) {
            Ok(data) => {
                push_check(
                    &mut items,
                    AnalysisCheckLevel::Ok,
                    "data_readable",
                    format!(
                        "data header has {} column(s); {} row(s) scanned",
                        data.headers.len(),
                        data.records.len()
                    ),
                );
                snapshot = Some(data);
            }
            Err(error) => push_check(
                &mut items,
                AnalysisCheckLevel::Error,
                "data_unreadable",
                error,
            ),
        }
    } else {
        push_check(
            &mut items,
            AnalysisCheckLevel::Error,
            "data_file_missing",
            format!("data file `{}` was not found", data_path.display()),
        );
    }

    let mut declared_variables = BTreeMap::new();
    for variable in &spec.variables {
        if declared_variables
            .insert(variable.name.clone(), variable.kind)
            .is_some()
        {
            push_check(
                &mut items,
                AnalysisCheckLevel::Error,
                "variable_duplicate",
                format!("variable `{}` is declared more than once", variable.name),
            );
        }
    }

    if let Some(data) = &snapshot {
        let header_index = build_header_index(&data.headers, &mut items);
        validate_declared_variables(&mut items, spec, &header_index);
        validate_policy_metadata(&mut items, spec, &header_index);
        validate_analysis_steps(&mut items, spec, data, &header_index, &declared_variables);
    }

    let error_count = items
        .iter()
        .filter(|item| item.level == AnalysisCheckLevel::Error)
        .count();
    let warning_count = items
        .iter()
        .filter(|item| item.level == AnalysisCheckLevel::Warning)
        .count();

    AnalysisCheckResult {
        status: if error_count == 0 { "ok" } else { "error" }.to_string(),
        analysis_path: analysis_path.display().to_string(),
        data_path: data_path.display().to_string(),
        error_count,
        warning_count,
        items,
        notes: vec![
            "Check validates the declared analysis contract without running statistics.".to_string(),
            "Survey and privacy metadata are reviewed here, but enforcement is not implemented in the deterministic engines yet.".to_string(),
        ],
    }
}

#[derive(Debug)]
struct DataSnapshot {
    headers: Vec<String>,
    records: Vec<Vec<String>>,
}

fn read_data_snapshot(path: &Path, format: DataFormat) -> Result<DataSnapshot, String> {
    match format {
        DataFormat::Csv => {
            let mut reader = csv::Reader::from_path(path).map_err(stringify_error)?;
            let headers = reader
                .headers()
                .map_err(stringify_error)?
                .iter()
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>();
            let records = reader
                .records()
                .map(|record| {
                    record
                        .map_err(stringify_error)
                        .map(|record| record.iter().map(ToOwned::to_owned).collect::<Vec<_>>())
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok(DataSnapshot { headers, records })
        }
        DataFormat::Excel => {
            let (headers, records) = read_excel_records(path)?;
            Ok(DataSnapshot { headers, records })
        }
        other => Err(format!(
            "check currently supports CSV and Excel data, not `{other:?}`"
        )),
    }
}

fn build_header_index(
    headers: &[String],
    items: &mut Vec<AnalysisCheckItem>,
) -> BTreeMap<String, usize> {
    let mut index = BTreeMap::new();
    for (position, header) in headers.iter().enumerate() {
        if index.insert(header.clone(), position).is_some() {
            push_check(
                items,
                AnalysisCheckLevel::Error,
                "data_header_duplicate",
                format!("data column `{header}` appears more than once"),
            );
        }
    }
    index
}

fn validate_declared_variables(
    items: &mut Vec<AnalysisCheckItem>,
    spec: &AnalysisSpec,
    header_index: &BTreeMap<String, usize>,
) {
    for variable in &spec.variables {
        if header_index.contains_key(&variable.name) {
            push_check(
                items,
                AnalysisCheckLevel::Ok,
                "variable_found",
                format!(
                    "declared variable `{}` exists in the data header",
                    variable.name
                ),
            );
        } else {
            push_check(
                items,
                AnalysisCheckLevel::Error,
                "variable_missing",
                format!(
                    "declared variable `{}` was not found in the data header",
                    variable.name
                ),
            );
        }
    }
}

fn validate_policy_metadata(
    items: &mut Vec<AnalysisCheckItem>,
    spec: &AnalysisSpec,
    header_index: &BTreeMap<String, usize>,
) {
    if let Some(survey) = &spec.survey {
        if survey.weight.is_some() {
            push_check(
                items,
                AnalysisCheckLevel::Ok,
                "survey_weight_supported",
                "survey weight metadata detected; supported deterministic engines apply observation weights to estimates",
            );
        }
        if survey_requires_policy_exception(survey) {
            push_check(
                items,
                AnalysisCheckLevel::Warning,
                "complex_survey_variance_unenforced",
                "complex survey variance metadata detected; strata, clusters, replicate weights, and linearized variance still require explicit review",
            );
        }
        for (field, value) in [
            ("survey.weight", survey.weight.as_ref()),
            ("survey.strata", survey.strata.as_ref()),
            ("survey.cluster", survey.cluster.as_ref()),
        ] {
            if let Some(name) = value {
                check_column_reference(items, header_index, field, name);
            }
        }
        for name in &survey.replicate_weights {
            check_column_reference(items, header_index, "survey.replicate_weights", name);
        }
    }

    if let Some(privacy) = &spec.privacy {
        if privacy.small_cell_threshold.is_some() {
            push_check(
                items,
                AnalysisCheckLevel::Ok,
                "small_cell_suppression_supported",
                "small-cell suppression metadata detected; report markdown tables suppress positive cells below the threshold",
            );
        }
        if privacy_requires_policy_exception(privacy) {
            push_check(
                items,
                AnalysisCheckLevel::Warning,
                "privacy_deidentification_unenforced",
                "privacy metadata requiring de-identification or identifier handling is not automatically enforced",
            );
        }
        for name in privacy
            .direct_identifiers
            .iter()
            .chain(privacy.quasi_identifiers.iter())
        {
            check_column_reference(items, header_index, "privacy identifier", name);
        }
    }
}

pub(super) fn survey_requires_policy_exception(survey: &crate::schema::SurveyDesignSpec) -> bool {
    survey.strata.is_some()
        || survey.cluster.is_some()
        || !survey.replicate_weights.is_empty()
        || survey.variance_estimator.is_some()
        || !survey.combined_cycles.is_empty()
}

pub(super) fn privacy_requires_policy_exception(privacy: &crate::schema::PrivacySpec) -> bool {
    privacy.deidentify
        || !privacy.direct_identifiers.is_empty()
        || !privacy.quasi_identifiers.is_empty()
}

fn validate_analysis_steps(
    items: &mut Vec<AnalysisCheckItem>,
    spec: &AnalysisSpec,
    data: &DataSnapshot,
    header_index: &BTreeMap<String, usize>,
    declared_variables: &BTreeMap<String, VariableKind>,
) {
    let mut ids = BTreeSet::new();
    if spec.analyses.is_empty() {
        push_check(
            items,
            AnalysisCheckLevel::Warning,
            "analyses_empty",
            "`analyses` is empty; workflow run will only be able to build scaffolded outputs",
        );
    }

    for (index, step) in spec.analyses.iter().enumerate() {
        let step_label = step_label(index, step.id.as_deref());
        match step
            .id
            .as_deref()
            .map(str::trim)
            .filter(|id| !id.is_empty())
        {
            Some(id) => {
                if ids.insert(id.to_string()) {
                    push_check(
                        items,
                        AnalysisCheckLevel::Ok,
                        "analysis_id_present",
                        format!("{step_label} has stable id `{id}`"),
                    );
                } else {
                    push_check(
                        items,
                        AnalysisCheckLevel::Error,
                        "analysis_id_duplicate",
                        format!("analysis id `{id}` is used more than once"),
                    );
                }
            }
            None => push_check(
                items,
                AnalysisCheckLevel::Error,
                "analysis_id_missing",
                format!("{step_label} is missing required `id`"),
            ),
        }

        match step.kind {
            AnalysisKind::Inspect => {}
            AnalysisKind::TableOne => {
                if let Some(by) =
                    required_contract_field(items, &step_label, "by", step.by.as_deref())
                {
                    check_column_reference(items, header_index, "table_one.by", by);
                    validate_variable_kind(
                        items,
                        declared_variables,
                        by,
                        &[
                            VariableKind::Binary,
                            VariableKind::Categorical,
                            VariableKind::Ordered,
                        ],
                        "categorical or binary grouping variable",
                    );
                }
            }
            AnalysisKind::Rate => {
                if let Some(event) =
                    required_contract_field(items, &step_label, "event", step.event.as_deref())
                {
                    check_column_reference(items, header_index, "rate.event", event);
                    validate_binary_observed_levels(items, data, header_index, event);
                }
                if let Some(person_time) = required_contract_field(
                    items,
                    &step_label,
                    "person_time",
                    step.person_time.as_deref(),
                ) {
                    check_column_reference(items, header_index, "rate.person_time", person_time);
                    validate_nonnegative_numeric_column(
                        items,
                        data,
                        header_index,
                        person_time,
                        true,
                    );
                }
            }
            AnalysisKind::Model => match step.model {
                Some(ModelKind::Logistic) => {
                    if let Some(outcome) = required_contract_field(
                        items,
                        &step_label,
                        "outcome",
                        step.outcome.as_deref(),
                    ) {
                        check_column_reference(items, header_index, "logistic.outcome", outcome);
                        validate_variable_kind(
                            items,
                            declared_variables,
                            outcome,
                            &[VariableKind::Binary, VariableKind::Event],
                            "binary outcome",
                        );
                        validate_binary_observed_levels(items, data, header_index, outcome);
                    }
                    validate_predictors(items, &step_label, header_index, declared_variables, step);
                }
                Some(ModelKind::Cox) => {
                    if let Some(time) =
                        required_contract_field(items, &step_label, "time", step.time.as_deref())
                    {
                        check_column_reference(items, header_index, "cox.time", time);
                        validate_nonnegative_numeric_column(items, data, header_index, time, false);
                    }
                    if let Some(event) =
                        required_contract_field(items, &step_label, "event", step.event.as_deref())
                    {
                        check_column_reference(items, header_index, "cox.event", event);
                        validate_binary_observed_levels(items, data, header_index, event);
                    }
                    validate_predictors(items, &step_label, header_index, declared_variables, step);
                }
                Some(ModelKind::Linear) => {
                    if let Some(outcome) = required_contract_field(
                        items,
                        &step_label,
                        "outcome",
                        step.outcome.as_deref(),
                    ) {
                        check_column_reference(items, header_index, "linear.outcome", outcome);
                        validate_variable_kind(
                            items,
                            declared_variables,
                            outcome,
                            &[VariableKind::Continuous],
                            "continuous outcome",
                        );
                        validate_numeric_column(items, data, header_index, outcome);
                    }
                    validate_predictors(items, &step_label, header_index, declared_variables, step);
                }
                None => push_check(
                    items,
                    AnalysisCheckLevel::Error,
                    "model_missing",
                    format!("{step_label} has kind `model` but no `model` field"),
                ),
            },
        }
    }
}

fn step_label(index: usize, id: Option<&str>) -> String {
    id.filter(|id| !id.trim().is_empty()).map_or_else(
        || format!("analysis step #{index}"),
        |id| format!("analysis `{}`", id.trim()),
    )
}

fn required_contract_field<'a>(
    items: &mut Vec<AnalysisCheckItem>,
    step_label: &str,
    field: &str,
    value: Option<&'a str>,
) -> Option<&'a str> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or_else(|| {
            push_check(
                items,
                AnalysisCheckLevel::Error,
                "analysis_field_missing",
                format!("{step_label} requires `{field}`"),
            );
            None
        })
}

fn validate_predictors(
    items: &mut Vec<AnalysisCheckItem>,
    step_label: &str,
    header_index: &BTreeMap<String, usize>,
    declared_variables: &BTreeMap<String, VariableKind>,
    step: &crate::schema::AnalysisStepSpec,
) {
    if step.predictors.is_empty() {
        push_check(
            items,
            AnalysisCheckLevel::Error,
            "predictors_missing",
            format!("{step_label} requires at least one predictor"),
        );
    }
    for name in step
        .predictors
        .iter()
        .chain(step.adjust.iter())
        .chain(step.strata.iter())
    {
        check_column_reference(items, header_index, "analysis variable", name);
        if !declared_variables.contains_key(name) {
            push_check(
                items,
                AnalysisCheckLevel::Warning,
                "variable_not_declared",
                format!("analysis variable `{name}` is used but not declared under `variables`"),
            );
        }
    }
}

fn check_column_reference(
    items: &mut Vec<AnalysisCheckItem>,
    header_index: &BTreeMap<String, usize>,
    field: &str,
    name: &str,
) {
    if header_index.contains_key(name) {
        push_check(
            items,
            AnalysisCheckLevel::Ok,
            "column_found",
            format!("{field} references existing column `{name}`"),
        );
    } else {
        push_check(
            items,
            AnalysisCheckLevel::Error,
            "column_missing",
            format!("{field} references missing column `{name}`"),
        );
    }
}

fn validate_variable_kind(
    items: &mut Vec<AnalysisCheckItem>,
    declared_variables: &BTreeMap<String, VariableKind>,
    name: &str,
    accepted: &[VariableKind],
    expected_label: &str,
) {
    match declared_variables.get(name) {
        Some(kind) if accepted.contains(kind) => push_check(
            items,
            AnalysisCheckLevel::Ok,
            "variable_kind_ok",
            format!("variable `{name}` is declared as {kind:?}"),
        ),
        Some(kind) => push_check(
            items,
            AnalysisCheckLevel::Error,
            "variable_kind_mismatch",
            format!("variable `{name}` is declared as {kind:?}, expected {expected_label}"),
        ),
        None => push_check(
            items,
            AnalysisCheckLevel::Warning,
            "variable_not_declared",
            format!("column `{name}` is used but not declared under `variables`"),
        ),
    }
}

fn validate_binary_observed_levels(
    items: &mut Vec<AnalysisCheckItem>,
    data: &DataSnapshot,
    header_index: &BTreeMap<String, usize>,
    name: &str,
) {
    let Ok(index) = require_column(header_index, name) else {
        return;
    };
    let levels = observed_levels(data, name, index);
    if levels.len() == 2 {
        push_check(
            items,
            AnalysisCheckLevel::Ok,
            "binary_levels_ok",
            format!("`{name}` has 2 observed non-missing level(s)"),
        );
    } else {
        push_check(
            items,
            AnalysisCheckLevel::Error,
            "binary_levels_invalid",
            format!(
                "`{name}` must have exactly 2 observed non-missing levels; found {}: {}",
                levels.len(),
                display_levels(&levels)
            ),
        );
    }
}

fn observed_levels(data: &DataSnapshot, column_name: &str, index: usize) -> BTreeSet<String> {
    data.records
        .iter()
        .filter_map(|record| record.get(index))
        .map(|value| value.trim())
        .filter(|value| !is_missing_value_for_column(column_name, value))
        .map(ToOwned::to_owned)
        .take(128)
        .collect()
}

fn display_levels(levels: &BTreeSet<String>) -> String {
    if levels.is_empty() {
        return "<none>".to_string();
    }
    levels
        .iter()
        .take(8)
        .cloned()
        .collect::<Vec<_>>()
        .join(", ")
}

fn validate_nonnegative_numeric_column(
    items: &mut Vec<AnalysisCheckItem>,
    data: &DataSnapshot,
    header_index: &BTreeMap<String, usize>,
    name: &str,
    allow_zero: bool,
) {
    let summary = validate_numeric_column(items, data, header_index, name);
    if let Some((non_missing, negative_count, zero_count)) = summary {
        if negative_count > 0 {
            push_check(
                items,
                AnalysisCheckLevel::Error,
                "numeric_negative",
                format!("`{name}` contains {negative_count} negative value(s)"),
            );
        }
        if !allow_zero && zero_count > 0 {
            push_check(
                items,
                AnalysisCheckLevel::Error,
                "time_nonpositive",
                format!("`{name}` contains {zero_count} zero value(s); Cox time must be > 0"),
            );
        } else if allow_zero && zero_count > 0 && non_missing > 0 {
            push_check(
                items,
                AnalysisCheckLevel::Warning,
                "person_time_zero",
                format!("`{name}` contains {zero_count} zero person-time value(s)"),
            );
        }
    }
}

fn validate_numeric_column(
    items: &mut Vec<AnalysisCheckItem>,
    data: &DataSnapshot,
    header_index: &BTreeMap<String, usize>,
    name: &str,
) -> Option<(usize, usize, usize)> {
    let Ok(index) = require_column(header_index, name) else {
        return None;
    };
    let mut non_missing = 0;
    let mut invalid = 0;
    let mut negative = 0;
    let mut zero = 0;
    for record in &data.records {
        let raw = record.get(index).map_or("", String::as_str).trim();
        if is_missing_value_for_column(name, raw) {
            continue;
        }
        non_missing += 1;
        match raw.parse::<f64>() {
            Ok(value) if value.is_finite() => {
                if value < 0.0 {
                    negative += 1;
                }
                if value == 0.0 {
                    zero += 1;
                }
            }
            _ => invalid += 1,
        }
    }
    if non_missing == 0 {
        push_check(
            items,
            AnalysisCheckLevel::Error,
            "numeric_empty",
            format!("`{name}` has no observed non-missing numeric values"),
        );
    } else if invalid == 0 {
        push_check(
            items,
            AnalysisCheckLevel::Ok,
            "numeric_values_ok",
            format!("`{name}` has {non_missing} numeric non-missing value(s)"),
        );
    } else {
        push_check(
            items,
            AnalysisCheckLevel::Error,
            "numeric_values_invalid",
            format!("`{name}` contains {invalid} non-numeric or non-finite value(s)"),
        );
    }
    Some((non_missing, negative, zero))
}
