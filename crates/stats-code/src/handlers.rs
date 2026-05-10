use std::fmt::Write;

use clap::Parser;
use serde_json::json;

mod analysis;
mod audit;
mod common;
mod data;
mod model;
mod project;
mod workflow;

pub(crate) use data::{handle_inspect, handle_rate, handle_tableone};
pub(crate) use model::{handle_model_cox, handle_model_linear, handle_model_logistic};
pub(crate) use workflow::handle_workflow_run;

use analysis::{handle_analysis_check, handle_analysis_plan};
use audit::{handle_audit_explain, handle_open_report};
use project::{handle_doctor, handle_init_project};

use crate::bridge::{self, Engine};
use crate::chat::run_chat_repl;
use crate::cli::{
    AiCommand, AuditCommand, AuthCommand, ChatArgs, Cli, Command, ConfigCommand, ModelCommand,
    OpenCommand, ReportCommand, ReportVerifyArgs, RunCommand, WorkflowCommand,
};
use crate::config::{
    handle_ai_ask, handle_auth_doctor, handle_auth_set, handle_config_add_model,
    handle_config_default_model, handle_config_remove_model, handle_config_show,
};
use crate::error::StatsCodeResult;
use crate::helpers::stringify_error;
use crate::render::{
    render_ai_ask_text, render_analysis_check_text, render_audit_explain_text,
    render_auth_doctor_text, render_auth_set_text, render_config_text, render_cox_text,
    render_doctor_text, render_init_project_text, render_inspect_text, render_linear_text,
    render_logistic_text, render_open_report_text, render_planned_text, render_rate_text,
    render_report_build_text, render_report_verify_text, render_tableone_text,
    render_workflow_run_text,
};
use crate::report::{
    handle_report_build, handle_report_verify, persist_run_artifacts_with_metadata,
};
use crate::schema::{
    AiAskResult, AnalysisCheckResult, ArtifactMetadata, ArtifactRole, ArtifactStatus,
    AuditExplainResult, AuthDoctorResult, AuthSetResult, ConfigResult, CoxResult, DoctorResult,
    InitProjectResult, InspectResult, LinearResult, LogisticResult, OpenReportResult,
    PlannedCommandResult, RateResult, ReportBuildResult, ReportVerifyResult, TableOneResult,
    WorkflowRunResult,
};
pub fn run() -> StatsCodeResult<()> {
    let cli = Cli::parse();
    match &cli.command {
        None => {
            let chat_args = ChatArgs::default();
            Ok(run_chat_repl(&cli, &chat_args)?)
        }
        Some(Command::Chat(args)) => Ok(run_chat_repl(&cli, args)?),
        Some(Command::Report {
            command: ReportCommand::Verify(args),
        }) => run_report_verify_command(&cli, args),
        Some(_) => {
            let rendered = dispatch(&cli)?;
            println!("{rendered}");
            Ok(())
        }
    }
}

fn run_report_verify_command(cli: &Cli, args: &ReportVerifyArgs) -> StatsCodeResult<()> {
    let result = handle_report_verify(args);
    let response = serde_json::to_value(&result).map_err(stringify_error)?;
    if let Some(base_dir) = &cli.artifacts_dir {
        let artifact = ArtifactMetadata::exploratory();
        persist_run_artifacts_with_metadata(
            base_dir,
            "report_verify",
            &json!(args),
            &response,
            Some(&artifact),
        )?;
    }

    if cli.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&response).map_err(stringify_error)?
        );
    } else {
        println!("{}", render_report_verify_text(&result));
    }

    match report_verify_exit_code(&result, args.fail_on_warning) {
        0 => Ok(()),
        code => std::process::exit(code),
    }
}

fn report_verify_exit_code(result: &ReportVerifyResult, fail_on_warning: bool) -> i32 {
    if result.has_errors() {
        1
    } else if fail_on_warning && result.warning_count > 0 {
        2
    } else {
        0
    }
}

pub fn dispatch(cli: &Cli) -> StatsCodeResult<String> {
    let Some(command) = &cli.command else {
        return Err(
            "Interactive chat mode is handled directly by `stats-code` without a subcommand."
                .into(),
        );
    };

    let (name, request, response) = match command {
        Command::Chat(_) => {
            return Err("Interactive chat mode is handled directly by `stats-code chat`.".into())
        }
        Command::Init(args) => {
            let result = handle_init_project(args)?;
            (
                "init",
                json!(args),
                serde_json::to_value(result).map_err(stringify_error)?,
            )
        }
        Command::Doctor(args) => {
            let result = handle_doctor(args);
            (
                "doctor",
                json!(args),
                serde_json::to_value(result).map_err(stringify_error)?,
            )
        }
        Command::Plan(args) => {
            let result = handle_analysis_plan(args)?;
            (
                "plan",
                json!(args),
                serde_json::to_value(result).map_err(stringify_error)?,
            )
        }
        Command::Config { command } => match command {
            ConfigCommand::Show => {
                let result = handle_config_show()?;
                (
                    "config",
                    json!({ "action": "show" }),
                    serde_json::to_value(result).map_err(stringify_error)?,
                )
            }
            ConfigCommand::DefaultModel(args) => {
                let result = handle_config_default_model(args)?;
                (
                    "config",
                    json!({ "action": "default_model", "model": args.model }),
                    serde_json::to_value(result).map_err(stringify_error)?,
                )
            }
            ConfigCommand::AddModel(args) => {
                let result = handle_config_add_model(args)?;
                (
                    "config",
                    json!({ "action": "add_model", "model": args.model }),
                    serde_json::to_value(result).map_err(stringify_error)?,
                )
            }
            ConfigCommand::RemoveModel(args) => {
                let result = handle_config_remove_model(args)?;
                (
                    "config",
                    json!({ "action": "remove_model", "model": args.model }),
                    serde_json::to_value(result).map_err(stringify_error)?,
                )
            }
        },
        Command::Check(args) => {
            let result = handle_analysis_check(args)?;
            (
                "analysis_check",
                json!(args),
                serde_json::to_value(result).map_err(stringify_error)?,
            )
        }
        Command::Inspect(args) => {
            let result = handle_inspect(args)?;
            (
                "inspect",
                json!(args),
                serde_json::to_value(result).map_err(stringify_error)?,
            )
        }
        Command::Tableone(args) => {
            let result = handle_tableone(args)?;
            (
                "tableone",
                json!(args),
                serde_json::to_value(result).map_err(stringify_error)?,
            )
        }
        Command::Rate(args) => {
            let result = handle_rate(args)?;
            (
                "rate",
                json!(args),
                serde_json::to_value(result).map_err(stringify_error)?,
            )
        }
        Command::Auth { command } => match command {
            AuthCommand::Set(args) => {
                let result = handle_auth_set(args)?;
                (
                    "auth_set",
                    json!({
                        "provider": args.provider,
                        "api_key": "<redacted>",
                        "base_url": args.base_url,
                    }),
                    serde_json::to_value(result).map_err(stringify_error)?,
                )
            }
            AuthCommand::Doctor(args) => {
                let result = handle_auth_doctor(args)?;
                (
                    "auth_doctor",
                    json!(args),
                    serde_json::to_value(result).map_err(stringify_error)?,
                )
            }
        },
        Command::Ai { command } => match command {
            AiCommand::Ask(args) => {
                let result = handle_ai_ask(args)?;
                (
                    "ai_ask",
                    json!(args),
                    serde_json::to_value(result).map_err(stringify_error)?,
                )
            }
        },
        Command::Audit { command } => match command {
            AuditCommand::Explain(args) => {
                let result = handle_audit_explain(args)?;
                (
                    "audit_explain",
                    json!(args),
                    serde_json::to_value(result).map_err(stringify_error)?,
                )
            }
        },
        Command::Open { command } => match command {
            OpenCommand::Report(args) => {
                let result = handle_open_report(args)?;
                (
                    "open_report",
                    json!(args),
                    serde_json::to_value(result).map_err(stringify_error)?,
                )
            }
        },
        Command::Model { command } => match command {
            ModelCommand::Logistic(args) => {
                let result = handle_model_logistic(args, cli.engine)?;
                (
                    "model_logistic",
                    json!(args),
                    serde_json::to_value(result).map_err(stringify_error)?,
                )
            }
            ModelCommand::Cox(args) => {
                let result = handle_model_cox(args, cli.engine)?;
                (
                    "model_cox",
                    json!(args),
                    serde_json::to_value(result).map_err(stringify_error)?,
                )
            }
            ModelCommand::Linear(args) => {
                let result = handle_model_linear(args, cli.engine)?;
                (
                    "model_linear",
                    json!(args),
                    serde_json::to_value(result).map_err(stringify_error)?,
                )
            }
        },
        Command::Report { command } => match command {
            ReportCommand::Build(args) => {
                let result = handle_report_build(args)?;
                (
                    "report_build",
                    json!(args),
                    serde_json::to_value(result).map_err(stringify_error)?,
                )
            }
            ReportCommand::Verify(args) => {
                let result = handle_report_verify(args);
                (
                    "report_verify",
                    json!(args),
                    serde_json::to_value(result).map_err(stringify_error)?,
                )
            }
        },
        Command::Workflow { command } => match command {
            WorkflowCommand::Run(args) => {
                let result = handle_workflow_run(args, cli.engine)?;
                (
                    "workflow_run",
                    json!(args),
                    serde_json::to_value(result).map_err(stringify_error)?,
                )
            }
        },
        Command::Run { command } => {
            let (engine, args) = match command {
                RunCommand::Python(args) => (Engine::Python, args),
                RunCommand::R(args) => (Engine::R, args),
            };
            let result = bridge::execute_custom_script(
                engine,
                &args.script,
                args.data.as_deref(),
                args.params.as_deref(),
            )?;

            let request_val = json!(args);
            let response_val = serde_json::to_value(&result).map_err(stringify_error)?;

            if let Some(base_dir) = &cli.artifacts_dir {
                let artifact = ArtifactMetadata::exploratory();
                persist_run_artifacts_with_metadata(
                    base_dir,
                    "run",
                    &request_val,
                    &response_val,
                    Some(&artifact),
                )?;
            }

            if cli.json {
                return Ok(serde_json::to_string_pretty(&response_val).map_err(stringify_error)?);
            }

            // Human-readable output
            let mut out = String::new();
            let _ = writeln!(out, "Run Script");
            let _ = writeln!(out, "  Engine           {}", result.engine);
            let _ = writeln!(out, "  Script           {}", result.script);
            let _ = writeln!(out, "  Exit code        {:?}", result.exit_code);
            if !result.stderr.trim().is_empty() {
                let _ = writeln!(out, "  Stderr");
                for line in result.stderr.lines().take(20) {
                    let _ = writeln!(out, "    {line}");
                }
            }
            if let Some(ref parsed) = result.parsed {
                let _ = writeln!(out, "  Output (JSON)");
                let pretty = serde_json::to_string_pretty(parsed).unwrap_or_default();
                for line in pretty.lines().take(50) {
                    let _ = writeln!(out, "    {line}");
                }
            } else if !result.stdout.trim().is_empty() {
                let _ = writeln!(out, "  Output (raw)");
                for line in result.stdout.lines().take(50) {
                    let _ = writeln!(out, "    {line}");
                }
            }
            return Ok(out);
        }
    };

    if let Some(base_dir) = &cli.artifacts_dir {
        let artifact = if name == "workflow_run" {
            response
                .get("run_id")
                .and_then(serde_json::Value::as_str)
                .map(|run_id| ArtifactMetadata {
                    role: ArtifactRole::Declared,
                    status: ArtifactStatus::Produced,
                    formal_run_id: Some(run_id.to_string()),
                    analysis_step_index: None,
                })
        } else {
            Some(ArtifactMetadata::exploratory())
        };
        persist_run_artifacts_with_metadata(
            base_dir,
            name,
            &request,
            &response,
            artifact.as_ref(),
        )?;
    }

    if cli.json {
        Ok(serde_json::to_string_pretty(&response).map_err(stringify_error)?)
    } else {
        match name {
            "inspect" => {
                let value: InspectResult =
                    serde_json::from_value(response).map_err(stringify_error)?;
                Ok(render_inspect_text(&value))
            }
            "tableone" => {
                let value: TableOneResult =
                    serde_json::from_value(response).map_err(stringify_error)?;
                Ok(render_tableone_text(&value))
            }
            "model_logistic" => {
                let value: LogisticResult =
                    serde_json::from_value(response).map_err(stringify_error)?;
                Ok(render_logistic_text(&value))
            }
            "model_cox" => {
                let value: CoxResult = serde_json::from_value(response).map_err(stringify_error)?;
                Ok(render_cox_text(&value))
            }
            "model_linear" => {
                let value: LinearResult =
                    serde_json::from_value(response).map_err(stringify_error)?;
                Ok(render_linear_text(&value))
            }
            "rate" => {
                let value: RateResult =
                    serde_json::from_value(response).map_err(stringify_error)?;
                Ok(render_rate_text(&value))
            }
            "auth_set" => {
                let value: AuthSetResult =
                    serde_json::from_value(response).map_err(stringify_error)?;
                Ok(render_auth_set_text(&value))
            }
            "auth_doctor" => {
                let value: AuthDoctorResult =
                    serde_json::from_value(response).map_err(stringify_error)?;
                Ok(render_auth_doctor_text(&value))
            }
            "ai_ask" => {
                let value: AiAskResult =
                    serde_json::from_value(response).map_err(stringify_error)?;
                Ok(render_ai_ask_text(&value))
            }
            "audit_explain" => {
                let value: AuditExplainResult =
                    serde_json::from_value(response).map_err(stringify_error)?;
                Ok(render_audit_explain_text(&value))
            }
            "open_report" => {
                let value: OpenReportResult =
                    serde_json::from_value(response).map_err(stringify_error)?;
                Ok(render_open_report_text(&value))
            }
            "config" => {
                let value: ConfigResult =
                    serde_json::from_value(response).map_err(stringify_error)?;
                Ok(render_config_text(&value))
            }
            "init" => {
                let value: InitProjectResult =
                    serde_json::from_value(response).map_err(stringify_error)?;
                Ok(render_init_project_text(&value))
            }
            "doctor" => {
                let value: DoctorResult =
                    serde_json::from_value(response).map_err(stringify_error)?;
                Ok(render_doctor_text(&value))
            }
            "analysis_check" => {
                let value: AnalysisCheckResult =
                    serde_json::from_value(response).map_err(stringify_error)?;
                Ok(render_analysis_check_text(&value))
            }
            "report_build" => {
                let value: ReportBuildResult =
                    serde_json::from_value(response).map_err(stringify_error)?;
                Ok(render_report_build_text(&value))
            }
            "report_verify" => {
                let value: ReportVerifyResult =
                    serde_json::from_value(response).map_err(stringify_error)?;
                Ok(render_report_verify_text(&value))
            }
            "workflow_run" => {
                let value: WorkflowRunResult =
                    serde_json::from_value(response).map_err(stringify_error)?;
                Ok(render_workflow_run_text(&value))
            }
            _ => {
                let value: PlannedCommandResult =
                    serde_json::from_value(response).map_err(stringify_error)?;
                Ok(render_planned_text(&value))
            }
        }
    }
}

// Report-related code has been moved to crate::report module.
// handle_report_build, discover_report_evidence, evidence types, etc.
// are now in src/report.rs

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use crate::cli::{
        AuditCommand, AuditExplainArgs, CheckArgs, Command, DoctorArgs, InitArgs, InspectArgs,
        ModelCommand, ModelCoxArgs, ModelLinearArgs, ModelLogisticArgs, OpenCommand,
        OpenReportArgs, PlanArgs, RateArgs, TableOneArgs, WorkflowCommand, WorkflowRunArgs,
    };
    use crate::schema::{
        detect_data_format, load_analysis_spec, AnalysisSpec, DataFormat, ReportVerifyResult,
    };

    use super::{dispatch, Cli};

    fn temp_dir(label: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time after epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("epistat-{label}-{nanos}"))
    }

    fn test_cli(command: Command) -> Cli {
        Cli {
            json: false,
            artifacts_dir: None,
            session: None,
            model: "gpt".to_string(),
            system: None,
            max_tokens: None,
            engine: crate::bridge::Engine::Rust,
            command: Some(command),
        }
    }

    fn report_verify_result(error_count: usize, warning_count: usize) -> ReportVerifyResult {
        ReportVerifyResult {
            status: if error_count == 0 { "ok" } else { "error" }.to_string(),
            artifacts_dir: "artifacts".to_string(),
            accepted_count: 0,
            rejected_count: 0,
            error_count,
            warning_count,
            items: Vec::new(),
            notes: Vec::new(),
        }
    }

    #[test]
    fn report_verify_exit_code_fails_on_errors_and_optional_warnings() {
        assert_eq!(
            super::report_verify_exit_code(&report_verify_result(0, 0), false),
            0
        );
        assert_eq!(
            super::report_verify_exit_code(&report_verify_result(1, 0), false),
            1
        );
        assert_eq!(
            super::report_verify_exit_code(&report_verify_result(0, 1), false),
            0
        );
        assert_eq!(
            super::report_verify_exit_code(&report_verify_result(0, 1), true),
            2
        );
    }

    #[test]
    fn init_project_writes_demo_contract_and_data() {
        let root = temp_dir("init-project");
        fs::create_dir_all(&root).expect("create root");
        let project_dir = root.join("demo-study");

        let cli = test_cli(Command::Init(InitArgs {
            project_dir: project_dir.clone(),
        }));
        let rendered = dispatch(&cli).expect("init should succeed");
        assert!(rendered.contains("Init Project"));
        assert!(project_dir.join("analysis.yaml").is_file());
        assert!(project_dir.join("data").join("demo_cohort.csv").is_file());
        assert!(project_dir
            .join("data")
            .join("demo_cohort.dictionary.csv")
            .is_file());
        assert!(project_dir.join("README.md").is_file());

        let check_cli = test_cli(Command::Check(CheckArgs {
            analysis: project_dir.join("analysis.yaml"),
        }));
        let check_rendered = dispatch(&check_cli).expect("initialized contract should validate");
        assert!(check_rendered.contains("Analysis Check"));
        assert!(check_rendered.contains("Status           ok"));

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn init_project_refuses_non_empty_directory() {
        let root = temp_dir("init-non-empty");
        let project_dir = root.join("demo-study");
        fs::create_dir_all(&project_dir).expect("create project");
        fs::write(project_dir.join("existing.txt"), "keep me").expect("write existing file");

        let cli = test_cli(Command::Init(InitArgs {
            project_dir: project_dir.clone(),
        }));
        let error = dispatch(&cli).expect_err("init should refuse non-empty target");
        assert!(error.contains("not empty"));
        assert!(project_dir.join("existing.txt").is_file());

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn doctor_reports_environment_status() {
        let cli = test_cli(Command::Doctor(DoctorArgs {}));
        let rendered = dispatch(&cli).expect("doctor should render");
        assert!(rendered.contains("Doctor"));
        assert!(rendered.contains("Version"));
        assert!(rendered.contains("current_dir_readable"));
        assert!(rendered.contains("analysis_template_found"));
    }

    #[test]
    fn plan_previews_declared_workflow_without_running_statistics() {
        let cli = test_cli(Command::Plan(PlanArgs {
            analysis: PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("examples")
                .join("analysis.example.yaml"),
            out: Some(PathBuf::from("planned-artifacts")),
            explore_out: Some(PathBuf::from("scratch-artifacts")),
            include_exploratory: false,
            strict: true,
            allow_warnings: false,
            allow_unenforced_survey: true,
            allow_unenforced_privacy: true,
        }));
        let rendered = dispatch(&cli).expect("plan should render");
        assert!(rendered.contains("Plan"));
        assert!(rendered.contains("workflow run"));
        assert!(rendered.contains("inspect_main"));
        assert!(rendered.contains("table1_by_disease"));
        assert!(rendered.contains("logistic_main"));
        assert!(rendered.contains("--strict"));
        assert!(rendered.contains("--allow-unenforced-survey"));
        assert!(rendered.contains("report.md"));
        assert!(rendered.contains("evidence-index.json"));
    }

    #[test]
    fn audit_explain_summarizes_evidence_index() {
        let root = temp_dir("audit-explain");
        let audit_dir = root.join("stats-code-artifacts").join("audit");
        fs::create_dir_all(&audit_dir).expect("create audit dir");
        fs::write(
            audit_dir.join("evidence-index.json"),
            r#"{
  "accepted_artifacts": [
    {
      "command": "inspect",
      "status": "accepted",
      "report_decision": "accepted",
      "matched_analysis_step_index": 0,
      "reason": "matched current analysis/data identity and declared analysis step",
      "result_path": "inspect/result.json",
      "context_path": "inspect/context.json"
    }
  ],
  "rejected_artifacts": [
    {
      "command": "model_logistic",
      "status": "rejected",
      "report_decision": "rejected",
      "matched_analysis_step_index": 3,
      "reason": "artifact has blocking diagnostics: possible_complete_separation"
    }
  ],
  "policy_exceptions": [
    {
      "code": "unsupported_survey_design",
      "message": "survey metadata was declared but explicitly allowed without enforcement"
    }
  ]
}"#,
        )
        .expect("write evidence index");

        let cli = test_cli(Command::Audit {
            command: AuditCommand::Explain(AuditExplainArgs {
                artifacts: root.join("stats-code-artifacts"),
            }),
        });
        let rendered = dispatch(&cli).expect("audit explain should render");
        assert!(rendered.contains("Audit Explain"));
        assert!(rendered.contains("accepted=1 rejected=1 policy_exceptions=1"));
        assert!(rendered.contains("inspect status=accepted"));
        assert!(rendered.contains("model_logistic status=rejected"));
        assert!(rendered.contains("possible_complete_separation"));
        assert!(rendered.contains("survey metadata was declared"));

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn open_report_print_only_returns_report_path() {
        let root = temp_dir("open-report");
        let report_dir = root.join("stats-code-artifacts").join("report");
        fs::create_dir_all(&report_dir).expect("create report dir");
        fs::write(report_dir.join("report.md"), "# Demo Report\n").expect("write report");

        let cli = test_cli(Command::Open {
            command: OpenCommand::Report(OpenReportArgs {
                artifacts: root.join("stats-code-artifacts"),
                print_only: true,
            }),
        });
        let rendered = dispatch(&cli).expect("open report should render");
        assert!(rendered.contains("Open Report"));
        assert!(rendered.contains("report.md"));
        assert!(rendered.contains("Opened           false"));
        assert!(rendered.contains("--print-only"));

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn open_report_rejects_missing_report_markdown() {
        let root = temp_dir("open-report-missing");
        let artifacts_dir = root.join("stats-code-artifacts");
        fs::create_dir_all(&artifacts_dir).expect("create artifacts dir");

        let cli = test_cli(Command::Open {
            command: OpenCommand::Report(OpenReportArgs {
                artifacts: artifacts_dir,
                print_only: true,
            }),
        });
        let error = dispatch(&cli).expect_err("open report should require report.md");
        assert!(error.contains("report.md"));
        assert!(error.contains("workflow run"));

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn detects_known_data_formats() {
        assert_eq!(
            detect_data_format(std::path::Path::new("a.csv")),
            DataFormat::Csv
        );
        assert_eq!(
            detect_data_format(std::path::Path::new("a.xlsx")),
            DataFormat::Excel
        );
        assert_eq!(
            detect_data_format(std::path::Path::new("a.parquet")),
            DataFormat::Parquet
        );
        assert_eq!(
            detect_data_format(std::path::Path::new("a.xpt")),
            DataFormat::Xpt
        );
    }

    #[test]
    fn inspect_csv_summarizes_missingness_and_types() {
        let root = temp_dir("inspect");
        fs::create_dir_all(&root).expect("create root");
        let csv_path = root.join("demo.csv");
        fs::write(
            &csv_path,
            "id,age,smoke,fu_time\n1,42,1,12\n2,38,0,8\n3,,1,NA\n",
        )
        .expect("write csv");

        let mut cli = test_cli(Command::Inspect(InspectArgs {
            data_path: csv_path.clone(),
        }));
        cli.json = true;

        let rendered = dispatch(&cli).expect("inspect should succeed");
        assert!(rendered.contains("\"rows\": 3"));
        assert!(rendered.contains("\"missing_count\": 1"));

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn rate_with_analysis_rejects_missing_required_study_context() {
        let root = temp_dir("rate-study-context");
        fs::create_dir_all(&root).expect("create root");
        let analysis_path = root.join("analysis.yaml");
        let csv_path = root.join("demo.csv");
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
  - name: case
    kind: binary
    roles: [outcome]
analyses:
  - kind: rate
    event: case
    person_time: fu_pt
",
        )
        .expect("write analysis yaml");
        fs::write(&csv_path, "case,fu_pt\n1,2.0\n0,1.0\n").expect("write csv");

        let cli = test_cli(Command::Rate(RateArgs {
            data: None,
            analysis: Some(analysis_path),
            event: "case".to_string(),
            person_time: "fu_pt".to_string(),
            strata: Vec::new(),
        }));

        let error = dispatch(&cli).expect_err("rate should fail");
        assert!(error.contains("study_context"));
        assert!(error.contains("time_zero"));
        assert!(error.contains("follow_up"));
        assert!(error.contains("censoring"));
        assert!(error.contains("study_context:"));
        assert!(error.contains("outcome: \"case\""));

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn rate_csv_calculates_grouped_person_time_rates() {
        let root = temp_dir("rate");
        fs::create_dir_all(&root).expect("create root");
        let csv_path = root.join("demo.csv");
        fs::write(
            &csv_path,
            "case,fu_pt,sex\n1,2.0,female\n0,1.0,female\n1,4.0,male\n1,,male\n",
        )
        .expect("write csv");

        let cli = test_cli(Command::Rate(RateArgs {
            data: Some(csv_path.clone()),
            analysis: None,
            event: "case".to_string(),
            person_time: "fu_pt".to_string(),
            strata: vec!["sex".to_string()],
        }));

        let rendered = dispatch(&cli).expect("rate should succeed");
        assert!(rendered.contains("sex=female"));
        assert!(rendered.contains("per_1000=333.333"));
        assert!(rendered.contains("sex=male"));

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn tableone_csv_summarizes_continuous_and_categorical_variables() {
        let root = temp_dir("tableone");
        fs::create_dir_all(&root).expect("create root");
        let csv_path = root.join("demo.csv");
        fs::write(
            &csv_path,
            "outcome,age,sex,smoke\n0,40,female,never\n1,52,male,current\n0,38,female,former\n1,47,male,current\n1,,female,never\n",
        )
        .expect("write csv");

        let cli = test_cli(Command::Tableone(TableOneArgs {
            data: Some(csv_path.clone()),
            analysis: None,
            by: "outcome".to_string(),
            vars: vec!["age".to_string(), "sex".to_string(), "smoke".to_string()],
        }));

        let rendered = dispatch(&cli).expect("tableone should succeed");
        assert!(rendered.contains("Table 1"));
        assert!(rendered.contains("age [continuous]"));
        assert!(rendered.contains("sex = female"));
        assert!(rendered.contains("smoke = current"));

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn workflow_run_executes_declared_analysis_and_builds_report() {
        let root = temp_dir("workflow-run");
        fs::create_dir_all(&root).expect("create root");
        let analysis_path = root.join("analysis.yaml");
        fs::write(
            &analysis_path,
            r"
schema_version: stats-code.v0
study:
  title: Workflow cohort
  design: cross-sectional
study_context:
  estimand: Descriptive category comparison
  exposure: Category
  comparator: Other category
  outcome: Data value
  missing_data_strategy: Complete-case descriptive summaries
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
  - id: inspect_main
    kind: inspect
  - id: table1_main
    kind: table_one
    by: category
report:
  out_dir: formal-artifacts
  include_methods: true
  include_tables: true
  include_assumptions: true
",
        )
        .expect("write analysis yaml");
        fs::write(
            root.join("demo.csv"),
            "category,data_value\nA,1.0\nB,2.0\nA,3.0\nB,4.0\n",
        )
        .expect("write csv");

        let out_dir = root.join("formal-artifacts");
        let cli = test_cli(Command::Workflow {
            command: WorkflowCommand::Run(WorkflowRunArgs {
                analysis: analysis_path,
                out: Some(out_dir.clone()),
                explore_out: Some(root.join("explore-artifacts")),
                include_exploratory: false,
                strict: false,
                allow_warnings: false,
                allow_unenforced_survey: false,
                allow_unenforced_privacy: false,
                no_chat: true,
            }),
        });

        let rendered = dispatch(&cli).expect("workflow run should succeed");
        assert!(rendered.contains("Workflow Run"));
        assert!(rendered.contains("inspect"));
        assert!(rendered.contains("tableone"));
        assert!(out_dir.join("report").join("report.md").is_file());
        assert!(out_dir.join("tables").join("tableone.md").is_file());

        let evidence_index = fs::read_to_string(out_dir.join("audit").join("evidence-index.json"))
            .expect("read evidence index");
        assert!(evidence_index.contains("\"accepted_artifacts\""));
        assert!(evidence_index.contains("\"formal_run_id\""));
        assert!(evidence_index.contains("\"analysis_step_index\": 1"));
        assert!(evidence_index.contains("\"role\": \"declared\""));

        let report_md =
            fs::read_to_string(out_dir.join("report").join("report.md")).expect("read report");
        assert!(report_md.contains("Table 1 available for `category`"));

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn workflow_run_strict_blocks_unenforced_policy_metadata() {
        let root = temp_dir("workflow-strict-policy-block");
        fs::create_dir_all(&root).expect("create root");
        let analysis_path = root.join("analysis.yaml");
        fs::write(&analysis_path, strict_policy_analysis_yaml()).expect("write analysis yaml");
        fs::write(
            root.join("demo.csv"),
            "participant_id,category,survey_weight,site\n1,A,1.2,S1\n2,B,0.8,S2\n",
        )
        .expect("write csv");

        let cli = test_cli(Command::Workflow {
            command: WorkflowCommand::Run(WorkflowRunArgs {
                analysis: analysis_path,
                out: Some(root.join("formal-artifacts")),
                explore_out: None,
                include_exploratory: false,
                strict: true,
                allow_warnings: false,
                allow_unenforced_survey: false,
                allow_unenforced_privacy: false,
                no_chat: true,
            }),
        });

        let error = dispatch(&cli).expect_err("strict workflow should block unenforced survey");
        assert!(error.contains("complex survey variance metadata was declared"));
        assert!(error.contains("--allow-unenforced-survey"));

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn workflow_run_strict_records_allowed_policy_exceptions() {
        let root = temp_dir("workflow-strict-policy-allow");
        fs::create_dir_all(&root).expect("create root");
        let analysis_path = root.join("analysis.yaml");
        fs::write(&analysis_path, strict_policy_analysis_yaml()).expect("write analysis yaml");
        fs::write(
            root.join("demo.csv"),
            "participant_id,category,survey_weight,site\n1,A,1.2,S1\n2,B,0.8,S2\n",
        )
        .expect("write csv");

        let out_dir = root.join("formal-artifacts");
        let cli = test_cli(Command::Workflow {
            command: WorkflowCommand::Run(WorkflowRunArgs {
                analysis: analysis_path,
                out: Some(out_dir.clone()),
                explore_out: None,
                include_exploratory: false,
                strict: true,
                allow_warnings: false,
                allow_unenforced_survey: true,
                allow_unenforced_privacy: true,
                no_chat: true,
            }),
        });

        let rendered = dispatch(&cli).expect("strict workflow should allow explicit exceptions");
        assert!(rendered.contains("Strict workflow policy was evaluated"));
        assert!(rendered.contains("Policy exception"));

        let evidence_index = fs::read_to_string(out_dir.join("audit").join("evidence-index.json"))
            .expect("read evidence index");
        assert!(evidence_index.contains("\"policy_exceptions\""));
        assert!(evidence_index.contains("unsupported_complex_survey_variance"));
        assert!(evidence_index.contains("unenforced_privacy_policy"));

        let report_md =
            fs::read_to_string(out_dir.join("report").join("report.md")).expect("read report");
        assert!(report_md.contains("## Policy Exceptions"));
        assert!(report_md.contains("Complex survey variance metadata was declared"));
        assert!(report_md.contains("Privacy metadata requiring de-identification"));

        fs::remove_dir_all(root).expect("cleanup");
    }

    fn strict_policy_analysis_yaml() -> &'static str {
        r"
schema_version: stats-code.v0
study:
  title: Strict policy cohort
  design: cross-sectional
study_context:
  estimand: Descriptive category comparison
  exposure: Category
  comparator: Other category
  outcome: Category distribution
  missing_data_strategy: Complete-case descriptive summaries
  clustering: site
  reporting_guideline: STROBE
data:
  path: demo.csv
  format: csv
variables:
  - name: participant_id
    kind: identifier
    roles: [id]
  - name: category
    kind: categorical
    roles: [outcome]
  - name: survey_weight
    kind: continuous
    roles: [weight]
  - name: site
    kind: categorical
    roles: [cluster]
survey:
  weight: survey_weight
  cluster: site
privacy:
  deidentify: true
  direct_identifiers: [participant_id]
  quasi_identifiers: [site]
  small_cell_threshold: 5
analyses:
  - id: inspect_main
    kind: inspect
report:
  out_dir: formal-artifacts
  include_methods: true
  include_tables: true
  include_assumptions: true
"
    }

    #[test]
    fn analysis_check_accepts_example_contract_with_policy_warnings() {
        let analysis_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("examples")
            .join("analysis.example.yaml");
        let rendered = dispatch(&test_cli(Command::Check(crate::cli::CheckArgs {
            analysis: analysis_path,
        })))
        .expect("check should render");

        assert!(rendered.contains("Analysis Check"));
        assert!(rendered.contains("Status           ok"));
        assert!(rendered.contains("OK survey_weight_supported"));
        assert!(rendered.contains("WARNING complex_survey_variance_unenforced"));
        assert!(rendered.contains("OK small_cell_suppression_supported"));
        assert!(rendered.contains("WARNING privacy_deidentification_unenforced"));
        assert!(rendered.contains("OK analysis_id_present"));
    }

    #[test]
    fn analysis_check_reports_missing_id_and_invalid_binary_outcome() {
        let root = temp_dir("analysis-check-invalid");
        fs::create_dir_all(&root).expect("create root");
        let analysis_path = root.join("analysis.yaml");
        fs::write(
            &analysis_path,
            r"
schema_version: stats-code.v0
study:
  title: Invalid check
  design: cross-sectional
study_context:
  estimand: Odds ratio
  outcome: Outcome
  missing_data_strategy: Complete case
  reporting_guideline: STROBE
data:
  path: demo.csv
  format: csv
variables:
  - name: outcome
    kind: binary
    roles: [outcome]
  - name: age
    kind: continuous
    roles: [covariate]
analyses:
  - kind: model
    model: logistic
    outcome: outcome
    predictors: [age]
",
        )
        .expect("write analysis");
        fs::write(root.join("demo.csv"), "outcome,age\n0,40\n1,50\n2,60\n").expect("write csv");

        let rendered = dispatch(&test_cli(Command::Check(crate::cli::CheckArgs {
            analysis: analysis_path,
        })))
        .expect("check should render errors");
        assert!(rendered.contains("Status           error"));
        assert!(rendered.contains("ERROR analysis_id_missing"));
        assert!(rendered.contains("ERROR binary_levels_invalid"));

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn logistic_csv_fits_basic_model_with_categorical_expansion() {
        let root = temp_dir("logistic");
        fs::create_dir_all(&root).expect("create root");
        let csv_path = root.join("demo.csv");
        fs::write(
            &csv_path,
            "case,age,sex,smoke\n0,30,female,never\n0,32,female,never\n0,35,male,former\n1,50,male,current\n1,55,male,current\n1,48,female,current\n0,28,female,never\n1,60,male,current\n0,40,female,former\n1,52,male,current\n",
        )
        .expect("write csv");

        let cli = test_cli(Command::Model {
            command: ModelCommand::Logistic(ModelLogisticArgs {
                data: Some(csv_path.clone()),
                analysis: None,
                outcome: "case".to_string(),
                predictors: vec!["age".to_string(), "smoke".to_string()],
                adjust: vec!["sex".to_string()],
                strata: Vec::new(),
            }),
        });

        let rendered = dispatch(&cli).expect("logistic should succeed");
        assert!(rendered.contains("Logistic Model"));
        assert!(rendered.contains("Intercept"));
        assert!(rendered.contains("age"));
        assert!(rendered.contains("smoke["));

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn cox_csv_fits_basic_model_with_time_to_event_data() {
        let root = temp_dir("cox");
        fs::create_dir_all(&root).expect("create root");
        let csv_path = root.join("demo.csv");
        fs::write(
            &csv_path,
            "fu_time,death,age,sex,smoke\n5,1,70,male,current\n8,1,66,male,current\n12,0,58,female,former\n4,1,72,male,current\n15,0,55,female,never\n10,1,63,male,former\n18,0,50,female,never\n7,1,68,male,current\n14,0,57,female,former\n9,1,65,male,current\n",
        )
        .expect("write csv");

        let cli = test_cli(Command::Model {
            command: ModelCommand::Cox(ModelCoxArgs {
                data: Some(csv_path.clone()),
                analysis: None,
                time: "fu_time".to_string(),
                event: "death".to_string(),
                predictors: vec!["age".to_string(), "smoke".to_string()],
                adjust: vec!["sex".to_string()],
                strata: Vec::new(),
            }),
        });

        let rendered = dispatch(&cli).expect("cox should succeed");
        assert!(rendered.contains("Cox Model"));
        assert!(rendered.contains("age"));
        assert!(rendered.contains("smoke["));
        assert!(rendered.contains("HR="));

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn loads_example_analysis_yaml() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("examples")
            .join("analysis.example.yaml");
        let spec: AnalysisSpec = load_analysis_spec(&path).expect("example should parse");
        assert_eq!(spec.study.design, "cohort");
        assert_eq!(spec.analyses.len(), 5);
    }

    #[test]
    fn linear_csv_fits_basic_ols_model() {
        let root = temp_dir("linear");
        fs::create_dir_all(&root).expect("create root");
        let csv_path = root.join("demo.csv");
        // y = 2 + 3*x + noise
        fs::write(
            &csv_path,
            "y,x,group\n5.1,1,A\n8.0,2,B\n10.9,3,A\n14.1,4,B\n17.0,5,A\n20.2,6,B\n22.8,7,A\n26.0,8,B\n29.1,9,A\n31.9,10,B\n",
        )
        .expect("write csv");

        let cli = test_cli(Command::Model {
            command: ModelCommand::Linear(ModelLinearArgs {
                data: Some(csv_path.clone()),
                analysis: None,
                outcome: "y".to_string(),
                predictors: vec!["x".to_string()],
                adjust: vec![],
                strata: Vec::new(),
            }),
        });

        let rendered = dispatch(&cli).expect("linear should succeed");
        assert!(rendered.contains("Linear Model"));
        assert!(rendered.contains("Intercept"));
        assert!(rendered.contains('x'));

        // Verify R² is very high (near perfect linear data)
        assert!(rendered.contains("R²="));

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn linear_ols_known_values() {
        // Simple dataset: y = 1 + 2*x exactly → β₀=1, β₁=2, R²=1
        let root = temp_dir("linear_known");
        fs::create_dir_all(&root).expect("create root");
        let csv_path = root.join("exact.csv");
        // Use float x so it's treated as continuous, and enough rows
        fs::write(
            &csv_path,
            "y,x\n3.0,1.0\n5.0,2.0\n7.0,3.0\n9.0,4.0\n11.0,5.0\n13.0,6.0\n15.0,7.0\n17.0,8.0\n19.0,9.0\n21.0,10.0",
        )
        .expect("write csv");
        let args = crate::cli::ModelLinearArgs {
            data: Some(csv_path.clone()),
            analysis: None,
            outcome: "y".to_string(),
            predictors: vec!["x".to_string()],
            adjust: vec![],
            strata: Vec::new(),
        };
        let result = super::handle_model_linear(&args, crate::bridge::Engine::Rust)
            .expect("linear should succeed");
        assert_eq!(result.status, "ok");
        assert_eq!(result.n_used, 10);
        assert!(
            (result.r_squared - 1.0).abs() < 1e-8,
            "perfect R²: got {}",
            result.r_squared
        );
        assert_eq!(result.coefficients.len(), 2);
        // Intercept → 1.0
        let intercept = &result.coefficients[0];
        assert!(
            (intercept.beta - 1.0).abs() < 1e-8,
            "intercept: got {}",
            intercept.beta
        );
        // Slope → 2.0
        let slope = &result.coefficients[1];
        assert!((slope.beta - 2.0).abs() < 1e-8, "slope: got {}", slope.beta);

        fs::remove_dir_all(root).expect("cleanup");
    }
}
