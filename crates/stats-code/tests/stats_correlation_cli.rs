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

fn write_correlation(path: &std::path::Path) {
    fs::write(
        path,
        "x,y\n10,20\n11,22\n12,21\n13,25\n14,26\n15,27\n16,30\n17,29\n",
    )
    .unwrap();
}

#[test]
fn pearson_correlation_cli_emits_snapshot_shape() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("correlation.csv");
    write_correlation(&data);

    let json = run_json(&[
        "--json",
        "stats",
        "correlation",
        "--data",
        data.to_str().unwrap(),
        "--x",
        "x",
        "--y",
        "y",
        "--method",
        "pearson",
    ]);
    let result = &json["result"];

    assert_eq!(json["status"].as_str(), Some("ok"));
    assert_eq!(json["command"].as_str(), Some("stats.correlation"));
    assert_eq!(result["method"].as_str(), Some("pearson"));
    assert_eq!(result["n_pairs"].as_u64(), Some(8));
    assert!((result["r"].as_f64().unwrap() - 0.9606597022317859).abs() < 1e-12);
    assert!((result["p_value"].as_f64().unwrap() - 0.00014775766285343456).abs() < 1e-12);
    assert!(result["spearman_rho"].is_null());
}

#[test]
fn spearman_correlation_cli_emits_snapshot_shape() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("correlation.csv");
    write_correlation(&data);

    let json = run_json(&[
        "--json",
        "stats",
        "correlation",
        "--data",
        data.to_str().unwrap(),
        "--x",
        "x",
        "--y",
        "y",
        "--method",
        "spearman",
    ]);
    let result = &json["result"];

    assert_eq!(json["command"].as_str(), Some("stats.correlation"));
    assert_eq!(result["method"].as_str(), Some("spearman"));
    assert_eq!(result["n_pairs"].as_u64(), Some(8));
    assert!((result["r"].as_f64().unwrap() - 0.9523809523809524).abs() < 1e-12);
    assert!((result["spearman_rho"].as_f64().unwrap() - 0.9523809523809524).abs() < 1e-12);
    assert!((result["spearman_p_value"].as_f64().unwrap() - 0.00026040002438725105).abs() < 1e-12);
}
