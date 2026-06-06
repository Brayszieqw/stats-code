use std::fs;
use std::process::Command;

use serde_json::Value;

fn stats_code_bin() -> &'static str {
    env!("CARGO_BIN_EXE_stats-code")
}

fn write_rows(path: &std::path::Path, rows: &[(&str, &str, &str)]) {
    let mut csv = String::from("exposure,outcome,stratum\n");
    for (exposure, outcome, stratum) in rows {
        csv.push_str(&format!("{exposure},{outcome},{stratum}\n"));
    }
    fs::write(path, csv).unwrap();
}

fn expand_rows(
    rows: &mut Vec<(&'static str, &'static str, &'static str)>,
    stratum: &'static str,
    exposed: &'static str,
    outcome: &'static str,
    n: usize,
) {
    rows.extend(std::iter::repeat_n((exposed, outcome, stratum), n));
}

fn run_or_rr(path: &std::path::Path) -> Value {
    let output = Command::new(stats_code_bin())
        .args([
            "--json",
            "stats",
            "epi",
            "or-rr",
            "--data",
            path.to_str().unwrap(),
            "--exposure",
            "exposure",
            "--outcome",
            "outcome",
            "--strata",
            "stratum",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn stratified_fixture(path: &std::path::Path) {
    let mut rows = Vec::new();
    expand_rows(&mut rows, "s1", "1", "1", 12);
    expand_rows(&mut rows, "s1", "1", "0", 5);
    expand_rows(&mut rows, "s1", "0", "1", 7);
    expand_rows(&mut rows, "s1", "0", "0", 20);
    expand_rows(&mut rows, "s2", "1", "1", 8);
    expand_rows(&mut rows, "s2", "1", "0", 10);
    expand_rows(&mut rows, "s2", "0", "1", 5);
    expand_rows(&mut rows, "s2", "0", "0", 18);
    write_rows(path, &rows);
}

#[test]
fn stratified_or_rr_cli_emits_mh_and_breslow_day_fields() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("stratified_or_rr.csv");
    stratified_fixture(&data);

    let json = run_or_rr(&data);
    let result = &json["result"];

    assert_eq!(json["command"].as_str(), Some("stats.epi.or_rr"));
    assert!((result["mh_or"].as_f64().unwrap() - 4.450_068_775_790_922).abs() < 1e-12);
    assert!((result["mh_rr"].as_f64().unwrap() - 2.418_825_659_011_200_3).abs() < 1e-12);
    assert!(
        (result["homogeneity_chi_square"].as_f64().unwrap() - 0.790_486_074_050_739_6).abs()
            < 1e-12
    );
    assert!((result["homogeneity_p"].as_f64().unwrap() - 0.373_953_187_852_784_2).abs() < 1e-12);
    assert_eq!(result["mh_strata"].as_array().unwrap().len(), 2);
}

#[test]
fn stratified_or_rr_cli_default_text_uses_method_renderer() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("stratified_or_rr.csv");
    stratified_fixture(&data);

    let output = Command::new(stats_code_bin())
        .args([
            "stats",
            "epi",
            "or-rr",
            "--data",
            data.to_str().unwrap(),
            "--exposure",
            "exposure",
            "--outcome",
            "outcome",
            "--strata",
            "stratum",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Odds Ratio / Relative Risk"));
    assert!(stdout.contains("MH OR"));
    assert!(stdout.contains("Breslow-Day"));
}

#[test]
fn stratified_or_rr_cli_handles_zero_cells() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("stratified_or_rr_zero.csv");
    let mut rows = Vec::new();
    expand_rows(&mut rows, "s1", "1", "1", 1);
    expand_rows(&mut rows, "s2", "1", "0", 3);
    expand_rows(&mut rows, "s2", "0", "1", 2);
    write_rows(&data, &rows);

    let json = run_or_rr(&data);
    let result = &json["result"];
    assert_eq!(result["continuity_correction"].as_bool(), Some(true));
    assert!(result["mh_or"].as_f64().unwrap().is_finite());
    assert!(result["mh_rr"].as_f64().unwrap().is_finite());
    assert!(result["homogeneity_p"].as_f64().unwrap().is_finite());
}

#[test]
fn analysis_check_accepts_epi_or_rr_step() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data.csv");
    stratified_fixture(&data);
    let analysis = dir.path().join("analysis.yaml");
    fs::write(
        &analysis,
        r"schema_version: stats-code.v0
study:
  title: Stratified OR/RR
  design: cohort
study_context:
  estimand: Mantel-Haenszel adjusted risk ratio
  exposure: Exposure
  comparator: Unexposed
  outcome: Outcome
  time_zero: Baseline
  follow_up: End of follow-up
  censoring: Administrative
  missing_data_strategy: Complete case
  clustering: None
  sensitivity_analyses: None
  reporting_guideline: STROBE
data:
  path: data.csv
  format: csv
variables:
  - name: exposure
    kind: binary
    roles: [exposure]
  - name: outcome
    kind: binary
    roles: [outcome]
  - name: stratum
    kind: categorical
    roles: [strata]
analyses:
  - id: stratified_or_rr
    kind: epi.or_rr
    exposure: exposure
    outcome: outcome
    strata: [stratum]
",
    )
    .unwrap();

    let output = Command::new(stats_code_bin())
        .args(["--json", "check", analysis.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["status"].as_str(), Some("ok"));
    assert_eq!(json["error_count"].as_u64(), Some(0));
}

#[test]
fn workflow_run_executes_epi_or_rr_step() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data.csv");
    stratified_fixture(&data);
    let analysis = dir.path().join("analysis.yaml");
    let out_dir = dir.path().join("artifacts");
    fs::write(
        &analysis,
        r"schema_version: stats-code.v0
study:
  title: Stratified OR/RR
  design: cohort
study_context:
  estimand: Mantel-Haenszel adjusted odds ratio
  exposure: Exposure
  comparator: Unexposed
  outcome: Outcome
  time_zero: Baseline
  follow_up: End of follow-up
  censoring: Administrative
  missing_data_strategy: Complete case
  clustering: None
  sensitivity_analyses: None
  reporting_guideline: STROBE
data:
  path: data.csv
  format: csv
variables:
  - name: exposure
    kind: binary
    roles: [exposure]
  - name: outcome
    kind: binary
    roles: [outcome]
  - name: stratum
    kind: categorical
    roles: [strata]
analyses:
  - id: stratified_or_rr
    kind: epi.or_rr
    exposure: exposure
    outcome: outcome
    strata: [stratum]
",
    )
    .unwrap();

    let output = Command::new(stats_code_bin())
        .args([
            "--json",
            "workflow",
            "run",
            analysis.to_str().unwrap(),
            "--out",
            out_dir.to_str().unwrap(),
            "--no-chat",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["status"].as_str(), Some("ok"));
    assert_eq!(
        json["steps"][0]["command"].as_str(),
        Some("stats.epi.or_rr")
    );
}
