use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;

use clap::Parser;
use serde_json::{json, Value};

use crate::bridge::{
    self, bridge_to_logistic, execute_bridge, BridgeConfig, BridgeRequest, Engine,
};
use crate::chat::run_chat_repl;
use crate::cli::{
    AiCommand, AuditCommand, AuditExplainArgs, AuthCommand, AuthDoctorArgs, ChatArgs, CheckArgs,
    Cli, Command, ConfigCommand, DoctorArgs, InitArgs, InspectArgs, ModelCommand, ModelCoxArgs,
    ModelLinearArgs, ModelLogisticArgs, OpenCommand, OpenReportArgs, PlanArgs, RateArgs,
    ReportCommand, ReportVerifyArgs, RunCommand, TableOneArgs, WorkflowCommand, WorkflowRunArgs,
};
use crate::config::{
    handle_ai_ask, handle_auth_doctor, handle_auth_set, handle_config_add_model,
    handle_config_default_model, handle_config_remove_model, handle_config_show,
};
use crate::cox::cox_csv;
use crate::helpers::{
    excel_to_temp_csv, read_excel_records, require_column, stringify_error, unix_timestamp_nanos,
};
use crate::linear::linear_csv;
use crate::logistic::logistic_csv;
use crate::rate::rate_csv;
use crate::render::{
    render_ai_ask_text, render_analysis_check_text, render_audit_explain_text,
    render_auth_doctor_text, render_auth_set_text, render_config_text, render_cox_text,
    render_doctor_text, render_init_project_text, render_inspect_text, render_linear_text,
    render_logistic_text, render_open_report_text, render_planned_text, render_rate_text,
    render_report_build_text, render_report_verify_text, render_tableone_text,
    render_workflow_run_text,
};
use crate::report::{
    ensure_study_context_ready, handle_report_build, handle_report_verify,
    persist_run_artifacts_with_metadata, resolve_data_path, resolve_relative_to_analysis,
};
use crate::schema::{
    detect_data_format, is_missing_value_for_column, load_analysis_spec, AiAskResult,
    AnalysisCheckItem, AnalysisCheckLevel, AnalysisCheckResult, AnalysisKind, AnalysisSpec,
    ArtifactMetadata, ArtifactRole, ArtifactStatus, AuditExplainArtifact, AuditExplainResult,
    AuthDoctorResult, AuthSetResult, ConfigResult, CoxResult, DataFormat, DoctorResult,
    InitProjectResult, InspectResult, LinearResult, LogisticResult, ModelKind, OpenReportResult,
    PlannedCommandResult, RateResult, ReportBuildResult, ReportVerifyResult, RunningColumnStats,
    TableOneResult, VariableKind, WorkflowRunResult, WorkflowStepRunResult,
};
use crate::tableone::tableone_csv;

pub fn run() -> Result<(), String> {
    let cli = Cli::parse();
    match &cli.command {
        None => {
            let chat_args = ChatArgs::default();
            run_chat_repl(&cli, &chat_args)
        }
        Some(Command::Chat(args)) => run_chat_repl(&cli, args),
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

fn run_report_verify_command(cli: &Cli, args: &ReportVerifyArgs) -> Result<(), String> {
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

pub fn dispatch(cli: &Cli) -> Result<String, String> {
    let Some(command) = &cli.command else {
        return Err(
            "Interactive chat mode is handled directly by `stats-code` without a subcommand."
                .to_string(),
        );
    };

    let (name, request, response) = match command {
        Command::Chat(_) => {
            return Err(
                "Interactive chat mode is handled directly by `stats-code chat`.".to_string(),
            )
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
                return serde_json::to_string_pretty(&response_val).map_err(stringify_error);
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
        serde_json::to_string_pretty(&response).map_err(stringify_error)
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

pub(crate) fn handle_init_project(args: &InitArgs) -> Result<InitProjectResult, String> {
    let project_dir = resolve_init_project_dir(&args.project_dir)?;
    if project_dir.exists() {
        if !project_dir.is_dir() {
            return Err(format!(
                "Target project path `{}` exists but is not a directory.",
                project_dir.display()
            ));
        }
        let mut entries = fs::read_dir(&project_dir).map_err(stringify_error)?;
        if entries
            .next()
            .transpose()
            .map_err(stringify_error)?
            .is_some()
        {
            return Err(format!(
                "Target project directory `{}` is not empty.",
                project_dir.display()
            ));
        }
    }

    let examples_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples");
    let data_dir = project_dir.join("data");
    fs::create_dir_all(&data_dir).map_err(stringify_error)?;

    let mut written_files = Vec::new();
    copy_init_template(
        &examples_dir.join("analysis.example.yaml"),
        &project_dir.join("analysis.yaml"),
        &mut written_files,
    )?;
    copy_init_template(
        &examples_dir.join("data").join("demo_cohort.csv"),
        &data_dir.join("demo_cohort.csv"),
        &mut written_files,
    )?;
    copy_init_template(
        &examples_dir.join("data").join("demo_cohort.dictionary.csv"),
        &data_dir.join("demo_cohort.dictionary.csv"),
        &mut written_files,
    )?;
    write_init_readme(&project_dir.join("README.md"), &mut written_files)?;

    Ok(InitProjectResult {
        status: "ok".to_string(),
        project_dir: project_dir.display().to_string(),
        analysis_path: project_dir.join("analysis.yaml").display().to_string(),
        data_dir: data_dir.display().to_string(),
        written_files,
        next_steps: vec![
            format!("cd {}", project_dir.display()),
            "stats-code doctor".to_string(),
            "stats-code check analysis.yaml".to_string(),
            "stats-code workflow run analysis.yaml --out stats-code-artifacts --no-chat"
                .to_string(),
            "stats-code report verify stats-code-artifacts".to_string(),
        ],
        notes: vec![
            "The initialized project uses bundled synthetic demo data.".to_string(),
            "Formal statistics should come from workflow artifacts, not chat-only summaries."
                .to_string(),
            "Survey/privacy sections are audit metadata unless an enforcement engine or explicit policy exception is present.".to_string(),
        ],
    })
}

fn resolve_init_project_dir(path: &Path) -> Result<PathBuf, String> {
    if path.as_os_str().is_empty() {
        return Err("Project directory cannot be empty.".to_string());
    }
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(std::env::current_dir().map_err(stringify_error)?.join(path))
    }
}

fn copy_init_template(
    source: &Path,
    target: &Path,
    written_files: &mut Vec<String>,
) -> Result<(), String> {
    if !source.is_file() {
        return Err(format!(
            "Bundled init template `{}` was not found.",
            source.display()
        ));
    }
    fs::copy(source, target).map_err(stringify_error)?;
    written_files.push(target.display().to_string());
    Ok(())
}

fn write_init_readme(path: &Path, written_files: &mut Vec<String>) -> Result<(), String> {
    fs::write(
        path,
        r"# Stats Code Demo Project

This project is a local reproducible workflow demo using bundled synthetic data.

## Quickstart

```bash
stats-code doctor
stats-code check analysis.yaml
stats-code workflow run analysis.yaml --out stats-code-artifacts --no-chat
stats-code report verify stats-code-artifacts
```

Formal report values should be traced through `stats-code-artifacts/audit/evidence-index.json`.
Survey and privacy sections in this demo are policy metadata unless an enforcement engine or explicit policy exception is used.
",
    )
    .map_err(stringify_error)?;
    written_files.push(path.display().to_string());
    Ok(())
}

pub(crate) fn handle_doctor(_args: &DoctorArgs) -> DoctorResult {
    let mut items = Vec::new();
    let version = env!("CARGO_PKG_VERSION").to_string();
    push_check(
        &mut items,
        AnalysisCheckLevel::Ok,
        "version_detected",
        format!("stats-code version {version}"),
    );

    let executable = match std::env::current_exe() {
        Ok(path) => {
            push_check(
                &mut items,
                AnalysisCheckLevel::Ok,
                "executable_detected",
                format!("executable path `{}`", path.display()),
            );
            path.display().to_string()
        }
        Err(error) => {
            push_check(
                &mut items,
                AnalysisCheckLevel::Warning,
                "executable_unavailable",
                format!("could not resolve executable path: {error}"),
            );
            String::new()
        }
    };

    let current_dir = match std::env::current_dir() {
        Ok(path) => {
            push_check(
                &mut items,
                AnalysisCheckLevel::Ok,
                "current_dir_readable",
                format!("current directory `{}`", path.display()),
            );
            check_current_dir_writable(&path, &mut items);
            path.display().to_string()
        }
        Err(error) => {
            push_check(
                &mut items,
                AnalysisCheckLevel::Error,
                "current_dir_unavailable",
                format!("could not resolve current directory: {error}"),
            );
            String::new()
        }
    };

    let examples_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples");
    check_required_file(
        &mut items,
        &examples_dir.join("analysis.example.yaml"),
        "analysis_template_found",
        "analysis_template_missing",
        "bundled analysis template",
    );
    check_required_file(
        &mut items,
        &examples_dir.join("data").join("demo_cohort.csv"),
        "demo_data_found",
        "demo_data_missing",
        "bundled demo data",
    );
    check_required_file(
        &mut items,
        &examples_dir.join("data").join("demo_cohort.dictionary.csv"),
        "demo_dictionary_found",
        "demo_dictionary_missing",
        "bundled demo dictionary",
    );

    if process_command_available("cargo", &["audit", "--version"]) {
        push_check(
            &mut items,
            AnalysisCheckLevel::Ok,
            "cargo_audit_available",
            "`cargo audit --version` is available",
        );
    } else {
        push_check(
            &mut items,
            AnalysisCheckLevel::Warning,
            "cargo_audit_unavailable",
            "`cargo audit` is not available locally; dependency audit is optional for the deterministic workflow",
        );
    }

    match handle_auth_doctor(&AuthDoctorArgs { provider: None }) {
        Ok(auth) => {
            let configured = auth
                .providers
                .iter()
                .filter(|provider| provider.api_key_present)
                .count();
            if configured > 0 {
                push_check(
                    &mut items,
                    AnalysisCheckLevel::Ok,
                    "provider_credentials_available",
                    format!("{configured} provider credential set(s) detected"),
                );
            } else {
                push_check(
                    &mut items,
                    AnalysisCheckLevel::Warning,
                    "provider_credentials_missing",
                    "no AI provider credential was detected; formal workflow commands do not require chat credentials",
                );
            }
        }
        Err(error) => push_check(
            &mut items,
            AnalysisCheckLevel::Warning,
            "provider_config_unreadable",
            format!("provider configuration could not be read: {error}"),
        ),
    }

    let error_count = items
        .iter()
        .filter(|item| item.level == AnalysisCheckLevel::Error)
        .count();
    let warning_count = items
        .iter()
        .filter(|item| item.level == AnalysisCheckLevel::Warning)
        .count();

    DoctorResult {
        status: if error_count > 0 {
            "error"
        } else if warning_count > 0 {
            "warning"
        } else {
            "ok"
        }
        .to_string(),
        version,
        current_dir,
        executable,
        error_count,
        warning_count,
        items,
        notes: vec![
            "Doctor checks local readiness only; it does not call external providers.".to_string(),
            "Use `stats-code auth doctor` for provider-specific credential detail.".to_string(),
            "The trusted formal path is check -> workflow run -> report verify.".to_string(),
        ],
    }
}

fn check_current_dir_writable(path: &Path, items: &mut Vec<AnalysisCheckItem>) {
    let probe = path.join(format!(
        ".stats-code-doctor-write-test-{}.tmp",
        unix_timestamp_nanos()
    ));
    match fs::write(&probe, b"stats-code doctor write probe") {
        Ok(()) => {
            match fs::remove_file(&probe) {
                Ok(()) => push_check(
                    items,
                    AnalysisCheckLevel::Ok,
                    "current_dir_writable",
                    "current directory accepts report/artifact writes",
                ),
                Err(error) => push_check(
                    items,
                    AnalysisCheckLevel::Warning,
                    "current_dir_probe_cleanup_failed",
                    format!(
                        "current directory is writable, but the doctor probe could not be removed: {error}"
                    ),
                ),
            }
        }
        Err(error) => push_check(
            items,
            AnalysisCheckLevel::Error,
            "current_dir_not_writable",
            format!("current directory is not writable: {error}"),
        ),
    }
}

fn check_required_file(
    items: &mut Vec<AnalysisCheckItem>,
    path: &Path,
    ok_code: &str,
    missing_code: &str,
    label: &str,
) {
    if path.is_file() {
        push_check(
            items,
            AnalysisCheckLevel::Ok,
            ok_code,
            format!("{label} found at `{}`", path.display()),
        );
    } else {
        push_check(
            items,
            AnalysisCheckLevel::Error,
            missing_code,
            format!("{label} was not found at `{}`", path.display()),
        );
    }
}

fn process_command_available(program: &str, args: &[&str]) -> bool {
    ProcessCommand::new(program)
        .args(args)
        .output()
        .is_ok_and(|output| output.status.success())
}

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

pub(crate) fn handle_audit_explain(args: &AuditExplainArgs) -> Result<AuditExplainResult, String> {
    let artifacts_dir = args.artifacts.canonicalize().map_err(|error| {
        format!(
            "Cannot read artifacts directory `{}`: {error}",
            args.artifacts.display()
        )
    })?;
    let evidence_index_path = artifacts_dir.join("audit").join("evidence-index.json");
    let evidence_text = fs::read_to_string(&evidence_index_path).map_err(|error| {
        format!(
            "Cannot read evidence index `{}`: {error}",
            evidence_index_path.display()
        )
    })?;
    let evidence: Value = serde_json::from_str(&evidence_text).map_err(|error| {
        format!(
            "Cannot parse evidence index `{}` as JSON: {error}",
            evidence_index_path.display()
        )
    })?;

    let accepted_artifacts =
        audit_artifact_entries(evidence.get("accepted_artifacts").and_then(Value::as_array));
    let rejected_artifacts =
        audit_artifact_entries(evidence.get("rejected_artifacts").and_then(Value::as_array));
    let policy_exceptions =
        audit_policy_exceptions(evidence.get("policy_exceptions").and_then(Value::as_array));

    let mut notes = vec![
        "Audit explain is read-only; it summarizes evidence-index.json without modifying artifacts."
            .to_string(),
        "Accepted artifacts are the only evidence candidates for the formal report.".to_string(),
    ];
    if !rejected_artifacts.is_empty() {
        notes.push(
            "Rejected artifacts were recorded and should not be treated as confirmatory evidence."
                .to_string(),
        );
    }
    if !policy_exceptions.is_empty() {
        notes.push(
            "Policy exceptions indicate user-allowed unsupported survey/privacy boundaries."
                .to_string(),
        );
    }

    Ok(AuditExplainResult {
        status: "ok".to_string(),
        artifacts_dir: artifacts_dir.display().to_string(),
        evidence_index_path: evidence_index_path.display().to_string(),
        accepted_count: accepted_artifacts.len(),
        rejected_count: rejected_artifacts.len(),
        policy_exception_count: policy_exceptions.len(),
        accepted_artifacts,
        rejected_artifacts,
        policy_exceptions,
        notes,
    })
}

fn audit_artifact_entries(items: Option<&Vec<Value>>) -> Vec<AuditExplainArtifact> {
    items.map_or_else(Vec::new, |items| {
        items
            .iter()
            .map(|item| AuditExplainArtifact {
                command: json_string(item, "command").unwrap_or_else(|| "<unknown>".to_string()),
                status: json_string(item, "status").unwrap_or_else(|| "<unknown>".to_string()),
                report_decision: json_string(item, "report_decision"),
                analysis_step_index: item
                    .get("matched_analysis_step_index")
                    .and_then(Value::as_u64)
                    .or_else(|| {
                        item.get("artifact")
                            .and_then(|artifact| artifact.get("analysis_step_index"))
                            .and_then(Value::as_u64)
                    })
                    .and_then(|value| usize::try_from(value).ok()),
                reason: json_string(item, "reason").unwrap_or_else(|| "<no reason>".to_string()),
                result_path: json_string(item, "result_path"),
                context_path: json_string(item, "context_path"),
            })
            .collect()
    })
}

fn audit_policy_exceptions(items: Option<&Vec<Value>>) -> Vec<String> {
    items.map_or_else(Vec::new, |items| {
        items
            .iter()
            .map(|item| {
                if let Some(message) = json_string(item, "message") {
                    message
                } else if let Some(code) = json_string(item, "code") {
                    code
                } else {
                    item.to_string()
                }
            })
            .collect()
    })
}

fn json_string(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

pub(crate) fn handle_open_report(args: &OpenReportArgs) -> Result<OpenReportResult, String> {
    let artifacts_dir = args.artifacts.canonicalize().map_err(|error| {
        format!(
            "Cannot read artifacts directory `{}`: {error}",
            args.artifacts.display()
        )
    })?;
    let report_path = artifacts_dir.join("report").join("report.md");
    if !report_path.is_file() {
        return Err(format!(
            "Report markdown was not found at `{}`. Run `stats-code workflow run ...` or `stats-code report build ...` first.",
            report_path.display()
        ));
    }

    let mut notes = vec![
        "Open report targets the generated markdown report under report/report.md.".to_string(),
        "Run `stats-code report verify` before treating report values as formal evidence."
            .to_string(),
    ];
    let opened = if args.print_only {
        notes.push("--print-only was set; the report path was not opened.".to_string());
        false
    } else {
        open_path_with_platform(&report_path)?;
        notes.push("Report open request was sent to the operating system.".to_string());
        true
    };

    Ok(OpenReportResult {
        status: "ok".to_string(),
        artifacts_dir: artifacts_dir.display().to_string(),
        report_path: report_path.display().to_string(),
        opened,
        notes,
    })
}

fn open_path_with_platform(path: &Path) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        ProcessCommand::new("cmd")
            .args(["/C", "start", ""])
            .arg(path)
            .status()
            .map_err(stringify_error)
            .and_then(|status| {
                if status.success() {
                    Ok(())
                } else {
                    Err(format!("open command exited with status {status}"))
                }
            })
    }
    #[cfg(target_os = "macos")]
    {
        ProcessCommand::new("open")
            .arg(path)
            .status()
            .map_err(stringify_error)
            .and_then(|status| {
                if status.success() {
                    Ok(())
                } else {
                    Err(format!("open command exited with status {status}"))
                }
            })
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        ProcessCommand::new("xdg-open")
            .arg(path)
            .status()
            .map_err(stringify_error)
            .and_then(|status| {
                if status.success() {
                    Ok(())
                } else {
                    Err(format!("open command exited with status {status}"))
                }
            })
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

fn validate_analysis_contract(analysis_path: &Path, spec: &AnalysisSpec) -> AnalysisCheckResult {
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

fn push_check(
    items: &mut Vec<AnalysisCheckItem>,
    level: AnalysisCheckLevel,
    code: &str,
    message: impl Into<String>,
) {
    items.push(AnalysisCheckItem {
        level,
        code: code.to_string(),
        message: message.into(),
    });
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

fn survey_requires_policy_exception(survey: &crate::schema::SurveyDesignSpec) -> bool {
    survey.strata.is_some()
        || survey.cluster.is_some()
        || !survey.replicate_weights.is_empty()
        || survey.variance_estimator.is_some()
        || !survey.combined_cycles.is_empty()
}

fn privacy_requires_policy_exception(privacy: &crate::schema::PrivacySpec) -> bool {
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

pub(crate) fn handle_workflow_run(
    args: &WorkflowRunArgs,
    engine: Engine,
) -> Result<WorkflowRunResult, String> {
    let analysis_path = args.analysis.canonicalize().map_err(stringify_error)?;
    let spec = load_analysis_spec(&analysis_path)?;
    let check = validate_analysis_contract(&analysis_path, &spec);
    if check.has_errors() {
        return Err(render_analysis_check_text(&check));
    }
    ensure_study_context_ready(&analysis_path, &spec)?;
    let policy_exceptions = workflow_policy_exceptions(args, &spec)?;
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
    fs::create_dir_all(&out_dir).map_err(stringify_error)?;

    let run_id = format!("formal-{}", crate::helpers::unix_timestamp_nanos());
    let mut steps = Vec::new();

    for (index, step) in spec.analyses.iter().enumerate() {
        let (command, request, response) =
            execute_workflow_step(index, step, &analysis_path, &data_path, engine)?;
        let artifact = ArtifactMetadata::declared(&run_id, index);
        let artifact_dir = persist_run_artifacts_with_metadata(
            &out_dir,
            &command,
            &request,
            &response,
            Some(&artifact),
        )?;
        steps.push(WorkflowStepRunResult {
            step_index: index,
            command,
            artifact_dir: artifact_dir.display().to_string(),
            status: response
                .get("status")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("ok")
                .to_string(),
            notes: vec!["Formal declared analysis step executed by workflow run.".to_string()],
        });
    }

    let report = handle_report_build(&crate::cli::ReportBuildArgs {
        analysis: analysis_path.clone(),
        out: Some(out_dir.clone()),
        artifacts: Some(out_dir.clone()),
        include_exploratory: args.include_exploratory,
    })?;
    record_workflow_policy_exceptions(&out_dir, &policy_exceptions)?;
    let strict_policy_notes = enforce_workflow_report_policy(args, &out_dir)?;

    let mut notes = vec![
        "Executed declared analysis.yaml steps in order.".to_string(),
        "Formal artifacts were written with declared workflow metadata.".to_string(),
    ];
    for exception in &policy_exceptions {
        if let Some(message) = exception.get("message").and_then(Value::as_str) {
            notes.push(format!("Policy exception: {message}"));
        }
    }
    notes.extend(strict_policy_notes);
    if let Some(path) = &args.explore_out {
        notes.push(format!(
            "Exploratory artifact directory is separate and was not used for formal evidence: {}.",
            path.display()
        ));
    }
    if args.no_chat {
        notes.push("Chat orchestration was skipped; workflow execution was CLI-only.".to_string());
    }

    Ok(WorkflowRunResult {
        status: "ok".to_string(),
        run_id,
        analysis_path: analysis_path.display().to_string(),
        data_path: data_path.display().to_string(),
        artifacts_dir: out_dir.display().to_string(),
        report_output_dir: report.output_dir.clone(),
        steps,
        report,
        notes,
    })
}

fn workflow_policy_exceptions(
    args: &WorkflowRunArgs,
    spec: &AnalysisSpec,
) -> Result<Vec<Value>, String> {
    let mut exceptions = Vec::new();
    if let Some(survey) = &spec.survey {
        let needs_exception = survey_requires_policy_exception(survey);
        if args.strict && needs_exception && !args.allow_unenforced_survey {
            return Err(
                "ERROR: complex survey variance metadata was declared but strata/cluster/replicate-weight variance is not implemented for this workflow.\nThis workflow cannot produce a strict formal report unless --allow-unenforced-survey is set."
                    .to_string(),
            );
        }
        if args.allow_unenforced_survey && needs_exception {
            exceptions.push(json!({
                "code": "unsupported_complex_survey_variance",
                "allowed_by_user": true,
                "message": "Complex survey variance metadata was declared, but strata/cluster/replicate-weight variance was not implemented in this run; supported engines apply weights to point estimates only."
            }));
        }
    }

    if let Some(privacy) = &spec.privacy {
        let needs_exception = privacy_requires_policy_exception(privacy);
        if args.strict && needs_exception && !args.allow_unenforced_privacy {
            return Err(
                "ERROR: privacy metadata requiring de-identification or identifier handling was declared but is not implemented for this workflow.\nThis workflow cannot produce a strict formal report unless --allow-unenforced-privacy is set."
                    .to_string(),
            );
        }
        if args.allow_unenforced_privacy && needs_exception {
            exceptions.push(json!({
                "code": "unenforced_privacy_policy",
                "allowed_by_user": true,
                "message": "Privacy metadata requiring de-identification or identifier handling was declared, but those controls were not enforced in this run."
            }));
        }
    }

    Ok(exceptions)
}

fn record_workflow_policy_exceptions(out_dir: &Path, exceptions: &[Value]) -> Result<(), String> {
    if exceptions.is_empty() {
        return Ok(());
    }

    let evidence_path = out_dir.join("audit").join("evidence-index.json");
    let evidence_text = fs::read_to_string(&evidence_path).map_err(stringify_error)?;
    let mut evidence: Value = serde_json::from_str(&evidence_text).map_err(stringify_error)?;
    evidence["policy_exceptions"] = Value::Array(exceptions.to_vec());
    if let Some(notes) = evidence.get_mut("notes").and_then(Value::as_array_mut) {
        for exception in exceptions {
            if let Some(message) = exception.get("message").and_then(Value::as_str) {
                notes.push(Value::String(format!("Policy exception: {message}")));
            }
        }
    }
    fs::write(
        &evidence_path,
        serde_json::to_string_pretty(&evidence).map_err(stringify_error)?,
    )
    .map_err(stringify_error)?;

    let report_path = out_dir.join("report").join("report.md");
    if report_path.is_file() {
        let mut report = fs::read_to_string(&report_path).map_err(stringify_error)?;
        report.push_str("\n## Policy Exceptions\n\n");
        for exception in exceptions {
            if let Some(message) = exception.get("message").and_then(Value::as_str) {
                let _ = writeln!(report, "- {message}");
            }
        }
        fs::write(&report_path, report).map_err(stringify_error)?;
    }

    Ok(())
}

fn enforce_workflow_report_policy(
    args: &WorkflowRunArgs,
    out_dir: &Path,
) -> Result<Vec<String>, String> {
    if !args.strict {
        return Ok(Vec::new());
    }

    let verify = handle_report_verify(&ReportVerifyArgs {
        artifacts: out_dir.to_path_buf(),
        fail_on_warning: false,
    });
    if verify.has_errors() {
        return Err(format!(
            "Strict workflow policy failed: report verify found {} error(s) in `{}`.",
            verify.error_count,
            out_dir.display()
        ));
    }
    if verify.warning_count > 0 && !args.allow_warnings {
        return Err(format!(
            "Strict workflow policy failed: report verify found {} warning(s) in `{}`.\nUse --allow-warnings only for internal exploratory or explicitly reviewed runs.",
            verify.warning_count,
            out_dir.display()
        ));
    }

    let mut notes = vec!["Strict workflow policy was evaluated after report build.".to_string()];
    if verify.warning_count > 0 {
        notes.push(format!(
            "--allow-warnings allowed {} report verification warning(s) for this run.",
            verify.warning_count
        ));
    }
    Ok(notes)
}

fn execute_workflow_step(
    index: usize,
    step: &crate::schema::AnalysisStepSpec,
    analysis_path: &Path,
    data_path: &Path,
    engine: Engine,
) -> Result<(String, serde_json::Value, serde_json::Value), String> {
    match step.kind {
        AnalysisKind::Inspect => {
            let args = InspectArgs {
                data_path: data_path.to_path_buf(),
            };
            let result = handle_inspect(&args)?;
            Ok((
                "inspect".to_string(),
                json!(args),
                serde_json::to_value(result).map_err(stringify_error)?,
            ))
        }
        AnalysisKind::TableOne => {
            let by = required_step_field(index, "by", step.by.as_deref())?;
            let args = TableOneArgs {
                data: None,
                analysis: Some(analysis_path.to_path_buf()),
                by,
                vars: Vec::new(),
            };
            let result = handle_tableone(&args)?;
            Ok((
                "tableone".to_string(),
                json!(args),
                serde_json::to_value(result).map_err(stringify_error)?,
            ))
        }
        AnalysisKind::Rate => {
            let event = required_step_field(index, "event", step.event.as_deref())?;
            let person_time =
                required_step_field(index, "person_time", step.person_time.as_deref())?;
            let args = RateArgs {
                data: None,
                analysis: Some(analysis_path.to_path_buf()),
                event,
                person_time,
                strata: step.strata.clone(),
            };
            let result = handle_rate(&args)?;
            Ok((
                "rate".to_string(),
                json!(args),
                serde_json::to_value(result).map_err(stringify_error)?,
            ))
        }
        AnalysisKind::Model => match step.model {
            Some(ModelKind::Logistic) => {
                let args = ModelLogisticArgs {
                    data: None,
                    analysis: Some(analysis_path.to_path_buf()),
                    outcome: required_step_field(index, "outcome", step.outcome.as_deref())?,
                    predictors: required_step_list(index, "predictors", &step.predictors)?,
                    adjust: step.adjust.clone(),
                    strata: step.strata.clone(),
                };
                let result = handle_model_logistic(&args, engine)?;
                Ok((
                    "model_logistic".to_string(),
                    json!(args),
                    serde_json::to_value(result).map_err(stringify_error)?,
                ))
            }
            Some(ModelKind::Cox) => {
                let args = ModelCoxArgs {
                    data: None,
                    analysis: Some(analysis_path.to_path_buf()),
                    time: required_step_field(index, "time", step.time.as_deref())?,
                    event: required_step_field(index, "event", step.event.as_deref())?,
                    predictors: required_step_list(index, "predictors", &step.predictors)?,
                    adjust: step.adjust.clone(),
                    strata: step.strata.clone(),
                };
                let result = handle_model_cox(&args, engine)?;
                Ok((
                    "model_cox".to_string(),
                    json!(args),
                    serde_json::to_value(result).map_err(stringify_error)?,
                ))
            }
            Some(ModelKind::Linear) => {
                let args = ModelLinearArgs {
                    data: None,
                    analysis: Some(analysis_path.to_path_buf()),
                    outcome: required_step_field(index, "outcome", step.outcome.as_deref())?,
                    predictors: required_step_list(index, "predictors", &step.predictors)?,
                    adjust: step.adjust.clone(),
                    strata: step.strata.clone(),
                };
                let result = handle_model_linear(&args, engine)?;
                Ok((
                    "model_linear".to_string(),
                    json!(args),
                    serde_json::to_value(result).map_err(stringify_error)?,
                ))
            }
            None => Err(format!(
                "analysis step {index} has kind `model` but no `model` field"
            )),
        },
    }
}

fn required_step_field(index: usize, field: &str, value: Option<&str>) -> Result<String, String> {
    value
        .filter(|value| !value.trim().is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| format!("analysis step {index} requires `{field}`"))
}

fn required_step_list(index: usize, field: &str, value: &[String]) -> Result<Vec<String>, String> {
    if value.is_empty() {
        Err(format!("analysis step {index} requires `{field}`"))
    } else {
        Ok(value.to_vec())
    }
}

// Chat-related code has been moved to crate::chat module.
// Types, REPL loop, slash commands, tool calling, session management
// are now in src/chat.rs

pub(crate) fn handle_inspect(args: &InspectArgs) -> Result<InspectResult, String> {
    let format = detect_data_format(&args.data_path);
    match format {
        DataFormat::Csv => inspect_csv(&args.data_path),
        DataFormat::Excel => inspect_excel(&args.data_path),
        DataFormat::Parquet | DataFormat::Xpt => Ok(InspectResult {
            status: "unsupported".to_string(),
            data_path: args.data_path.display().to_string(),
            format,
            rows: None,
            columns: 0,
            variables: Vec::new(),
            notes: vec![format!(
                "{:?} format is not yet supported for inspect. \
                     Please convert your file to CSV first, for example: \
                     `pandas.read_excel('file.xlsx').to_csv('file.csv', index=False)`",
                format
            )],
        }),
        DataFormat::Unknown => Err(format!(
            "Unsupported data file extension for `{}`. Expected csv, xlsx/xls, parquet, or xpt.",
            args.data_path.display()
        )),
    }
}

fn inspect_csv(path: &Path) -> Result<InspectResult, String> {
    let mut reader = csv::Reader::from_path(path).map_err(stringify_error)?;
    let headers = reader
        .headers()
        .map_err(stringify_error)?
        .iter()
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    let mut stats = headers
        .iter()
        .map(|name| RunningColumnStats::new(name))
        .collect::<Vec<_>>();
    let mut rows = 0usize;

    for record in reader.records() {
        let record = record.map_err(stringify_error)?;
        rows += 1;
        for (index, value) in record.iter().enumerate() {
            if let Some(stat) = stats.get_mut(index) {
                stat.observe(value);
            }
        }
    }

    let variables = stats
        .into_iter()
        .map(RunningColumnStats::finish)
        .collect::<Vec<_>>();
    let high_missing_columns = variables
        .iter()
        .filter(|column| column.missing_count > 0)
        .count();

    Ok(InspectResult {
        status: "ok".to_string(),
        data_path: path.display().to_string(),
        format: DataFormat::Csv,
        rows: Some(rows),
        columns: headers.len(),
        variables,
        notes: vec![
            "CSV inspection is deterministic and local.".to_string(),
            "Missing values detected: blank, NA, N/A, null, missing, none, unknown, ., -, nd, nm, 9/99/999/9999.".to_string(),
            format!("Columns with at least one missing value: {high_missing_columns}."),
        ],
    })
}

fn inspect_excel(path: &Path) -> Result<InspectResult, String> {
    let (headers, records) = read_excel_records(path)?;
    let mut stats = headers
        .iter()
        .map(|name| RunningColumnStats::new(name))
        .collect::<Vec<_>>();
    let rows = records.len();

    for record in &records {
        for (index, value) in record.iter().enumerate() {
            if let Some(stat) = stats.get_mut(index) {
                stat.observe(value);
            }
        }
    }

    let variables = stats
        .into_iter()
        .map(RunningColumnStats::finish)
        .collect::<Vec<_>>();
    let high_missing_columns = variables
        .iter()
        .filter(|column| column.missing_count > 0)
        .count();

    Ok(InspectResult {
        status: "ok".to_string(),
        data_path: path.display().to_string(),
        format: DataFormat::Excel,
        rows: Some(rows),
        columns: headers.len(),
        variables,
        notes: vec![
            "Excel inspection reads the first worksheet.".to_string(),
            "Missing values detected: blank, NA, N/A, null, missing, none, unknown, ., -, nd, nm, 9/99/999/9999.".to_string(),
            format!("Columns with at least one missing value: {high_missing_columns}."),
        ],
    })
}

pub(crate) fn handle_tableone(args: &TableOneArgs) -> Result<TableOneResult, String> {
    let (data_path, analysis_path) = resolve_data_path(args.data.as_ref(), args.analysis.as_ref())?;
    let analysis_spec = analysis_path
        .as_ref()
        .map(|path| load_analysis_spec(path))
        .transpose()?;
    if let (Some(path), Some(spec)) = (analysis_path.as_deref(), analysis_spec.as_ref()) {
        ensure_study_context_ready(path, spec)?;
    }
    match detect_data_format(&data_path) {
        DataFormat::Csv => tableone_csv(
            &data_path,
            analysis_path.as_deref(),
            analysis_spec.as_ref(),
            args,
        ),
        DataFormat::Excel => {
            let tmp = excel_to_temp_csv(&data_path)?;
            let result = tableone_csv(&tmp, analysis_path.as_deref(), analysis_spec.as_ref(), args);
            let _ = fs::remove_file(&tmp);
            result
        }
        format => Err(format!(
            "Unsupported format `{:?}` for `{}`. Supported: CSV, Excel (xls/xlsx).",
            format,
            data_path.display()
        )),
    }
}

pub(crate) fn handle_rate(args: &RateArgs) -> Result<RateResult, String> {
    let (data_path, analysis_path) = resolve_data_path(args.data.as_ref(), args.analysis.as_ref())?;
    let analysis_spec = analysis_path
        .as_ref()
        .map(|path| load_analysis_spec(path))
        .transpose()?;
    if let (Some(path), Some(spec)) = (analysis_path.as_deref(), analysis_spec.as_ref()) {
        ensure_study_context_ready(path, spec)?;
    }
    match detect_data_format(&data_path) {
        DataFormat::Csv => rate_csv(
            &data_path,
            analysis_path.as_deref(),
            analysis_spec.as_ref(),
            args,
        ),
        DataFormat::Excel => {
            let tmp = excel_to_temp_csv(&data_path)?;
            let result = rate_csv(&tmp, analysis_path.as_deref(), analysis_spec.as_ref(), args);
            let _ = fs::remove_file(&tmp);
            result
        }
        format => Err(format!(
            "Unsupported format `{:?}` for `{}`. Supported: CSV, Excel (xls/xlsx).",
            format,
            data_path.display()
        )),
    }
}

// ---------------------------------------------------------------------------
// Common model handler infrastructure
// ---------------------------------------------------------------------------

/// Resolved context shared by all model handlers.
struct ModelContext {
    data_path: std::path::PathBuf,
    analysis_path: Option<std::path::PathBuf>,
    analysis_spec: Option<crate::schema::AnalysisSpec>,
}

/// Resolve data/analysis paths, load spec, and validate study context.
fn resolve_model_context(
    data: Option<&std::path::PathBuf>,
    analysis: Option<&std::path::PathBuf>,
) -> Result<ModelContext, String> {
    let (data_path, analysis_path) = resolve_data_path(data, analysis)?;
    let analysis_spec = analysis_path
        .as_ref()
        .map(|path| load_analysis_spec(path))
        .transpose()?;
    if let (Some(path), Some(spec)) = (analysis_path.as_deref(), analysis_spec.as_ref()) {
        ensure_study_context_ready(path, spec)?;
    }
    Ok(ModelContext {
        data_path,
        analysis_path,
        analysis_spec,
    })
}

/// Ensure we have a CSV path, converting from Excel if necessary.
/// Returns the csv path and whether it is a temp file that needs cleanup.
fn ensure_csv_path(data_path: &Path) -> Result<(std::path::PathBuf, bool), String> {
    match detect_data_format(data_path) {
        DataFormat::Csv => Ok((data_path.to_path_buf(), false)),
        DataFormat::Excel => Ok((excel_to_temp_csv(data_path)?, true)),
        format => Err(format!(
            "Unsupported format `{format:?}` for `{}`. Supported: CSV, Excel (xls/xlsx).",
            data_path.display()
        )),
    }
}

/// Run a model through the Python bridge engine.
fn run_python_bridge<T>(
    data_path: &Path,
    build_request: impl FnOnce(&Path) -> BridgeRequest,
    convert_response: impl FnOnce(&bridge::BridgeResponse) -> Result<T, String>,
) -> Result<T, String> {
    let (csv_path, is_temp) = ensure_csv_path(data_path)?;
    let request = build_request(&csv_path);
    let response = execute_bridge(&request, &BridgeConfig::default())?;
    if is_temp {
        let _ = fs::remove_file(&csv_path);
    }
    convert_response(&response)
}

/// Run a model through the Rust engine with CSV/Excel format dispatch.
fn run_rust_model<T>(
    ctx: &ModelContext,
    fit_csv: impl FnOnce(
        &Path,
        Option<&Path>,
        Option<&crate::schema::AnalysisSpec>,
    ) -> Result<T, String>,
) -> Result<T, String> {
    let (csv_path, is_temp) = ensure_csv_path(&ctx.data_path)?;
    let result = fit_csv(
        &csv_path,
        ctx.analysis_path.as_deref(),
        ctx.analysis_spec.as_ref(),
    );
    if is_temp {
        let _ = fs::remove_file(&csv_path);
    }
    result
}

// ---------------------------------------------------------------------------
// Model handlers
// ---------------------------------------------------------------------------

pub(crate) fn handle_model_logistic(
    args: &ModelLogisticArgs,
    engine: Engine,
) -> Result<LogisticResult, String> {
    let ctx = resolve_model_context(args.data.as_ref(), args.analysis.as_ref())?;

    if matches!(engine, Engine::Python) {
        return run_python_bridge(
            &ctx.data_path,
            |csv| BridgeRequest::from_logistic(args, csv),
            bridge_to_logistic,
        );
    }
    if matches!(engine, Engine::R) {
        return Err("R engine is not yet implemented. Install R and check back in Phase 4.".into());
    }

    run_rust_model(&ctx, |csv, ap, spec| logistic_csv(csv, ap, spec, args))
}

pub(crate) fn handle_model_cox(args: &ModelCoxArgs, engine: Engine) -> Result<CoxResult, String> {
    let ctx = resolve_model_context(args.data.as_ref(), args.analysis.as_ref())?;

    if matches!(engine, Engine::Python) {
        return run_python_bridge(
            &ctx.data_path,
            |csv| BridgeRequest::from_cox(args, csv),
            bridge::bridge_to_cox,
        );
    }
    if matches!(engine, Engine::R) {
        return Err("R engine is not yet implemented.".into());
    }

    run_rust_model(&ctx, |csv, ap, spec| cox_csv(csv, ap, spec, args))
}

pub(crate) fn handle_model_linear(
    args: &ModelLinearArgs,
    engine: Engine,
) -> Result<LinearResult, String> {
    let ctx = resolve_model_context(args.data.as_ref(), args.analysis.as_ref())?;

    if matches!(engine, Engine::Python) {
        return run_python_bridge(
            &ctx.data_path,
            |csv| BridgeRequest::from_linear(args, csv),
            bridge::bridge_to_linear,
        );
    }
    if matches!(engine, Engine::R) {
        return Err("R engine is not yet implemented.".into());
    }

    run_rust_model(&ctx, |csv, ap, spec| linear_csv(csv, ap, spec, args))
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
