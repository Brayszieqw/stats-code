//! `SkillRunner`: subprocess scheduling for `Stats_Engine` CLI invocations.
//!
//! Spawns `stats-code <subcommand...>` as a child process, passes arguments via stdin
//! (never on the command line), enforces wall-clock timeout and memory limits, and
//! parses structured JSON output from stdout.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::Value;
use thiserror::Error;
use tokio::process::Command;

use crate::models::skill::SkillResult;
use crate::skill::detect_risk_signals;
use crate::skill::registry::{SkillDescriptor, SkillInvoker};
use crate::util::stderr_excerpt;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors that can occur when running a skill subprocess.
#[derive(Debug, Error, Clone)]
pub enum SkillRunError {
    /// The subprocess exceeded `max_wall_secs`.
    #[error("skill execution timed out (exceeded {wall_secs}s)")]
    Timeout { wall_secs: u32 },

    /// The subprocess was killed due to memory limit (detected via exit code/signal).
    #[error("skill execution exceeded memory limit ({max_rss_mib} MiB)")]
    Oom { max_rss_mib: u32 },

    /// The subprocess exited with a non-zero code or produced unparseable output.
    #[error("skill execution failed (exit_code={exit_code:?}): {stderr_excerpt}")]
    ExecutionFailed {
        exit_code: Option<i32>,
        stderr_excerpt: String,
    },

    /// Could not spawn the subprocess at all.
    #[error("failed to spawn skill process: {reason}")]
    SpawnFailed { reason: String },
}

// ---------------------------------------------------------------------------
// SkillRunner
// ---------------------------------------------------------------------------

/// Subprocess scheduler for `Stats_Engine` CLI skill invocations.
///
/// Enforces:
/// - Wall-clock timeout (`max_wall_secs`)
/// - Memory limit (`max_rss_mib`, platform-dependent)
/// - Data isolation: arguments are passed via stdin JSON, never on the command line
pub struct SkillRunner {
    /// Path to the `stats-code` binary.
    pub stats_code_bin: PathBuf,
    /// Working directory for child processes.
    pub workspace_root: PathBuf,
    /// Maximum wall-clock seconds before killing the child (default 60).
    pub max_wall_secs: u32,
    /// Maximum resident set size in MiB (default 1024).
    pub max_rss_mib: u32,
}

impl SkillRunner {
    /// Create a new `SkillRunner` with explicit configuration.
    #[must_use]
    pub fn new(
        stats_code_bin: PathBuf,
        workspace_root: PathBuf,
        max_wall_secs: u32,
        max_rss_mib: u32,
    ) -> Self {
        Self {
            stats_code_bin,
            workspace_root,
            max_wall_secs,
            max_rss_mib,
        }
    }

    /// Build a `tokio::process::Command` for the given skill descriptor and dataset path.
    ///
    /// The command's argv contains ONLY:
    /// - The binary path
    /// - The subcommand tokens (e.g. `["model", "linear", "--json"]`)
    /// - `--data-file <dataset_path>` to point at the dataset
    ///
    /// **No argument values from `args` appear in argv.** Data is passed via stdin.
    #[must_use]
    pub fn build_command(&self, desc: &SkillDescriptor, dataset_path: &Path) -> Command {
        let subcommand = match &desc.invoker {
            SkillInvoker::StatsCli { subcommand } => subcommand.clone(),
            SkillInvoker::NativeFn { .. } => {
                // NativeFn skills should not go through subprocess path,
                // but we handle gracefully with an empty subcommand.
                vec![]
            }
        };

        let mut cmd = Command::new(&self.stats_code_bin);
        cmd.kill_on_drop(true);
        cmd.current_dir(&self.workspace_root);

        // Add subcommand tokens (e.g. "model", "linear", "--json")
        for token in &subcommand {
            cmd.arg(token);
        }

        // Add dataset path as a known flag (not user data)
        cmd.arg("--data-file");
        cmd.arg(dataset_path.as_os_str());

        // Configure stdin for piping args JSON
        cmd.stdin(std::process::Stdio::piped());
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        // Platform-specific memory limits
        #[cfg(target_os = "linux")]
        {
            use std::os::unix::process::CommandExt;
            let max_bytes = (self.max_rss_mib as u64) * 1024 * 1024;
            // Safety: pre_exec runs in the child after fork, before exec.
            // setrlimit is async-signal-safe on Linux.
            unsafe {
                cmd.pre_exec(move || {
                    let rlim = libc::rlimit {
                        rlim_cur: max_bytes,
                        rlim_max: max_bytes,
                    };
                    if libc::setrlimit(libc::RLIMIT_AS, &rlim) != 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                    Ok(())
                });
            }
        }

        #[cfg(target_os = "macos")]
        {
            use std::os::unix::process::CommandExt;
            let max_bytes = (self.max_rss_mib as u64) * 1024 * 1024;
            unsafe {
                cmd.pre_exec(move || {
                    let rlim = libc::rlimit {
                        rlim_cur: max_bytes,
                        rlim_max: max_bytes,
                    };
                    if libc::setrlimit(libc::RLIMIT_RSS, &rlim) != 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                    Ok(())
                });
            }
        }

        // Windows: Job Object memory limits are set after spawn (see `run` method)
        // No pre_exec equivalent on Windows.

        cmd
    }

    /// Run a skill as a subprocess.
    ///
    /// 1. Builds the command (argv contains no user data)
    /// 2. Spawns the child process
    /// 3. Writes `args` as JSON to the child's stdin
    /// 4. Waits for completion with a wall-clock timeout
    /// 5. Parses stdout JSON into `SkillResult`
    ///
    /// Returns `SkillRunError` on timeout, OOM, non-zero exit, or parse failure.
    pub async fn run(
        &self,
        desc: &SkillDescriptor,
        args: Value,
        dataset_path: &Path,
    ) -> Result<SkillResult, SkillRunError> {
        let mut cmd = self.build_command(desc, dataset_path);

        // Spawn the child
        let mut child = cmd.spawn().map_err(|e| SkillRunError::SpawnFailed {
            reason: e.to_string(),
        })?;

        // Write args JSON to stdin
        if let Some(mut stdin) = child.stdin.take() {
            let args_bytes = serde_json::to_vec(&args).unwrap_or_default();
            use tokio::io::AsyncWriteExt;
            // Write and close stdin; ignore errors (child may have exited)
            let _ = stdin.write_all(&args_bytes).await;
            let _ = stdin.shutdown().await;
        }

        // Apply Windows Job Object memory limit after spawn
        #[cfg(target_os = "windows")]
        {
            // On Windows, memory limits via Job Objects require win32 API calls.
            // For now we rely on the timeout to catch runaway processes.
            // A full implementation would use CreateJobObject + AssignProcessToJobObject
            // + SetInformationJobObject(JobObjectExtendedLimitInformation).
            // This is left as a platform-specific enhancement.
            let _ = self.max_rss_mib; // suppress unused warning
        }

        // Wait with timeout.
        // `wait_with_output` takes ownership. `kill_on_drop(true)` makes the
        // timeout path terminate the child instead of leaving a detached stats process.
        let timeout_duration = Duration::from_secs(u64::from(self.max_wall_secs));
        let output =
            tokio::time::timeout(timeout_duration, child.wait_with_output()).await;

        let output = match output {
            Ok(Ok(output)) => output,
            Ok(Err(e)) => {
                return Err(SkillRunError::SpawnFailed {
                    reason: format!("failed to wait on child: {e}"),
                });
            }
            Err(_elapsed) => {
                // Timeout: child is dropped, which kills it.
                return Err(SkillRunError::Timeout {
                    wall_secs: self.max_wall_secs,
                });
            }
        };

        // Check for OOM signals
        if is_oom_exit(&output) {
            return Err(SkillRunError::Oom {
                max_rss_mib: self.max_rss_mib,
            });
        }

        // Check exit code
        if !output.status.success() {
            let excerpt = stderr_excerpt(&output.stderr, 4096);
            return Err(SkillRunError::ExecutionFailed {
                exit_code: output.status.code(),
                stderr_excerpt: excerpt,
            });
        }

        // Parse stdout as JSON
        let payload: Value =
            serde_json::from_slice(&output.stdout).map_err(|e| SkillRunError::ExecutionFailed {
                exit_code: output.status.code(),
                stderr_excerpt: format!(
                    "stdout JSON parse error: {e}; raw: {}",
                    stderr_excerpt(&output.stdout, 512)
                ),
            })?;

        // Detect risk signals from the payload
        let risk_signals = detect_risk_signals(&payload);

        Ok(SkillResult {
            schema_version: "1.0".to_string(),
            payload,
            risk_signals,
            analysis: None,
        })
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Detect if the process exit indicates an out-of-memory condition.
fn is_oom_exit(output: &std::process::Output) -> bool {
    // On Unix, SIGKILL (signal 9) from the OOM killer or cgroup
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = output.status.signal() {
            // SIGKILL = 9 (OOM killer), SIGXFSZ = 25 (sometimes used)
            if signal == 9 {
                return true;
            }
        }
    }

    // On Windows, exit code 0xC0000017 (STATUS_NO_MEMORY) or similar
    #[cfg(windows)]
    {
        if let Some(code) = output.status.code() {
            // STATUS_NO_MEMORY = 0xC0000017 as i32
            let status_no_memory = 0xC0000017_u32 as i32;
            if code == status_no_memory {
                return true;
            }
        }
    }

    // Heuristic: check stderr for common OOM messages
    let stderr_str = String::from_utf8_lossy(&output.stderr);
    stderr_str.contains("out of memory")
        || stderr_str.contains("Cannot allocate memory")
        || stderr_str.contains("memory allocation")
}

/// Extract the argv list from a `Command` for testing purposes.
///
/// Returns the program name followed by all arguments.
#[must_use]
pub fn command_argv(cmd: &Command) -> Vec<String> {
    let program = cmd.as_std().get_program().to_string_lossy().to_string();
    let cmd_args: Vec<String> = cmd
        .as_std()
        .get_args()
        .map(|a| a.to_string_lossy().to_string())
        .collect();
    let mut argv = vec![program];
    argv.extend(cmd_args);
    argv
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::path::PathBuf;

    fn test_runner() -> SkillRunner {
        SkillRunner::new(
            PathBuf::from("/usr/local/bin/stats-code"),
            PathBuf::from("/tmp/workspace"),
            60,
            1024,
        )
    }

    fn test_descriptor() -> SkillDescriptor {
        SkillDescriptor {
            skill_id: "model_linear".into(),
            display_name: "线性回归".into(),
            input_schema: json!({"type": "object"}),
            output_schema: json!({"type": "object"}),
            invoker: SkillInvoker::StatsCli {
                subcommand: vec!["model".into(), "linear".into(), "--json".into()],
            },
        }
    }

    #[test]
    fn test_build_command_contains_subcommand_tokens() {
        let runner = test_runner();
        let desc = test_descriptor();
        let dataset = PathBuf::from("/data/test.csv");

        let cmd = runner.build_command(&desc, &dataset);
        let argv = command_argv(&cmd);

        // Should contain the binary
        assert!(argv[0].contains("stats-code"));
        // Should contain subcommand tokens
        assert!(argv.contains(&"model".to_string()));
        assert!(argv.contains(&"linear".to_string()));
        assert!(argv.contains(&"--json".to_string()));
        // Should contain --data-file flag
        assert!(argv.contains(&"--data-file".to_string()));
    }

    #[test]
    fn test_build_command_does_not_contain_args_values() {
        let runner = test_runner();
        let desc = test_descriptor();
        let dataset = PathBuf::from("/data/test.csv");

        // Simulate user args with various string values
        let args = json!({
            "outcome": "blood_pressure",
            "predictors": ["age", "weight", "height"],
            "dataset_id": "ds-12345",
            "notes": "This is sensitive patient data with special chars: <>&\"'"
        });

        let cmd = runner.build_command(&desc, &dataset);
        let argv = command_argv(&cmd);

        // Extract all string values from args
        let arg_strings = extract_string_values(&args);

        // None of the arg string values should appear in argv
        for val in &arg_strings {
            if val.is_empty() {
                continue;
            }
            for arg in &argv {
                assert!(
                    !arg.contains(val.as_str()),
                    "argv element {arg:?} contains args value {val:?}"
                );
            }
        }
    }

    #[test]
    fn test_build_command_sets_working_directory() {
        let runner = test_runner();
        let desc = test_descriptor();
        let dataset = PathBuf::from("/data/test.csv");

        let cmd = runner.build_command(&desc, &dataset);
        let std_cmd = cmd.as_std();
        assert_eq!(
            std_cmd.get_current_dir(),
            Some(Path::new("/tmp/workspace"))
        );
    }

    #[test]
    fn test_build_command_native_fn_produces_empty_subcommand() {
        use std::sync::Arc;

        let runner = test_runner();
        let desc = SkillDescriptor {
            skill_id: "native_test".into(),
            display_name: "Native".into(),
            input_schema: json!({}),
            output_schema: json!({}),
            invoker: SkillInvoker::NativeFn {
                handler: Arc::new(|_| {
                    Box::pin(async { Err("not implemented".into()) })
                }),
            },
        };
        let dataset = PathBuf::from("/data/test.csv");

        let cmd = runner.build_command(&desc, &dataset);
        let argv = command_argv(&cmd);

        // Should have binary + --data-file + path, no subcommand tokens
        assert_eq!(argv.len(), 3);
        assert!(argv[0].contains("stats-code"));
        assert_eq!(argv[1], "--data-file");
    }

    #[test]
    fn test_new_sets_fields_correctly() {
        let runner = SkillRunner::new(
            PathBuf::from("/bin/sc"),
            PathBuf::from("/work"),
            30,
            512,
        );
        assert_eq!(runner.stats_code_bin, PathBuf::from("/bin/sc"));
        assert_eq!(runner.workspace_root, PathBuf::from("/work"));
        assert_eq!(runner.max_wall_secs, 30);
        assert_eq!(runner.max_rss_mib, 512);
    }

    #[test]
    fn test_is_oom_exit_stderr_heuristic() {
        let output = std::process::Output {
            status: make_exit_status(1),
            stdout: vec![],
            stderr: b"fatal: out of memory".to_vec(),
        };
        assert!(is_oom_exit(&output));
    }

    #[test]
    fn test_is_oom_exit_normal_failure() {
        let output = std::process::Output {
            status: make_exit_status(1),
            stdout: vec![],
            stderr: b"error: invalid argument".to_vec(),
        };
        assert!(!is_oom_exit(&output));
    }

    // Helper: extract all string values from a JSON Value recursively
    fn extract_string_values(val: &Value) -> Vec<String> {
        let mut result = Vec::new();
        match val {
            Value::String(s) => result.push(s.clone()),
            Value::Array(arr) => {
                for item in arr {
                    result.extend(extract_string_values(item));
                }
            }
            Value::Object(map) => {
                for v in map.values() {
                    result.extend(extract_string_values(v));
                }
            }
            _ => {}
        }
        result
    }

    // Helper: create an ExitStatus with a given code (platform-specific)
    #[cfg(unix)]
    fn make_exit_status(code: i32) -> std::process::ExitStatus {
        use std::os::unix::process::ExitStatusExt;
        std::process::ExitStatus::from_raw(code << 8)
    }

    #[cfg(windows)]
    fn make_exit_status(code: i32) -> std::process::ExitStatus {
        use std::os::windows::process::ExitStatusExt;
        std::process::ExitStatus::from_raw(code as u32)
    }
}
