use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

// ---------------------------------------------------------------------------
// Engine enum
// ---------------------------------------------------------------------------

/// Execution engine for statistical models.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum Engine {
    /// Native Rust implementation (default).
    Rust,
    /// Delegate to a Python subprocess.
    Python,
    /// Delegate to an R subprocess (future).
    R,
}

impl std::fmt::Display for Engine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Rust => write!(f, "rust"),
            Self::Python => write!(f, "python"),
            Self::R => write!(f, "r"),
        }
    }
}

// ---------------------------------------------------------------------------
// Bridge protocol types
// ---------------------------------------------------------------------------

/// Configuration for bridge execution.
#[derive(Debug, Clone)]
pub struct BridgeConfig {
    /// Timeout for subprocess execution.
    pub timeout: Duration,
    /// Working directory for the subprocess.
    pub work_dir: Option<PathBuf>,
}

impl Default for BridgeConfig {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(120),
            work_dir: None,
        }
    }
}

/// Request sent from Stats Code to a Python/R subprocess.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeRequest {
    pub command: String,
    pub data_path: String,
    pub params: Value,
    pub output_format: String,
}

/// Response received from a Python/R subprocess.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeResponse {
    pub status: String,
    pub engine: String,
    #[serde(default)]
    pub engine_version: Option<String>,
    #[serde(default)]
    pub package: Option<String>,
    #[serde(default)]
    pub package_version: Option<String>,
    pub result: Value,
    #[serde(default)]
    pub diagnostics: Option<BridgeDiagnostics>,
    #[serde(default)]
    pub raw_output: Option<String>,
}

/// Diagnostics attached to a bridge response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeDiagnostics {
    #[serde(default)]
    pub execution_time_ms: Option<u64>,
    #[serde(default)]
    pub warnings: Vec<String>,
}

// ---------------------------------------------------------------------------
// Engine discovery
// ---------------------------------------------------------------------------

/// Discover the path to a Python or R interpreter.
pub fn discover_engine(engine: Engine) -> Result<PathBuf, String> {
    let candidates: &[&str] = match engine {
        Engine::Python => &["python3", "python"],
        Engine::R => &["Rscript"],
        Engine::Rust => return Err("discover_engine is not applicable for the Rust engine".into()),
    };

    for candidate in candidates {
        let result = Command::new(candidate).arg("--version").output();
        if let Ok(output) = result {
            if output.status.success() {
                return Ok(PathBuf::from(candidate));
            }
        }
    }

    Err(format!(
        "Could not find a working {} interpreter. Tried: {}",
        engine,
        candidates.join(", ")
    ))
}

// ---------------------------------------------------------------------------
// Script resolution (A+B hybrid)
// ---------------------------------------------------------------------------

/// Resolve a built-in script by name.
///
/// Strategy (A+B hybrid):
///   1. Look for user override at `~/.stats-code/scripts/python/<name>`
///   2. Fall back to built-in scripts compiled via `include_str!()`
fn resolve_python_script(name: &str) -> Result<String, String> {
    // Phase B: check user override directory
    if let Some(home) = dirs_user_script_dir() {
        let user_path = home.join(name);
        if user_path.is_file() {
            return std::fs::read_to_string(&user_path)
                .map_err(|e| format!("Failed to read user script {}: {e}", user_path.display()));
        }
    }

    // Phase A: built-in scripts
    match name {
        "bridge_runner.py" => Ok(include_str!("../scripts/python/bridge_runner.py").to_string()),
        "model_logistic.py" => Ok(include_str!("../scripts/python/model_logistic.py").to_string()),
        "model_linear.py" => Ok(include_str!("../scripts/python/model_linear.py").to_string()),
        "model_cox.py" => Ok(include_str!("../scripts/python/model_cox.py").to_string()),
        _ => Err(format!("Unknown built-in script: {name}")),
    }
}

/// Return the user script override directory: `~/.stats-code/scripts/python/`
fn dirs_user_script_dir() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        std::env::var("USERPROFILE").ok().map(|p| {
            PathBuf::from(p)
                .join(".stats-code")
                .join("scripts")
                .join("python")
        })
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::env::var("HOME").ok().map(|p| {
            PathBuf::from(p)
                .join(".stats-code")
                .join("scripts")
                .join("python")
        })
    }
}

// ---------------------------------------------------------------------------
// Core execution
// ---------------------------------------------------------------------------

/// Execute a bridge request against a Python subprocess.
///
/// 1. Write JSON params to a temp file
/// 2. Resolve the `bridge_runner.py` script (user override or built-in)
/// 3. Invoke `python bridge_runner.py --input <tmp_file>`
/// 4. Parse stdout as JSON → `BridgeResponse`
pub fn execute_bridge(
    request: &BridgeRequest,
    config: &BridgeConfig,
) -> Result<BridgeResponse, String> {
    let python = discover_engine(Engine::Python)?;

    // Write the request JSON to a temp file
    let mut tmp =
        tempfile::NamedTempFile::new().map_err(|e| format!("Failed to create temp file: {e}"))?;
    let json_bytes = serde_json::to_vec_pretty(request)
        .map_err(|e| format!("Failed to serialize request: {e}"))?;
    tmp.write_all(&json_bytes)
        .map_err(|e| format!("Failed to write temp file: {e}"))?;
    tmp.flush()
        .map_err(|e| format!("Failed to flush temp file: {e}"))?;

    // Resolve the runner script
    let runner_script = resolve_python_script("bridge_runner.py")?;

    // Write runner script to a temp file so Python can execute it
    let mut runner_tmp = tempfile::Builder::new()
        .suffix(".py")
        .tempfile()
        .map_err(|e| format!("Failed to create runner temp file: {e}"))?;
    runner_tmp
        .write_all(runner_script.as_bytes())
        .map_err(|e| format!("Failed to write runner script: {e}"))?;
    runner_tmp
        .flush()
        .map_err(|e| format!("Failed to flush runner script: {e}"))?;

    // Also write the command-specific module next to the runner
    let module_name = format!("{}.py", request.command);
    let module_script = resolve_python_script(&module_name)?;
    let module_dir = runner_tmp.path().parent().unwrap_or(Path::new("."));
    let module_path = module_dir.join(&module_name);
    std::fs::write(&module_path, module_script.as_bytes())
        .map_err(|e| format!("Failed to write module script {module_name}: {e}"))?;

    // Execute: python runner.py --input params.json
    let mut cmd = Command::new(&python);
    cmd.arg(runner_tmp.path()).arg("--input").arg(tmp.path());

    if let Some(ref wd) = config.work_dir {
        cmd.current_dir(wd);
    }

    let output = cmd
        .output()
        .map_err(|e| format!("Failed to run Python subprocess: {e}"))?;

    // Clean up the module file
    let _ = std::fs::remove_file(&module_path);

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        return Err(format!(
            "Python bridge failed (exit code {:?}):\n--- stderr ---\n{stderr}\n--- stdout ---\n{stdout}",
            output.status.code()
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str::<BridgeResponse>(stdout.trim()).map_err(|e| {
        format!("Failed to parse bridge response JSON: {e}\n--- raw stdout ---\n{stdout}")
    })
}

// ---------------------------------------------------------------------------
// Request builders
// ---------------------------------------------------------------------------

use crate::cli::{ModelCoxArgs, ModelLinearArgs, ModelLogisticArgs};

impl BridgeRequest {
    #[must_use]
    pub fn from_logistic(args: &ModelLogisticArgs, data_path: &Path) -> Self {
        let mut all_predictors = args.predictors.clone();
        all_predictors.extend(args.adjust.iter().cloned());

        Self {
            command: "model_logistic".to_string(),
            data_path: data_path.display().to_string(),
            params: serde_json::json!({
                "outcome": args.outcome,
                "predictors": all_predictors,
                "ci_level": 0.95
            }),
            output_format: "statscode_v1".to_string(),
        }
    }

    #[must_use]
    pub fn from_linear(args: &ModelLinearArgs, data_path: &Path) -> Self {
        let mut all_predictors = args.predictors.clone();
        all_predictors.extend(args.adjust.iter().cloned());

        Self {
            command: "model_linear".to_string(),
            data_path: data_path.display().to_string(),
            params: serde_json::json!({
                "outcome": args.outcome,
                "predictors": all_predictors,
                "ci_level": 0.95
            }),
            output_format: "statscode_v1".to_string(),
        }
    }

    #[must_use]
    pub fn from_cox(args: &ModelCoxArgs, data_path: &Path) -> Self {
        let mut all_predictors = args.predictors.clone();
        all_predictors.extend(args.adjust.iter().cloned());

        Self {
            command: "model_cox".to_string(),
            data_path: data_path.display().to_string(),
            params: serde_json::json!({
                "time": args.time,
                "event": args.event,
                "predictors": all_predictors,
                "ci_level": 0.95
            }),
            output_format: "statscode_v1".to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// Result converters
// ---------------------------------------------------------------------------

use crate::schema::{CoxResult, LinearResult, LogisticResult};

/// Convert a bridge response into a `LogisticResult`.
pub fn bridge_to_logistic(response: &BridgeResponse) -> Result<LogisticResult, String> {
    serde_json::from_value(response.result.clone())
        .map_err(|e| format!("Failed to convert bridge result to LogisticResult: {e}"))
}

/// Convert a bridge response into a `LinearResult`.
pub fn bridge_to_linear(response: &BridgeResponse) -> Result<LinearResult, String> {
    serde_json::from_value(response.result.clone())
        .map_err(|e| format!("Failed to convert bridge result to LinearResult: {e}"))
}

/// Convert a bridge response into a `CoxResult`.
pub fn bridge_to_cox(response: &BridgeResponse) -> Result<CoxResult, String> {
    serde_json::from_value(response.result.clone())
        .map_err(|e| format!("Failed to convert bridge result to CoxResult: {e}"))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Result of running a custom user script via `run python`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomScriptResult {
    pub engine: String,
    pub script: String,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    /// If stdout is valid JSON, this holds the parsed value.
    #[serde(default)]
    pub parsed: Option<Value>,
}

/// Execute a user-provided Python or R script.
///
/// Protocol: the script receives `--input <params.json>` where params.json
/// contains `{ "data_path": "...", "params": {...} }`.
/// The script's stdout is captured and returned (parsed as JSON if possible).
pub fn execute_custom_script(
    engine: Engine,
    script_path: &Path,
    data_path: Option<&Path>,
    params_json: Option<&str>,
) -> Result<CustomScriptResult, String> {
    if !script_path.is_file() {
        return Err(format!("Script not found: {}", script_path.display()));
    }

    let interpreter = discover_engine(engine)?;

    // Build the input JSON
    let params_value: Value = match params_json {
        Some(json_str) => {
            serde_json::from_str(json_str).map_err(|e| format!("Invalid --params JSON: {e}"))?
        }
        None => Value::Object(serde_json::Map::new()),
    };

    let input_json = serde_json::json!({
        "data_path": data_path.map(|p| p.display().to_string()).unwrap_or_default(),
        "params": params_value,
    });

    // Write input to a temp file
    let mut tmp =
        tempfile::NamedTempFile::new().map_err(|e| format!("Failed to create temp file: {e}"))?;
    tmp.write_all(
        serde_json::to_vec_pretty(&input_json)
            .map_err(|e| format!("Failed to serialize input: {e}"))?
            .as_slice(),
    )
    .map_err(|e| format!("Failed to write temp file: {e}"))?;
    tmp.flush()
        .map_err(|e| format!("Failed to flush temp file: {e}"))?;

    // Execute: python <script> --input <tmp>
    let output = Command::new(&interpreter)
        .arg(script_path)
        .arg("--input")
        .arg(tmp.path())
        .output()
        .map_err(|e| format!("Failed to run {engine} subprocess: {e}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let exit_code = output.status.code();

    // Try to parse stdout as JSON
    let parsed = serde_json::from_str::<Value>(stdout.trim()).ok();

    Ok(CustomScriptResult {
        engine: engine.to_string(),
        script: script_path.display().to_string(),
        exit_code,
        stdout,
        stderr,
        parsed,
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovers_python_engine() {
        // This test requires Python on PATH; skip gracefully if not found.
        match discover_engine(Engine::Python) {
            Ok(path) => {
                let path_str = path.to_string_lossy();
                assert!(
                    path_str.contains("python"),
                    "Expected python in path, got: {path_str}"
                );
            }
            Err(msg) => {
                eprintln!("Skipping: {msg}");
            }
        }
    }

    #[test]
    fn rust_engine_discovery_returns_error() {
        let result = discover_engine(Engine::Rust);
        assert!(result.is_err());
    }

    #[test]
    fn resolves_builtin_scripts() {
        let runner = resolve_python_script("bridge_runner.py");
        assert!(runner.is_ok(), "bridge_runner.py should be built-in");
        assert!(runner.unwrap().contains("def main"));

        let logistic = resolve_python_script("model_logistic.py");
        assert!(logistic.is_ok(), "model_logistic.py should be built-in");
    }

    #[test]
    fn unknown_script_returns_error() {
        let result = resolve_python_script("nonexistent.py");
        assert!(result.is_err());
    }
}
