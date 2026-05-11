use std::fmt::Write;

use crate::bridge::{self, CustomScriptResult, Engine};
use crate::cli::RunScriptArgs;
use crate::error::StatsCodeResult;

pub(crate) fn handle_run_script(
    engine: Engine,
    args: &RunScriptArgs,
) -> StatsCodeResult<CustomScriptResult> {
    Ok(bridge::execute_custom_script(
        engine,
        &args.script,
        args.data.as_deref(),
        args.params.as_deref(),
    )?)
}

pub(crate) fn render_run_script_text(result: &CustomScriptResult) -> String {
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
    out
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn render_run_script_text_prefers_json_output() {
        let result = CustomScriptResult {
            engine: "python".to_string(),
            script: "demo.py".to_string(),
            exit_code: Some(0),
            stdout: "{\"ok\":true}".to_string(),
            stderr: String::new(),
            parsed: Some(json!({ "ok": true })),
        };

        let rendered = render_run_script_text(&result);

        assert!(rendered.contains("Run Script"));
        assert!(rendered.contains("Output (JSON)"));
        assert!(rendered.contains("\"ok\": true"));
    }

    #[test]
    fn render_run_script_text_shows_stderr_and_raw_output() {
        let result = CustomScriptResult {
            engine: "r".to_string(),
            script: "demo.R".to_string(),
            exit_code: Some(1),
            stdout: "plain output".to_string(),
            stderr: "warning".to_string(),
            parsed: None,
        };

        let rendered = render_run_script_text(&result);

        assert!(rendered.contains("Stderr"));
        assert!(rendered.contains("warning"));
        assert!(rendered.contains("Output (raw)"));
        assert!(rendered.contains("plain output"));
    }
}
