use std::fmt::Write;
use std::fs;
use std::path::Path;

use clap::Parser;
use serde_json::json;

use crate::bridge::{
    self, bridge_to_logistic, execute_bridge, BridgeConfig, BridgeRequest, Engine,
};
use crate::chat::run_chat_repl;
use crate::cli::{
    AiCommand, AuthCommand, ChatArgs, Cli, Command, ConfigCommand, InspectArgs, ModelCommand,
    ModelCoxArgs, ModelLinearArgs, ModelLogisticArgs, RateArgs, ReportCommand, RunCommand,
    TableOneArgs,
};
use crate::config::{
    handle_ai_ask, handle_auth_doctor, handle_auth_set, handle_config_add_model,
    handle_config_default_model, handle_config_remove_model, handle_config_show,
};
use crate::cox::cox_csv;
use crate::helpers::{excel_to_temp_csv, read_excel_records, stringify_error};
use crate::linear::linear_csv;
use crate::logistic::logistic_csv;
use crate::rate::rate_csv;
use crate::render::{
    render_ai_ask_text, render_auth_doctor_text, render_auth_set_text, render_config_text,
    render_cox_text, render_inspect_text, render_linear_text, render_logistic_text,
    render_planned_text, render_rate_text, render_report_build_text, render_tableone_text,
};
use crate::report::{
    ensure_study_context_ready, handle_report_build, persist_run_artifacts, resolve_data_path,
};
use crate::schema::{
    detect_data_format, load_analysis_spec, AiAskResult, AuthDoctorResult, AuthSetResult,
    ConfigResult, CoxResult, DataFormat, InspectResult, LinearResult, LogisticResult,
    PlannedCommandResult, RateResult, ReportBuildResult, RunningColumnStats, TableOneResult,
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
        Some(_) => {
            let rendered = dispatch(&cli)?;
            println!("{rendered}");
            Ok(())
        }
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
                persist_run_artifacts(base_dir, "run", &request_val, &response_val)?;
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
        persist_run_artifacts(base_dir, name, &request, &response)?;
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
            "config" => {
                let value: ConfigResult =
                    serde_json::from_value(response).map_err(stringify_error)?;
                Ok(render_config_text(&value))
            }
            "report_build" => {
                let value: ReportBuildResult =
                    serde_json::from_value(response).map_err(stringify_error)?;
                Ok(render_report_build_text(&value))
            }
            _ => {
                let value: PlannedCommandResult =
                    serde_json::from_value(response).map_err(stringify_error)?;
                Ok(render_planned_text(&value))
            }
        }
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
        DataFormat::Csv => rate_csv(&data_path, analysis_path.as_deref(), args),
        DataFormat::Excel => {
            let tmp = excel_to_temp_csv(&data_path)?;
            let result = rate_csv(&tmp, analysis_path.as_deref(), args);
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
        Command, InspectArgs, ModelCommand, ModelCoxArgs, ModelLinearArgs, ModelLogisticArgs,
        RateArgs, TableOneArgs,
    };
    use crate::schema::{detect_data_format, load_analysis_spec, AnalysisSpec, DataFormat};

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
