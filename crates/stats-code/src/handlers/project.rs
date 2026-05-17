use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;

use crate::cli::{AuthDoctorArgs, DoctorArgs, InitArgs};
use crate::config::handle_auth_doctor;
use crate::helpers::{stringify_error, unix_timestamp_nanos};
use crate::schema::{AnalysisCheckItem, AnalysisCheckLevel, DoctorResult, InitProjectResult};

use super::common::push_check;
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
    copy_init_template(
        &examples_dir.join("data").join("demo_standard_pop.csv"),
        &data_dir.join("demo_standard_pop.csv"),
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
    check_required_file(
        &mut items,
        &examples_dir.join("data").join("demo_standard_pop.csv"),
        "demo_standard_population_found",
        "demo_standard_population_missing",
        "bundled demo standard population",
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
