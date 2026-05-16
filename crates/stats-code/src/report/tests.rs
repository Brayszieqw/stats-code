use std::fs;
use std::path::PathBuf;

use serde_json::json;

use crate::cli::{
    Command, ReportBuildArgs, ReportCommand, ReportVerifyArgs, WorkflowCommand, WorkflowRunArgs,
};
use crate::helpers::{fingerprint_file, resolve_path_for_match};
use crate::schema::{AnalysisSpec, Diagnostic, LogisticCoefficient, LogisticResult};

fn temp_dir(label: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("time after epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("epistat-{label}-{nanos}"))
}

fn test_cli(command: Command) -> crate::cli::Cli {
    crate::cli::Cli {
        json: false,
        artifacts_dir: None,
        session: None,
        model: "gpt".to_string(),
        system: None,
        max_tokens: None,
        engine: crate::bridge::Engine::Rust,
        alpha: 0.05,
        na_strategy: crate::cli::NaStrategy::Drop,
        command: Some(command),
    }
}

fn write_minimal_verified_report_artifacts(root: &std::path::Path) -> PathBuf {
    let out_dir = root.join("artifacts");
    let audit_dir = out_dir.join("audit");
    let report_dir = out_dir.join("report");
    let run_dir = out_dir.join("inspect-main");
    fs::create_dir_all(&audit_dir).expect("create audit dir");
    fs::create_dir_all(&report_dir).expect("create report dir");
    fs::create_dir_all(&run_dir).expect("create run dir");

    let analysis_path = root.join("analysis.yaml");
    let data_path = root.join("demo.csv");
    let result_path = run_dir.join("result.json");
    let context_path = run_dir.join("context.json");
    fs::write(&analysis_path, "schema_version: stats-code.v0\n").expect("write analysis");
    fs::write(&data_path, "disease\n1\n0\n").expect("write data");
    fs::write(&result_path, r#"{"status":"ok"}"#).expect("write result");
    fs::write(&context_path, r#"{"command":"inspect"}"#).expect("write context");
    fs::write(report_dir.join("report.md"), "# Report\n").expect("write report");
    fs::write(audit_dir.join("analysis_manifest.json"), "{}").expect("write manifest");
    fs::write(
        audit_dir.join("run.json"),
        serde_json::to_string_pretty(&json!({
            "schema_version": "stats-code.v0",
            "stats_code_version": "0.1.0",
            "analysis_path": analysis_path.display().to_string(),
            "data_path": data_path.display().to_string(),
            "analysis_fingerprint_fnv1a64": "analysis-hash",
            "data_fingerprint_fnv1a64": "data-hash",
        }))
        .expect("serialize run"),
    )
    .expect("write run");
    fs::write(
        audit_dir.join("evidence-index.json"),
        serde_json::to_string_pretty(&json!({
            "artifacts_dir": out_dir.display().to_string(),
            "query": {
                "analysis_path": analysis_path.display().to_string(),
                "data_path": data_path.display().to_string(),
                "data_fingerprint_fnv1a64": "data-hash",
                "include_exploratory": false,
            },
            "discovered_runs": [],
            "accepted_artifacts": [
                {
                    "command": "inspect",
                    "run_dir": run_dir.display().to_string(),
                    "result_path": result_path.display().to_string(),
                    "context_path": context_path.display().to_string(),
                    "status": "accepted",
                    "reason": "matched declared analysis step",
                    "matched_by": "analysis_step",
                    "matched_analysis_step_index": 0,
                    "artifact": {
                        "role": "declared",
                        "status": "produced",
                        "formal_run_id": "run-1",
                        "analysis_step_index": 0,
                    },
                }
            ],
            "rejected_artifacts": [],
            "notes": [],
        }))
        .expect("serialize evidence"),
    )
    .expect("write evidence");

    out_dir
}

fn unstable_logistic_result() -> LogisticResult {
    LogisticResult {
        status: "ok".to_string(),
        validity_status: "unstable".to_string(),
        data_path: "demo.csv".to_string(),
        analysis_path: Some("analysis.yaml".to_string()),
        formula: "logit(disease ~ age)".to_string(),
        outcome: "disease".to_string(),
        predictors: vec!["age".to_string()],
        survey_weight: None,
        n_total: 36,
        n_used: 36,
        n_excluded_missing: 0,
        n_excluded_invalid: 0,
        n_events: 14,
        n_nonevents: 22,
        iterations: 50,
        converged: false,
        log_likelihood: -0.0001,
        null_log_likelihood: None,
        pseudo_r2_nagelkerke: None,
        aic: None,
        bic: None,
        c_statistic: None,
        coefficients: vec![LogisticCoefficient {
            term: "age".to_string(),
            variable: "age".to_string(),
            level: None,
            reference: None,
            beta: 29.2379,
            standard_error: 8064.97,
            odds_ratio: 4_987_598_079_561.157,
            ci_lower: 0.0,
            ci_upper: f64::MAX,
            p_value: 0.9971,
        }],
        notes: vec![],
        diagnostics: vec![Diagnostic::blocking(
            "unstable_confidence_interval",
            "Confidence interval is unstable.",
            None,
        )],
        warnings: vec![
            "model_did_not_converge_within_max_iterations".to_string(),
            "possible_separation_or_extreme_fitted_probabilities".to_string(),
        ],
    }
}

#[test]
fn model_markdown_marks_unstable_intervals_and_warnings() {
    let markdown = super::build_logistic_markdown(&unstable_logistic_result());

    assert!(markdown.contains(
            "- Warnings: model_did_not_converge_within_max_iterations, possible_separation_or_extreme_fitted_probabilities."
        ));
    assert!(markdown.contains("| age | 4.9876e12 | unstable | 0.9971 |"));
    assert!(!markdown.contains("17976931348623157"));
}

#[test]
fn report_markdown_marks_unstable_model_summaries_and_warnings() {
    let spec: AnalysisSpec = serde_yaml::from_str(
        r"
study:
  title: Demo cohort
  design: cohort
data:
  path: demo.csv
  format: csv
analyses:
  - kind: model
    model: logistic
    outcome: disease
    predictors: [age]
",
    )
    .expect("parse analysis spec");
    let evidence = super::ReportEvidence {
        source_dir: PathBuf::from("runs"),
        logistic: Some(unstable_logistic_result()),
        ..super::ReportEvidence::default()
    };

    let report = super::build_report_markdown_from_evidence(&spec, &evidence);

    assert!(report.contains("age OR 4.99e12 (CI unstable)"));
    assert!(report.contains(
            "Logistic model warnings: model_did_not_converge_within_max_iterations, possible_separation_or_extreme_fitted_probabilities."
        ));
    assert!(!report.contains("17976931348623157"));
}

#[test]
fn report_build_writes_expected_scaffold_files() {
    let root = temp_dir("report");
    fs::create_dir_all(&root).expect("create root");
    let analysis_path = root.join("analysis.yaml");
    fs::write(
        &analysis_path,
        r"
study:
  title: Demo cohort
  design: cohort
  population: Adults under surveillance
study_context:
  estimand: 1-year risk ratio
  exposure: Smoking
  comparator: Never smoking
  outcome: Incident disease
  time_zero: Baseline exam date
  follow_up: 12 months
  censoring: Death or loss to follow-up
  missing_data_strategy: Multiple imputation
  clustering: site
  sensitivity_analyses: Alternate exposure coding
  reporting_guideline: STROBE
data:
  path: demo.csv
  format: csv
variables:
  - name: disease
    kind: binary
    roles: [outcome]
  - name: age
    kind: continuous
    roles: [covariate]
analyses:
  - kind: inspect
  - kind: table_one
    by: disease
  - kind: model
    model: logistic
    outcome: disease
    predictors: [age]
report:
  out_dir: epistat-artifacts
  include_methods: true
  include_tables: true
  include_assumptions: true
audit:
  log_dir: epistat-artifacts/audit
  save_commands: true
  save_inputs: true
  save_outputs: true
  save_environment: true
  save_decisions: true
",
    )
    .expect("write analysis yaml");

    let out_dir = root.join("artifacts");
    let cli = test_cli(Command::Report {
        command: ReportCommand::Build(ReportBuildArgs {
            analysis: analysis_path.clone(),
            out: Some(out_dir.clone()),
            artifacts: None,
            include_exploratory: false,
        }),
    });

    let rendered = crate::handlers::dispatch(&cli).expect("report build should succeed");
    assert!(rendered.contains("Report Build"));
    assert!(out_dir.join("report").join("methods.md").is_file());
    assert!(out_dir.join("report").join("study-context.md").is_file());
    assert!(out_dir
        .join("report")
        .join("reporting-checklist.md")
        .is_file());
    assert!(out_dir
        .join("audit")
        .join("analysis.normalized.json")
        .is_file());
    assert!(out_dir
        .join("audit")
        .join("analysis_manifest.json")
        .is_file());
    assert!(out_dir.join("audit").join("run.json").is_file());
    assert!(out_dir.join("audit").join("audit-trail.md").is_file());
    assert!(out_dir.join("audit").join("evidence-index.json").is_file());
    let checklist = fs::read_to_string(out_dir.join("report").join("reporting-checklist.md"))
        .expect("read checklist");
    assert!(checklist.contains("STROBE"));
    assert!(checklist.contains("estimand"));
    let manifest = fs::read_to_string(out_dir.join("audit").join("analysis_manifest.json"))
        .expect("read manifest");
    assert!(manifest.contains("\"analysis_fingerprint_fnv1a64\""));
    assert!(manifest.contains("\"reporting\""));

    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn report_verify_accepts_evidence_index_with_existing_artifacts() {
    let root = temp_dir("report-verify-ok");
    fs::create_dir_all(&root).expect("create root");
    let out_dir = write_minimal_verified_report_artifacts(&root);

    let result = super::handle_report_verify(&ReportVerifyArgs {
        artifacts: out_dir.clone(),
        fail_on_warning: false,
    });
    assert_eq!(result.status, "ok");
    assert_eq!(result.accepted_count, 1);
    assert_eq!(result.rejected_count, 0);
    assert_eq!(result.error_count, 0);
    assert!(result
        .items
        .iter()
        .any(|item| item.code == "data_fingerprint_matches"));

    let rendered = crate::handlers::dispatch(&test_cli(Command::Report {
        command: ReportCommand::Verify(ReportVerifyArgs {
            artifacts: out_dir,
            fail_on_warning: false,
        }),
    }))
    .expect("report verify should render");
    assert!(rendered.contains("Report Verify"));
    assert!(rendered.contains("Status           ok"));
    assert!(rendered.contains("accepted=1 rejected=0 errors=0"));

    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn report_verify_reports_missing_accepted_result_path() {
    let root = temp_dir("report-verify-missing-result");
    fs::create_dir_all(&root).expect("create root");
    let out_dir = write_minimal_verified_report_artifacts(&root);
    fs::remove_file(out_dir.join("inspect-main").join("result.json")).expect("remove result");

    let result = super::handle_report_verify(&ReportVerifyArgs {
        artifacts: out_dir,
        fail_on_warning: false,
    });
    assert_eq!(result.status, "error");
    assert!(result.error_count > 0);
    assert!(result
        .items
        .iter()
        .any(|item| item.code == "artifact_result_missing"));

    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn report_verify_reports_accepted_artifact_with_blocking_diagnostics() {
    let root = temp_dir("report-verify-blocking-diagnostics");
    fs::create_dir_all(&root).expect("create root");
    let out_dir = write_minimal_verified_report_artifacts(&root);
    fs::write(
        out_dir.join("inspect-main").join("result.json"),
        serde_json::to_string_pretty(&json!({
            "status": "ok",
            "diagnostics": [
                {
                    "code": "unstable_confidence_interval",
                    "severity": "blocking",
                    "message": "Confidence interval is unstable."
                }
            ]
        }))
        .expect("serialize result"),
    )
    .expect("write result");

    let result = super::handle_report_verify(&ReportVerifyArgs {
        artifacts: out_dir,
        fail_on_warning: false,
    });
    assert_eq!(result.status, "error");
    assert!(result
        .items
        .iter()
        .any(|item| item.code == "accepted_artifact_blocking_diagnostics"));

    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn workflow_report_rejects_logistic_with_blocking_diagnostics() {
    let root = temp_dir("workflow-logistic-blocking-diagnostics");
    fs::create_dir_all(&root).expect("create root");
    let analysis_path = root.join("analysis.yaml");
    fs::write(
        &analysis_path,
        r"
schema_version: stats-code.v0
study:
  title: Separation demo
  design: cohort
study_context:
  estimand: Odds ratio
  exposure: Treatment
  comparator: Control
  outcome: Outcome
  missing_data_strategy: Complete-case analysis
  reporting_guideline: STROBE
data:
  path: demo.csv
  format: csv
variables:
  - name: outcome
    kind: binary
    roles: [outcome]
  - name: treatment
    kind: binary
    roles: [exposure]
  - name: age
    kind: continuous
    roles: [covariate]
analyses:
  - id: logistic_sep
    kind: model
    model: logistic
    outcome: outcome
    predictors: [treatment, age]
report:
  out_dir: artifacts
  include_methods: true
  include_tables: true
  include_assumptions: true
",
    )
    .expect("write analysis");
    fs::write(
        root.join("demo.csv"),
        "outcome,treatment,age\n0,0,40\n0,0,42\n0,0,44\n0,0,46\n1,1,50\n1,1,52\n1,1,54\n1,1,56\n",
    )
    .expect("write csv");
    let out_dir = root.join("artifacts");
    crate::handlers::dispatch(&test_cli(Command::Workflow {
        command: WorkflowCommand::Run(WorkflowRunArgs {
            analysis: analysis_path,
            out: Some(out_dir.clone()),
            explore_out: None,
            include_exploratory: false,
            strict: false,
            allow_warnings: false,
            allow_unenforced_survey: false,
            allow_unenforced_privacy: false,
            no_chat: true,
        }),
    }))
    .expect("workflow should execute and reject bad formal evidence");

    let step_dir = fs::read_dir(&out_dir)
        .expect("read artifacts")
        .filter_map(std::result::Result::ok)
        .map(|entry| entry.path())
        .find(|path| path.join("context.json").is_file())
        .expect("step artifact with context");
    let command_json =
        fs::read_to_string(step_dir.join("command.json")).expect("read command json");
    let context_json =
        fs::read_to_string(step_dir.join("context.json")).expect("read context json");
    assert!(command_json.contains("\"artifact_schema_version\": \"1.0\""));
    assert!(context_json.contains("\"artifact_schema_version\": \"1.0\""));
    assert!(context_json.contains("\"stats_code_version\""));

    let report_md =
        fs::read_to_string(out_dir.join("report").join("report.md")).expect("read report");
    let evidence_index = fs::read_to_string(out_dir.join("audit").join("evidence-index.json"))
        .expect("read evidence index");
    assert!(report_md.contains("Regression models: adjusted effect estimates."));
    assert!(!report_md.contains("2.9804e44"));
    assert!(evidence_index.contains("\"rejected_artifacts\""));
    assert!(evidence_index.contains("artifact has blocking diagnostics"));
    assert!(evidence_index.contains("possible_complete_separation"));
    assert!(evidence_index.contains("\"report_decision\": \"rejected\""));

    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn report_build_only_reports_missing_declared_analyses() {
    let root = temp_dir("report-declared-missing-only");
    fs::create_dir_all(&root).expect("create root");
    let analysis_path = root.join("analysis.yaml");
    fs::write(
        &analysis_path,
        r"
study:
  title: Descriptive-only cohort
  design: cross-sectional
study_context:
  estimand: Descriptive prevalence summaries
  exposure: Category
  comparator: Other categories
  outcome: Prevalence
  missing_data_strategy: Report missing values
  reporting_guideline: STROBE
data:
  path: demo.csv
  format: csv
variables:
  - name: category
    kind: categorical
    roles: [exposure]
  - name: data_value
    kind: continuous
    roles: [outcome]
analyses:
  - kind: inspect
  - kind: table_one
    by: category
report:
  out_dir: epistat-artifacts
  include_methods: true
  include_tables: true
  include_assumptions: true
",
    )
    .expect("write analysis yaml");
    let data_path = root.join("demo.csv");
    fs::write(&data_path, "category,data_value\nA,1.0\nB,2.0\n").expect("write csv");
    let artifacts_dir = root.join("runs");
    let tableone_dir = artifacts_dir.join("tableone-1");
    fs::create_dir_all(&tableone_dir).expect("create tableone dir");
    fs::write(
        tableone_dir.join("command.json"),
        r#"{"command":"tableone","request":{}}"#,
    )
    .expect("write tableone command");
    fs::write(
        tableone_dir.join("context.json"),
        serde_json::to_string_pretty(&json!({
            "command": "tableone",
            "analysis_path": analysis_path.display().to_string(),
            "analysis_path_resolved": resolve_path_for_match(&analysis_path),
            "data_path": data_path.display().to_string(),
            "data_path_resolved": resolve_path_for_match(&data_path),
            "data_fingerprint_fnv1a64": fingerprint_file(&data_path).expect("fingerprint"),
            "cwd": root.display().to_string(),
        }))
        .expect("serialize tableone context"),
    )
    .expect("write tableone context");
    fs::write(
            tableone_dir.join("result.json"),
            r#"{
  "status":"ok",
  "data_path":"demo.csv",
  "analysis_path":"analysis.yaml",
  "by":"category",
  "group_levels":["A","B"],
  "rows":[
    {
      "variable":"data_value",
      "kind":"continuous",
      "overall":{"display":"1.50 (0.71); median 1.50 [1.00, 2.00]","n_total":2,"n_non_missing":2,"missing_count":0},
      "groups":[
        {"group":"A","cell":{"display":"1.00 (NA); median 1.00 [1.00, 1.00]","n_total":1,"n_non_missing":1,"missing_count":0}},
        {"group":"B","cell":{"display":"2.00 (NA); median 2.00 [2.00, 2.00]","n_total":1,"n_non_missing":1,"missing_count":0}}
      ],
      "test_name":"Welch_t_test",
      "p_value":0.0,
      "warnings":[]
    }
  ],
  "notes":[]
}"#,
        )
        .expect("write tableone result");

    let out_dir = root.join("artifacts");
    let cli = test_cli(Command::Report {
        command: ReportCommand::Build(ReportBuildArgs {
            analysis: analysis_path,
            out: Some(out_dir.clone()),
            artifacts: Some(artifacts_dir),
            include_exploratory: false,
        }),
    });

    crate::handlers::dispatch(&cli).expect("report build should succeed");
    let report_md =
        fs::read_to_string(out_dir.join("report").join("report.md")).expect("read report");
    assert!(report_md.contains("Table 1 available for `category`"));
    assert!(!report_md.contains("Rate analysis: no observed result found."));
    assert!(!report_md.contains("Logistic model: no observed result found."));
    assert!(!report_md.contains("Cox model: no observed result found."));
    let table_md =
        fs::read_to_string(out_dir.join("tables").join("tableone.md")).expect("read table");
    assert!(table_md.contains("data_value"));

    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn report_build_selects_tableone_matching_declared_by() {
    let root = temp_dir("report-tableone-declared-by");
    fs::create_dir_all(&root).expect("create root");
    let analysis_path = root.join("analysis.yaml");
    fs::write(
        &analysis_path,
        r"
study:
  title: Descriptive-only cohort
  design: cross-sectional
study_context:
  estimand: Descriptive prevalence summaries
  exposure: Category
  comparator: Other categories
  outcome: Prevalence
  missing_data_strategy: Report missing values
  reporting_guideline: STROBE
data:
  path: demo.csv
  format: csv
variables:
  - name: category
    kind: categorical
    roles: [exposure]
  - name: year
    kind: categorical
    roles: [strata]
  - name: data_value
    kind: continuous
    roles: [outcome]
analyses:
  - kind: inspect
  - kind: table_one
    by: category
report:
  out_dir: epistat-artifacts
  include_methods: true
  include_tables: true
  include_assumptions: true
",
    )
    .expect("write analysis yaml");
    let data_path = root.join("demo.csv");
    fs::write(
        &data_path,
        "category,year,data_value\nA,2022,1.0\nB,2023,2.0\n",
    )
    .expect("write csv");
    let data_fingerprint = fingerprint_file(&data_path).expect("fingerprint");
    let artifacts_dir = root.join("runs");
    let category_dir = artifacts_dir.join("tableone-category");
    let year_dir = artifacts_dir.join("tableone-year");
    fs::create_dir_all(&category_dir).expect("create category dir");
    fs::create_dir_all(&year_dir).expect("create year dir");

    for run_dir in [&category_dir, &year_dir] {
        fs::write(
            run_dir.join("command.json"),
            r#"{"command":"tableone","request":{}}"#,
        )
        .expect("write tableone command");
        fs::write(
            run_dir.join("context.json"),
            serde_json::to_string_pretty(&json!({
                "command": "tableone",
                "analysis_path": analysis_path.display().to_string(),
                "analysis_path_resolved": resolve_path_for_match(&analysis_path),
                "data_path": data_path.display().to_string(),
                "data_path_resolved": resolve_path_for_match(&data_path),
                "data_fingerprint_fnv1a64": data_fingerprint,
                "cwd": root.display().to_string(),
            }))
            .expect("serialize context"),
        )
        .expect("write context");
    }

    fs::write(
        category_dir.join("result.json"),
        r#"{
  "status":"ok",
  "data_path":"demo.csv",
  "analysis_path":"analysis.yaml",
  "by":"category",
  "group_levels":["A","B"],
  "rows":[
    {
      "variable":"data_value",
      "kind":"continuous",
      "overall":{"display":"1.50 (0.71)","n_total":2,"n_non_missing":2,"missing_count":0},
      "groups":[
        {"group":"A","cell":{"display":"1.00","n_total":1,"n_non_missing":1,"missing_count":0}},
        {"group":"B","cell":{"display":"2.00","n_total":1,"n_non_missing":1,"missing_count":0}}
      ],
      "warnings":[]
    }
  ],
  "notes":[]
}"#,
    )
    .expect("write category result");
    fs::write(
        year_dir.join("result.json"),
        r#"{
  "status":"ok",
  "data_path":"demo.csv",
  "analysis_path":"analysis.yaml",
  "by":"year",
  "group_levels":["2022","2023"],
  "rows":[
    {
      "variable":"data_value",
      "kind":"continuous",
      "overall":{"display":"1.50 (0.71)","n_total":2,"n_non_missing":2,"missing_count":0},
      "groups":[
        {"group":"2022","cell":{"display":"1.00","n_total":1,"n_non_missing":1,"missing_count":0}},
        {"group":"2023","cell":{"display":"2.00","n_total":1,"n_non_missing":1,"missing_count":0}}
      ],
      "warnings":[]
    }
  ],
  "notes":[]
}"#,
    )
    .expect("write year result");

    let out_dir = root.join("artifacts");
    let cli = test_cli(Command::Report {
        command: ReportCommand::Build(ReportBuildArgs {
            analysis: analysis_path,
            out: Some(out_dir.clone()),
            artifacts: Some(artifacts_dir),
            include_exploratory: false,
        }),
    });

    crate::handlers::dispatch(&cli).expect("report build should succeed");
    let report_md =
        fs::read_to_string(out_dir.join("report").join("report.md")).expect("read report");
    let table_md =
        fs::read_to_string(out_dir.join("tables").join("tableone.md")).expect("read table");
    let evidence_index = fs::read_to_string(out_dir.join("audit").join("evidence-index.json"))
        .expect("read evidence index");
    assert!(report_md.contains("Table 1 available for `category`"));
    assert!(!report_md.contains("Table 1 available for `year`"));
    assert!(table_md.contains("| Variable | Overall | A | B |"));
    assert!(!table_md.contains("| Variable | Overall | 2022 | 2023 |"));
    assert!(evidence_index.contains("\"accepted_artifacts\""));
    assert!(evidence_index.contains("\"rejected_artifacts\""));
    assert!(evidence_index.contains("artifact does not match a declared analysis step"));
    assert!(evidence_index.contains("\"matched_analysis_step_index\": 1"));

    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn report_build_rejects_exploratory_artifacts_by_default() {
    let root = temp_dir("report-exploratory-filter");
    fs::create_dir_all(&root).expect("create root");
    let analysis_path = root.join("analysis.yaml");
    fs::write(
        &analysis_path,
        r"
study:
  title: Exploratory filter cohort
  design: cross-sectional
study_context:
  estimand: Descriptive prevalence summaries
  exposure: Category
  comparator: Other categories
  outcome: Prevalence
  missing_data_strategy: Report missing values
  reporting_guideline: STROBE
data:
  path: demo.csv
  format: csv
variables:
  - name: category
    kind: categorical
    roles: [exposure]
  - name: data_value
    kind: continuous
    roles: [outcome]
analyses:
  - kind: table_one
    by: category
report:
  out_dir: epistat-artifacts
  include_methods: true
  include_tables: true
  include_assumptions: true
",
    )
    .expect("write analysis yaml");
    let data_path = root.join("demo.csv");
    fs::write(&data_path, "category,data_value\nA,1.0\nB,2.0\n").expect("write csv");

    let artifacts_dir = root.join("runs");
    let tableone_dir = artifacts_dir.join("tableone-explore");
    fs::create_dir_all(&tableone_dir).expect("create tableone dir");
    fs::write(
        tableone_dir.join("command.json"),
        r#"{"command":"tableone","request":{}}"#,
    )
    .expect("write tableone command");
    fs::write(
        tableone_dir.join("context.json"),
        serde_json::to_string_pretty(&json!({
            "command": "tableone",
            "analysis_path": analysis_path.display().to_string(),
            "analysis_path_resolved": resolve_path_for_match(&analysis_path),
            "data_path": data_path.display().to_string(),
            "data_path_resolved": resolve_path_for_match(&data_path),
            "data_fingerprint_fnv1a64": fingerprint_file(&data_path).expect("fingerprint"),
            "cwd": root.display().to_string(),
            "artifact": {
                "role": "exploratory",
                "status": "produced"
            }
        }))
        .expect("serialize context"),
    )
    .expect("write context");
    fs::write(
        tableone_dir.join("result.json"),
        r#"{
  "status":"ok",
  "data_path":"demo.csv",
  "analysis_path":"analysis.yaml",
  "by":"category",
  "group_levels":["A","B"],
  "rows":[
    {
      "variable":"data_value",
      "kind":"continuous",
      "overall":{"display":"1.50 (0.71)","n_total":2,"n_non_missing":2,"missing_count":0},
      "groups":[
        {"group":"A","cell":{"display":"1.00","n_total":1,"n_non_missing":1,"missing_count":0}},
        {"group":"B","cell":{"display":"2.00","n_total":1,"n_non_missing":1,"missing_count":0}}
      ],
      "warnings":[]
    }
  ],
  "notes":[]
}"#,
    )
    .expect("write result");

    let out_dir = root.join("formal-report");
    let cli = test_cli(Command::Report {
        command: ReportCommand::Build(ReportBuildArgs {
            analysis: analysis_path,
            out: Some(out_dir.clone()),
            artifacts: Some(artifacts_dir),
            include_exploratory: false,
        }),
    });

    crate::handlers::dispatch(&cli).expect("report build should reject exploratory evidence");
    assert!(!out_dir.join("tables").join("tableone.md").exists());
    let evidence_index = fs::read_to_string(out_dir.join("audit").join("evidence-index.json"))
        .expect("read evidence index");
    assert!(evidence_index.contains("\"rejected_artifacts\""));
    assert!(evidence_index.contains("exploratory artifact was not requested"));
    assert!(evidence_index.contains("\"role\": \"exploratory\""));

    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn report_build_consumes_observed_result_artifacts() {
    let root = temp_dir("report-evidence");
    fs::create_dir_all(&root).expect("create root");
    let analysis_path = root.join("analysis.yaml");
    let data_path = root.join("demo.csv");
    fs::write(
        &analysis_path,
        r"
study:
  title: Demo cohort
  design: cohort
study_context:
  estimand: 1-year rate ratio and odds ratio
  outcome: Incident disease
  time_zero: Baseline visit
  follow_up: 12 months
  censoring: End of follow-up
  missing_data_strategy: Complete-case analysis
  reporting_guideline: STROBE
data:
  path: demo.csv
  format: csv
variables:
  - name: disease
    kind: binary
    roles: [outcome]
  - name: age
    kind: continuous
    roles: [covariate]
analyses:
  - kind: rate
    event: disease
    person_time: fu_pt
  - kind: model
    model: logistic
    outcome: disease
    predictors: [age]
report:
  out_dir: epistat-artifacts
  include_methods: true
  include_tables: true
  include_assumptions: true
",
    )
    .expect("write analysis yaml");
    fs::write(&data_path, "disease,fu_pt,age,sex\n1,1.0,50,female\n").expect("write csv");
    let data_fingerprint = fingerprint_file(&data_path).expect("fingerprint");

    let artifacts_dir = root.join("runs");
    let logistic_dir = artifacts_dir.join("model_logistic-1");
    let rate_dir = artifacts_dir.join("rate-1");
    fs::create_dir_all(&logistic_dir).expect("create logistic dir");
    fs::create_dir_all(&rate_dir).expect("create rate dir");
    fs::write(
        logistic_dir.join("command.json"),
        r#"{"command":"model_logistic","request":{}}"#,
    )
    .expect("write logistic command");
    fs::write(
        logistic_dir.join("result.json"),
        r#"{
  "status":"ok",
  "data_path":"demo.csv",
  "analysis_path":"analysis.yaml",
  "formula":"logit(disease ~ age)",
  "outcome":"disease",
  "predictors":["age"],
  "n_total":100,
  "n_used":96,
  "n_excluded_missing":4,
  "n_excluded_invalid":0,
  "n_events":24,
  "n_nonevents":72,
  "iterations":5,
  "converged":true,
  "log_likelihood":-48.12,
  "coefficients":[
    {
      "term":"Intercept",
      "variable":"Intercept",
      "beta":-2.1,
      "standard_error":0.8,
      "odds_ratio":0.1225,
      "ci_lower":0.025,
      "ci_upper":0.600,
      "p_value":0.01
    },
    {
      "term":"age",
      "variable":"age",
      "beta":0.08,
      "standard_error":0.03,
      "odds_ratio":1.0833,
      "ci_lower":1.02,
      "ci_upper":1.15,
      "p_value":0.008
    }
  ],
  "notes":["demo logistic"],
  "warnings":[]
}"#,
    )
    .expect("write logistic result");
    fs::write(
        logistic_dir.join("context.json"),
        serde_json::to_string_pretty(&json!({
            "command": "model_logistic",
            "analysis_path": analysis_path.display().to_string(),
            "analysis_path_resolved": resolve_path_for_match(&analysis_path),
            "data_path": data_path.display().to_string(),
            "data_path_resolved": resolve_path_for_match(&data_path),
            "data_fingerprint_fnv1a64": data_fingerprint,
            "cwd": root.display().to_string(),
        }))
        .expect("serialize logistic context"),
    )
    .expect("write logistic context");
    fs::write(
        rate_dir.join("command.json"),
        r#"{"command":"rate","request":{}}"#,
    )
    .expect("write rate command");
    fs::write(
        rate_dir.join("result.json"),
        r#"{
  "status":"ok",
  "data_path":"demo.csv",
  "analysis_path":"analysis.yaml",
  "event":"disease",
  "person_time":"fu_pt",
  "strata":["sex"],
  "rows":[
    {
      "stratum":"sex=female",
      "total_records":50,
      "included_records":50,
      "events":10.0,
      "person_time":120.0,
      "rate":0.083333,
      "rate_per_1000":83.333,
      "lower_ci_per_1000":40.000,
      "upper_ci_per_1000":150.000
    }
  ],
  "notes":["demo rate"]
}"#,
    )
    .expect("write rate result");
    fs::write(
        rate_dir.join("context.json"),
        serde_json::to_string_pretty(&json!({
            "command": "rate",
            "analysis_path": analysis_path.display().to_string(),
            "analysis_path_resolved": resolve_path_for_match(&analysis_path),
            "data_path": data_path.display().to_string(),
            "data_path_resolved": resolve_path_for_match(&data_path),
            "data_fingerprint_fnv1a64": fingerprint_file(&data_path).expect("fingerprint"),
            "cwd": root.display().to_string(),
        }))
        .expect("serialize rate context"),
    )
    .expect("write rate context");

    let out_dir = root.join("artifacts");
    let cli = test_cli(Command::Report {
        command: ReportCommand::Build(ReportBuildArgs {
            analysis: analysis_path.clone(),
            out: Some(out_dir.clone()),
            artifacts: Some(artifacts_dir.clone()),
            include_exploratory: false,
        }),
    });

    let rendered = crate::handlers::dispatch(&cli).expect("report build should consume evidence");
    assert!(rendered.contains("Report Build"));
    let report_md =
        fs::read_to_string(out_dir.join("report").join("report.md")).expect("read report");
    assert!(report_md.contains("age OR 1.08"));
    assert!(report_md.contains("sex=female = 83.33/1000"));
    assert!(out_dir
        .join("tables")
        .join("model-logistic-summary.md")
        .is_file());
    assert!(out_dir.join("tables").join("rate-summary.md").is_file());
    assert!(out_dir.join("audit").join("evidence-index.json").is_file());

    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn report_build_ignores_mismatched_result_artifacts() {
    let root = temp_dir("report-evidence-mismatch");
    fs::create_dir_all(&root).expect("create root");
    let analysis_path = root.join("analysis.yaml");
    let data_path = root.join("demo.csv");
    let other_data_path = root.join("other.csv");
    fs::write(
        &analysis_path,
        r"
study:
  title: Demo cohort
  design: cohort
study_context:
  estimand: Adjusted odds ratio
  outcome: Incident disease
  missing_data_strategy: Complete-case analysis
  reporting_guideline: STROBE
data:
  path: demo.csv
  format: csv
variables:
  - name: disease
    kind: binary
    roles: [outcome]
  - name: age
    kind: continuous
    roles: [covariate]
analyses:
  - kind: model
    model: logistic
    outcome: disease
    predictors: [age]
report:
  out_dir: epistat-artifacts
  include_methods: true
  include_tables: true
  include_assumptions: true
",
    )
    .expect("write analysis yaml");
    fs::write(&data_path, "disease,age\n1,50\n0,40\n").expect("write primary csv");
    fs::write(&other_data_path, "disease,age\n1,80\n1,78\n").expect("write other csv");

    let artifacts_dir = root.join("runs");
    let matching_dir = artifacts_dir.join("model_logistic-match");
    let mismatched_dir = artifacts_dir.join("model_logistic-mismatch");
    fs::create_dir_all(&matching_dir).expect("create match dir");
    fs::create_dir_all(&mismatched_dir).expect("create mismatch dir");

    let matching_context = json!({
        "command": "model_logistic",
        "analysis_path": analysis_path.display().to_string(),
        "analysis_path_resolved": resolve_path_for_match(&analysis_path),
        "data_path": data_path.display().to_string(),
        "data_path_resolved": resolve_path_for_match(&data_path),
        "data_fingerprint_fnv1a64": fingerprint_file(&data_path).expect("primary fingerprint"),
        "cwd": root.display().to_string(),
    });
    let mismatched_context = json!({
        "command": "model_logistic",
        "analysis_path": analysis_path.display().to_string(),
        "analysis_path_resolved": resolve_path_for_match(&analysis_path),
        "data_path": other_data_path.display().to_string(),
        "data_path_resolved": resolve_path_for_match(&other_data_path),
        "data_fingerprint_fnv1a64": fingerprint_file(&other_data_path).expect("other fingerprint"),
        "cwd": root.display().to_string(),
    });

    fs::write(
        matching_dir.join("command.json"),
        r#"{"command":"model_logistic","request":{}}"#,
    )
    .expect("write matching command");
    fs::write(
        matching_dir.join("context.json"),
        serde_json::to_string_pretty(&matching_context).expect("serialize match context"),
    )
    .expect("write matching context");
    fs::write(
        matching_dir.join("result.json"),
        r#"{
  "status":"ok",
  "data_path":"demo.csv",
  "analysis_path":"analysis.yaml",
  "formula":"logit(disease ~ age)",
  "outcome":"disease",
  "predictors":["age"],
  "n_total":100,
  "n_used":96,
  "n_excluded_missing":4,
  "n_excluded_invalid":0,
  "n_events":24,
  "n_nonevents":72,
  "iterations":5,
  "converged":true,
  "log_likelihood":-48.12,
  "coefficients":[
    {
      "term":"Intercept",
      "variable":"Intercept",
      "beta":-2.1,
      "standard_error":0.8,
      "odds_ratio":0.1225,
      "ci_lower":0.025,
      "ci_upper":0.600,
      "p_value":0.01
    },
    {
      "term":"age",
      "variable":"age",
      "beta":0.08,
      "standard_error":0.03,
      "odds_ratio":1.0833,
      "ci_lower":1.02,
      "ci_upper":1.15,
      "p_value":0.008
    }
  ],
  "notes":["matching logistic"],
  "warnings":[]
}"#,
    )
    .expect("write matching result");

    fs::write(
        mismatched_dir.join("command.json"),
        r#"{"command":"model_logistic","request":{}}"#,
    )
    .expect("write mismatched command");
    fs::write(
        mismatched_dir.join("context.json"),
        serde_json::to_string_pretty(&mismatched_context).expect("serialize mismatch context"),
    )
    .expect("write mismatched context");
    fs::write(
        mismatched_dir.join("result.json"),
        r#"{
  "status":"ok",
  "data_path":"other.csv",
  "analysis_path":"analysis.yaml",
  "formula":"logit(disease ~ age)",
  "outcome":"disease",
  "predictors":["age"],
  "n_total":40,
  "n_used":40,
  "n_excluded_missing":0,
  "n_excluded_invalid":0,
  "n_events":30,
  "n_nonevents":10,
  "iterations":8,
  "converged":true,
  "log_likelihood":-10.00,
  "coefficients":[
    {
      "term":"Intercept",
      "variable":"Intercept",
      "beta":-0.5,
      "standard_error":0.5,
      "odds_ratio":0.6065,
      "ci_lower":0.22,
      "ci_upper":1.66,
      "p_value":0.32
    },
    {
      "term":"age",
      "variable":"age",
      "beta":1.5041,
      "standard_error":0.4,
      "odds_ratio":4.5000,
      "ci_lower":2.00,
      "ci_upper":9.00,
      "p_value":0.0001
    }
  ],
  "notes":["mismatched logistic"],
  "warnings":[]
}"#,
    )
    .expect("write mismatched result");

    let out_dir = root.join("artifacts");
    let cli = test_cli(Command::Report {
        command: ReportCommand::Build(ReportBuildArgs {
            analysis: analysis_path.clone(),
            out: Some(out_dir.clone()),
            artifacts: Some(artifacts_dir.clone()),
            include_exploratory: false,
        }),
    });

    crate::handlers::dispatch(&cli).expect("report build should filter mismatched evidence");
    let report_md =
        fs::read_to_string(out_dir.join("report").join("report.md")).expect("read report");
    let evidence_index = fs::read_to_string(out_dir.join("audit").join("evidence-index.json"))
        .expect("read evidence index");
    assert!(report_md.contains("age OR 1.08"));
    assert!(!report_md.contains("age OR 4.50"));
    assert!(evidence_index.contains("data_fingerprint"));
    assert!(evidence_index.contains("did not match the current analysis/data identity"));

    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn report_build_rejects_missing_required_study_context() {
    let root = temp_dir("report-missing-study-context");
    fs::create_dir_all(&root).expect("create root");
    let analysis_path = root.join("analysis.yaml");
    fs::write(
        &analysis_path,
        r"
study:
  title: Demo cohort
  design: cohort
data:
  path: demo.csv
  format: csv
variables:
  - name: disease
    kind: binary
    roles: [outcome]
analyses:
  - kind: table_one
    by: disease
",
    )
    .expect("write analysis yaml");
    fs::write(root.join("demo.csv"), "disease\n1\n0\n").expect("write csv");

    let cli = test_cli(Command::Report {
        command: ReportCommand::Build(ReportBuildArgs {
            analysis: analysis_path,
            out: Some(root.join("artifacts")),
            artifacts: None,
            include_exploratory: false,
        }),
    });

    let error = crate::handlers::dispatch(&cli).expect_err("report build should fail");
    assert!(error.contains("study_context"));
    assert!(error.contains("estimand"));
    assert!(error.contains("reporting_guideline"));
    assert!(error.contains("Suggested template"));
    assert!(error.contains("study_context:"));
    assert!(error.contains("outcome: \"disease\""));

    fs::remove_dir_all(root).expect("cleanup");
}
