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

#[test]
fn normality_cli_emits_snapshot_shape() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("normality.csv");
    fs::write(&data, "value\n10\n11\n12\n13\n14\n15\n16\n17\n").unwrap();

    let json = run_json(&[
        "--json",
        "stats",
        "diagnostic",
        "normality",
        "--data",
        data.to_str().unwrap(),
        "--var",
        "value",
    ]);
    let result = &json["result"];

    assert_eq!(json["status"].as_str(), Some("ok"));
    assert_eq!(json["command"].as_str(), Some("stats.diagnostic.normality"));
    assert_eq!(result["variable"].as_str(), Some("value"));
    assert_eq!(result["n"].as_u64(), Some(8));
    assert!((result["shapiro_w"].as_f64().unwrap() - 0.9897276433712999).abs() < 1e-12);
    assert!((result["ks_d"].as_f64().unwrap() - 0.10485437009596166).abs() < 1e-12);
    assert_eq!(result["lilliefors_used"].as_bool(), Some(true));
}

#[test]
fn variance_cli_emits_snapshot_shape() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("variance.csv");
    fs::write(
        &data,
        "group,value\nA,10\nA,12\nA,14\nA,16\nB,20\nB,23\nB,24\nB,27\nC,30\nC,35\nC,39\nC,42\n",
    )
    .unwrap();

    let json = run_json(&[
        "--json",
        "stats",
        "diagnostic",
        "variance",
        "--data",
        data.to_str().unwrap(),
        "--var",
        "value",
        "--group",
        "group",
        "--center",
        "median",
    ]);
    let result = &json["result"];

    assert_eq!(json["status"].as_str(), Some("ok"));
    assert_eq!(json["command"].as_str(), Some("stats.diagnostic.variance"));
    assert_eq!(result["variable"].as_str(), Some("value"));
    assert_eq!(result["group"].as_str(), Some("group"));
    assert_eq!(result["groups"].as_array().unwrap().len(), 3);
    assert!((result["levene_statistic"].as_f64().unwrap() - 1.5483870967741935).abs() < 1e-12);
    assert!((result["bartlett_statistic"].as_f64().unwrap() - 1.5780670884578505).abs() < 1e-12);
}
