use clap::Parser;
use serde_json::json;

mod analysis;
mod audit;
mod common;
mod data;
mod model;
mod power;
mod project;
mod run;
mod survival;
mod workflow;

pub(crate) use data::{handle_inspect, handle_rate, handle_tableone};
pub(crate) use model::{handle_model_cox, handle_model_linear, handle_model_logistic};
pub(crate) use survival::handle_survival_km;
pub(crate) use workflow::handle_workflow_run;

use analysis::{handle_analysis_check, handle_analysis_plan};
use audit::{handle_audit_explain, handle_open_report};
use power::handle_power;
use project::{handle_doctor, handle_init_project};
use run::{handle_run_script, render_run_script_text};

use crate::bridge::Engine;
use crate::chat::run_chat_repl;
use crate::cli::{
    AiCommand, AuditCommand, AuthCommand, ChatArgs, Cli, Command, ConfigCommand, ModelCommand,
    OpenCommand, ReportCommand, ReportVerifyArgs, RunCommand, SurvivalCommand, WorkflowCommand,
};
use crate::config::{
    handle_ai_ask, handle_auth_doctor, handle_auth_set, handle_config_add_model,
    handle_config_default_model, handle_config_remove_model, handle_config_show,
};
use crate::error::StatsCodeResult;
use crate::render::{
    render_ai_ask_text, render_analysis_check_text, render_audit_explain_text,
    render_auth_doctor_text, render_auth_set_text, render_config_text, render_cox_text,
    render_doctor_text, render_init_project_text, render_inspect_text, render_linear_text,
    render_logistic_text, render_open_report_text, render_planned_text, render_power_text,
    render_rate_text, render_report_build_text, render_report_verify_text, render_survival_km_text,
    render_tableone_text, render_workflow_run_text,
};
use crate::report::{
    handle_report_build, handle_report_verify, persist_run_artifacts_with_metadata,
};
use crate::schema::{
    AiAskResult, AnalysisCheckResult, ArtifactMetadata, ArtifactRole, ArtifactStatus,
    AuditExplainResult, AuthDoctorResult, AuthSetResult, ConfigResult, CoxResult, DoctorResult,
    InitProjectResult, InspectResult, LinearResult, LogisticResult, OpenReportResult,
    PlannedCommandResult, PowerResult, RateResult, ReportBuildResult, ReportVerifyResult,
    SurvivalKmResult, TableOneResult, WorkflowRunResult,
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
    let response = serde_json::to_value(&result)?;
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
        println!("{}", serde_json::to_string_pretty(&response)?);
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
            ("init", json!(args), serde_json::to_value(result)?)
        }
        Command::Doctor(args) => {
            let result = handle_doctor(args);
            ("doctor", json!(args), serde_json::to_value(result)?)
        }
        Command::Plan(args) => {
            let result = handle_analysis_plan(args)?;
            ("plan", json!(args), serde_json::to_value(result)?)
        }
        Command::Config { command } => match command {
            ConfigCommand::Show => {
                let result = handle_config_show()?;
                (
                    "config",
                    json!({ "action": "show" }),
                    serde_json::to_value(result)?,
                )
            }
            ConfigCommand::DefaultModel(args) => {
                let result = handle_config_default_model(args)?;
                (
                    "config",
                    json!({ "action": "default_model", "model": args.model }),
                    serde_json::to_value(result)?,
                )
            }
            ConfigCommand::AddModel(args) => {
                let result = handle_config_add_model(args)?;
                (
                    "config",
                    json!({ "action": "add_model", "model": args.model }),
                    serde_json::to_value(result)?,
                )
            }
            ConfigCommand::RemoveModel(args) => {
                let result = handle_config_remove_model(args)?;
                (
                    "config",
                    json!({ "action": "remove_model", "model": args.model }),
                    serde_json::to_value(result)?,
                )
            }
        },
        Command::Check(args) => {
            let result = handle_analysis_check(args)?;
            ("analysis_check", json!(args), serde_json::to_value(result)?)
        }
        Command::Inspect(args) => {
            let result = handle_inspect(args)?;
            ("inspect", json!(args), serde_json::to_value(result)?)
        }
        Command::Tableone(args) => {
            let result = handle_tableone(args)?;
            ("tableone", json!(args), serde_json::to_value(result)?)
        }
        Command::Rate(args) => {
            let result = handle_rate(args)?;
            ("rate", json!(args), serde_json::to_value(result)?)
        }
        Command::Power { command } => {
            let result = handle_power(command)?;
            ("power", json!(command), serde_json::to_value(result)?)
        }
        Command::Survival { command } => match command {
            SurvivalCommand::Km(args) => {
                let result = handle_survival_km(args)?;
                ("survival_km", json!(args), serde_json::to_value(result)?)
            }
        },
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
                    serde_json::to_value(result)?,
                )
            }
            AuthCommand::Doctor(args) => {
                let result = handle_auth_doctor(args)?;
                ("auth_doctor", json!(args), serde_json::to_value(result)?)
            }
        },
        Command::Ai { command } => match command {
            AiCommand::Ask(args) => {
                let result = handle_ai_ask(args)?;
                ("ai_ask", json!(args), serde_json::to_value(result)?)
            }
        },
        Command::Audit { command } => match command {
            AuditCommand::Explain(args) => {
                let result = handle_audit_explain(args)?;
                ("audit_explain", json!(args), serde_json::to_value(result)?)
            }
        },
        Command::Open { command } => match command {
            OpenCommand::Report(args) => {
                let result = handle_open_report(args)?;
                ("open_report", json!(args), serde_json::to_value(result)?)
            }
        },
        Command::Model { command } => match command {
            ModelCommand::Logistic(args) => {
                let result = handle_model_logistic(args, cli.engine)?;
                ("model_logistic", json!(args), serde_json::to_value(result)?)
            }
            ModelCommand::Cox(args) => {
                let result = handle_model_cox(args, cli.engine)?;
                ("model_cox", json!(args), serde_json::to_value(result)?)
            }
            ModelCommand::Linear(args) => {
                let result = handle_model_linear(args, cli.engine)?;
                ("model_linear", json!(args), serde_json::to_value(result)?)
            }
        },
        Command::Report { command } => match command {
            ReportCommand::Build(args) => {
                let result = handle_report_build(args)?;
                ("report_build", json!(args), serde_json::to_value(result)?)
            }
            ReportCommand::Verify(args) => {
                let result = handle_report_verify(args);
                ("report_verify", json!(args), serde_json::to_value(result)?)
            }
        },
        Command::Workflow { command } => match command {
            WorkflowCommand::Run(args) => {
                let result = handle_workflow_run(args, cli.engine)?;
                ("workflow_run", json!(args), serde_json::to_value(result)?)
            }
        },
        Command::Run { command } => {
            let (engine, args) = match command {
                RunCommand::Python(args) => (Engine::Python, args),
                RunCommand::R(args) => (Engine::R, args),
            };
            let result = handle_run_script(engine, args)?;
            let request_val = json!(args);
            let response_val = serde_json::to_value(&result)?;

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
                return Ok(serde_json::to_string_pretty(&response_val)?);
            }
            return Ok(render_run_script_text(&result));
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
        Ok(serde_json::to_string_pretty(&response)?)
    } else {
        match name {
            "inspect" => {
                let value: InspectResult = serde_json::from_value(response)?;
                Ok(render_inspect_text(&value))
            }
            "tableone" => {
                let value: TableOneResult = serde_json::from_value(response)?;
                Ok(render_tableone_text(&value))
            }
            "model_logistic" => {
                let value: LogisticResult = serde_json::from_value(response)?;
                Ok(render_logistic_text(&value))
            }
            "model_cox" => {
                let value: CoxResult = serde_json::from_value(response)?;
                Ok(render_cox_text(&value))
            }
            "model_linear" => {
                let value: LinearResult = serde_json::from_value(response)?;
                Ok(render_linear_text(&value))
            }
            "rate" => {
                let value: RateResult = serde_json::from_value(response)?;
                Ok(render_rate_text(&value))
            }
            "survival_km" => {
                let value: SurvivalKmResult = serde_json::from_value(response)?;
                Ok(render_survival_km_text(&value))
            }
            "power" => {
                let value: PowerResult = serde_json::from_value(response)?;
                Ok(render_power_text(&value))
            }
            "auth_set" => {
                let value: AuthSetResult = serde_json::from_value(response)?;
                Ok(render_auth_set_text(&value))
            }
            "auth_doctor" => {
                let value: AuthDoctorResult = serde_json::from_value(response)?;
                Ok(render_auth_doctor_text(&value))
            }
            "ai_ask" => {
                let value: AiAskResult = serde_json::from_value(response)?;
                Ok(render_ai_ask_text(&value))
            }
            "audit_explain" => {
                let value: AuditExplainResult = serde_json::from_value(response)?;
                Ok(render_audit_explain_text(&value))
            }
            "open_report" => {
                let value: OpenReportResult = serde_json::from_value(response)?;
                Ok(render_open_report_text(&value))
            }
            "config" => {
                let value: ConfigResult = serde_json::from_value(response)?;
                Ok(render_config_text(&value))
            }
            "init" => {
                let value: InitProjectResult = serde_json::from_value(response)?;
                Ok(render_init_project_text(&value))
            }
            "doctor" => {
                let value: DoctorResult = serde_json::from_value(response)?;
                Ok(render_doctor_text(&value))
            }
            "analysis_check" => {
                let value: AnalysisCheckResult = serde_json::from_value(response)?;
                Ok(render_analysis_check_text(&value))
            }
            "report_build" => {
                let value: ReportBuildResult = serde_json::from_value(response)?;
                Ok(render_report_build_text(&value))
            }
            "report_verify" => {
                let value: ReportVerifyResult = serde_json::from_value(response)?;
                Ok(render_report_verify_text(&value))
            }
            "workflow_run" => {
                let value: WorkflowRunResult = serde_json::from_value(response)?;
                Ok(render_workflow_run_text(&value))
            }
            _ => {
                let value: PlannedCommandResult = serde_json::from_value(response)?;
                Ok(render_planned_text(&value))
            }
        }
    }
}

// Report-related code has been moved to crate::report module.
// handle_report_build, discover_report_evidence, evidence types, etc.
// are now in src/report.rs

#[cfg(test)]
mod tests;
