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
fn paired_ttest_cli_emits_snapshot_shape() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("paired.csv");
    fs::write(
        &data,
        "before,after\n100,102\n105,108\n110,112\n115,118\n120,122\n125,130\n",
    )
    .unwrap();

    let json = run_json(&[
        "--json",
        "stats",
        "ttest",
        "paired",
        "--data",
        data.to_str().unwrap(),
        "--before",
        "before",
        "--after",
        "after",
    ]);
    let result = &json["result"];

    assert_eq!(json["status"].as_str(), Some("ok"));
    assert_eq!(json["command"].as_str(), Some("stats.ttest.paired"));
    assert_eq!(result["method"].as_str(), Some("Paired t-test"));
    assert_eq!(result["before_variable"].as_str(), Some("before"));
    assert_eq!(result["after_variable"].as_str(), Some("after"));
    assert_eq!(result["n_pairs"].as_u64(), Some(6));
    assert!((result["mean_diff"].as_f64().unwrap() - 2.8333333333333335).abs() < 1e-12);
    assert!((result["t_statistic"].as_f64().unwrap() - 5.936657514041414).abs() < 1e-8);
    assert!(result["ci_lower"].as_f64().is_some());
    assert!(result["ci_upper"].as_f64().is_some());
}

#[test]
fn one_sample_ttest_cli_emits_snapshot_shape() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("one_sample.csv");
    fs::write(&data, "value\n100\n102\n98\n101\n103\n97\n104\n100\n").unwrap();

    let json = run_json(&[
        "--json",
        "stats",
        "ttest",
        "one-sample",
        "--data",
        data.to_str().unwrap(),
        "--var",
        "value",
        "--mu",
        "100",
    ]);
    let result = &json["result"];

    assert_eq!(json["status"].as_str(), Some("ok"));
    assert_eq!(json["command"].as_str(), Some("stats.ttest.one_sample"));
    assert_eq!(result["method"].as_str(), Some("One-sample t-test"));
    assert_eq!(result["variable"].as_str(), Some("value"));
    assert_eq!(result["n"].as_u64(), Some(8));
    assert!((result["sample_mean"].as_f64().unwrap() - 100.625).abs() < 1e-12);
    assert!((result["p_value"].as_f64().unwrap() - 0.482993878967768).abs() < 1e-6);
    assert_eq!(result["alpha"].as_f64(), Some(0.05));
}
