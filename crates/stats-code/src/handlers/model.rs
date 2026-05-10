use std::fs;
use std::path::Path;

use crate::bridge::{
    self, bridge_to_logistic, execute_bridge, BridgeConfig, BridgeRequest, Engine,
};
use crate::cli::{ModelCoxArgs, ModelLinearArgs, ModelLogisticArgs};
use crate::cox::cox_csv;
use crate::helpers::excel_to_temp_csv;
use crate::linear::linear_csv;
use crate::logistic::logistic_csv;
use crate::report::{ensure_study_context_ready, resolve_data_path};
use crate::schema::{
    detect_data_format, load_analysis_spec, CoxResult, DataFormat, LinearResult, LogisticResult,
};
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
