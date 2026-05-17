use std::fs;
use std::process::Command;

use serde_json::Value;

fn stats_code_bin() -> &'static str {
    env!("CARGO_BIN_EXE_stats-code")
}

fn run_json(args: &[&str]) -> Value {
    let output = Command::new(stats_code_bin()).args(args).output().unwrap();
    assert!(
        output.status.success(),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn write_attributable(path: &std::path::Path) {
    fs::write(
        path,
        "exposure,outcome,person_time\n1,1,100\n1,1,100\n1,1,100\n1,0,100\n1,0,100\n0,1,100\n0,0,100\n0,0,100\n0,0,100\n0,0,100\n",
    )
    .unwrap();
}

#[test]
fn attributable_cli_emits_snapshot_shape() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("attributable.csv");
    write_attributable(&data);

    let json = run_json(&[
        "--json",
        "stats",
        "epi",
        "attributable",
        "--data",
        data.to_str().unwrap(),
        "--exposure",
        "exposure",
        "--outcome",
        "outcome",
        "--person-time",
        "person_time",
    ]);
    let result = &json["result"];

    assert_eq!(json["status"].as_str(), Some("ok"));
    assert_eq!(json["command"].as_str(), Some("stats.epi.attributable"));
    assert!((result["rate_exposed"].as_f64().unwrap() - 0.006).abs() < 1e-12);
    assert!((result["rate_unexposed"].as_f64().unwrap() - 0.002).abs() < 1e-12);
    assert!((result["ar"].as_f64().unwrap() - 0.004).abs() < 1e-12);
    assert!((result["par"].as_f64().unwrap() - 0.002).abs() < 1e-12);
    assert!((result["par_percent"].as_f64().unwrap() - 50.0).abs() < 1e-12);
}

#[test]
fn attributable_cli_uses_exposure_prevalence_override() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("attributable.csv");
    write_attributable(&data);

    let json = run_json(&[
        "--json",
        "stats",
        "epi",
        "attributable",
        "--data",
        data.to_str().unwrap(),
        "--exposure",
        "exposure",
        "--outcome",
        "outcome",
        "--person-time",
        "person_time",
        "--exposure-prevalence",
        "0.25",
    ]);
    let result = &json["result"];

    assert_eq!(json["command"].as_str(), Some("stats.epi.attributable"));
    assert!((result["exposure_prevalence"].as_f64().unwrap() - 0.25).abs() < 1e-12);
    assert!((result["par"].as_f64().unwrap() - 0.001).abs() < 1e-12);
    assert!((result["par_percent"].as_f64().unwrap() - 33.33333333333333).abs() < 1e-12);
}

#[test]
fn attributable_cli_rejects_invalid_exposure_prevalence() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("attributable.csv");
    write_attributable(&data);

    let output = Command::new(stats_code_bin())
        .args([
            "--json",
            "stats",
            "epi",
            "attributable",
            "--data",
            data.to_str().unwrap(),
            "--exposure",
            "exposure",
            "--outcome",
            "outcome",
            "--person-time",
            "person_time",
            "--exposure-prevalence",
            "1.5",
        ])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("between 0 and 1"), "stderr={stderr}");
}
