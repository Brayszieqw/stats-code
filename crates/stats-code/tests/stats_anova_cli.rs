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

fn write_oneway(path: &std::path::Path) {
    fs::write(
        path,
        "group,value\nA,12\nA,14\nA,15\nA,13\nB,18\nB,20\nB,21\nB,19\nC,25\nC,24\nC,27\nC,26\n",
    )
    .unwrap();
}

fn write_rbd(path: &std::path::Path) {
    fs::write(
        path,
        "group,block,value\nA,B1,12\nB,B1,18\nC,B1,23\nA,B2,13\nB,B2,20\nC,B2,25\nA,B3,15\nB,B3,21\nC,B3,28\nA,B4,14\nB,B4,19\nC,B4,26\n",
    )
    .unwrap();
}

#[test]
fn oneway_anova_cli_emits_snapshot_shape() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("anova.csv");
    write_oneway(&data);

    let json = run_json(&[
        "--json",
        "stats",
        "anova",
        "oneway",
        "--data",
        data.to_str().unwrap(),
        "--var",
        "value",
        "--group",
        "group",
    ]);
    let result = &json["result"];

    assert_eq!(json["status"].as_str(), Some("ok"));
    assert_eq!(json["command"].as_str(), Some("stats.anova.oneway"));
    assert_eq!(result["variable"].as_str(), Some("value"));
    assert_eq!(result["group"].as_str(), Some("group"));
    assert_eq!(result["groups"].as_array().unwrap().len(), 3);
    assert_eq!(result["df_between"].as_u64(), Some(2));
    assert_eq!(result["df_within"].as_u64(), Some(9));
    assert!((result["ss_total"].as_f64().unwrap() - 303.0).abs() < 1e-12);
    assert!((result["f_statistic"].as_f64().unwrap() - 86.4).abs() < 1e-10);
    assert!((result["p_value"].as_f64().unwrap() - 0.0000013363457521235217).abs() < 1e-10);
}

#[test]
fn rbd_anova_cli_emits_snapshot_shape() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("rbd.csv");
    write_rbd(&data);

    let json = run_json(&[
        "--json",
        "stats",
        "anova",
        "oneway",
        "--data",
        data.to_str().unwrap(),
        "--var",
        "value",
        "--group",
        "group",
        "--block",
        "block",
    ]);
    let result = &json["result"];

    assert_eq!(json["status"].as_str(), Some("ok"));
    assert_eq!(json["command"].as_str(), Some("stats.anova.oneway"));
    assert_eq!(result["block"].as_str(), Some("block"));
    assert_eq!(result["treatment_df1"].as_u64(), Some(2));
    assert_eq!(result["treatment_df2"].as_u64(), Some(6));
    assert!((result["treatment_f"].as_f64().unwrap() - 324.0).abs() < 1e-8);
    assert!((result["block_f"].as_f64().unwrap() - 15.25).abs() < 1e-8);
}

#[test]
fn oneway_anova_cli_rejects_sparse_group() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("sparse.csv");
    fs::write(&data, "group,value\nA,12\nA,14\nB,18\n").unwrap();

    let output = Command::new(stats_code_bin())
        .args([
            "--json",
            "stats",
            "anova",
            "oneway",
            "--data",
            data.to_str().unwrap(),
            "--var",
            "value",
            "--group",
            "group",
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("group `B` has 1"), "stderr={stderr}");
}
