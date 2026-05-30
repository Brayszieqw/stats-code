//! CI smoke test — structural assertions on the parity workflow.
//!
//! Task 12.3: Verify `.github/workflows/parity.yml` exists and contains
//! the expected triggers and path filters. This is a dev-only verification
//! step (not in CI must-run list) that asserts the workflow file is
//! correctly wired to trigger on changes to `crates/stats-code/src/**`.
//!
//! _Requirements: 4.1_

use std::fs;
use std::path::PathBuf;

/// Locate the repository root by walking up from the test binary's
/// manifest directory.
fn repo_root() -> PathBuf {
    // CARGO_MANIFEST_DIR points to `crates/stats-code/`
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // Walk up two levels: crates/stats-code → crates → repo root
    manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .expect("cannot find repo root from CARGO_MANIFEST_DIR")
        .to_path_buf()
}

#[test]
fn parity_workflow_file_exists() {
    let workflow_path = repo_root().join(".github/workflows/parity.yml");
    assert!(
        workflow_path.exists(),
        "parity workflow must exist at .github/workflows/parity.yml; checked: {}",
        workflow_path.display()
    );
}

#[test]
fn parity_workflow_triggers_on_pull_request() {
    let workflow_path = repo_root().join(".github/workflows/parity.yml");
    let content = fs::read_to_string(&workflow_path)
        .expect("failed to read parity.yml");

    assert!(
        content.contains("pull_request:"),
        "parity workflow must trigger on pull_request events"
    );
}

#[test]
fn parity_workflow_filters_on_stats_code_src() {
    let workflow_path = repo_root().join(".github/workflows/parity.yml");
    let content = fs::read_to_string(&workflow_path)
        .expect("failed to read parity.yml");

    // The workflow must reference `crates/stats-code/src/**` in its path filter
    assert!(
        content.contains("crates/stats-code/src/**"),
        "parity workflow must filter on crates/stats-code/src/** path changes"
    );
}

#[test]
fn parity_workflow_runs_on_windows_latest() {
    let workflow_path = repo_root().join(".github/workflows/parity.yml");
    let content = fs::read_to_string(&workflow_path)
        .expect("failed to read parity.yml");

    assert!(
        content.contains("windows-latest"),
        "parity workflow must run on windows-latest (Requirement 4.6)"
    );
}

#[test]
fn parity_workflow_uploads_report_artifact() {
    let workflow_path = repo_root().join(".github/workflows/parity.yml");
    let content = fs::read_to_string(&workflow_path)
        .expect("failed to read parity.yml");

    assert!(
        content.contains("upload-artifact"),
        "parity workflow must upload report artifacts"
    );
    assert!(
        content.contains("report.json") || content.contains("report"),
        "parity workflow must reference report output in artifact upload"
    );
}

#[test]
fn parity_workflow_has_timeout() {
    let workflow_path = repo_root().join(".github/workflows/parity.yml");
    let content = fs::read_to_string(&workflow_path)
        .expect("failed to read parity.yml");

    // Requirement 4.2: 60-minute timeout
    assert!(
        content.contains("timeout-minutes: 60"),
        "parity workflow must have a 60-minute timeout (Requirement 4.2)"
    );
}
