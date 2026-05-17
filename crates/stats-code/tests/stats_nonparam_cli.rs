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

fn write_cochran_armitage(path: &std::path::Path) {
    let mut csv = String::from("exposure,outcome\n");
    for (exposure, events, total) in [("a", 2usize, 20usize), ("b", 5, 20), ("c", 11, 20)] {
        for i in 0..total {
            let outcome = if i < events { "1" } else { "0" };
            csv.push_str(&format!("{exposure},{outcome}\n"));
        }
    }
    fs::write(path, csv).unwrap();
}

#[test]
fn cochran_armitage_cli_emits_snapshot_shape() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("trend.csv");
    write_cochran_armitage(&data);

    let json = run_json(&[
        "--json",
        "stats",
        "nonparam",
        "cochran-armitage",
        "--data",
        data.to_str().unwrap(),
        "--exposure",
        "exposure",
        "--outcome",
        "outcome",
        "--scores",
        "0,1,2",
    ]);
    let result = &json["result"];

    assert_eq!(json["status"].as_str(), Some("ok"));
    assert_eq!(
        json["command"].as_str(),
        Some("stats.nonparam.cochran_armitage")
    );
    assert_eq!(result["n_used"].as_u64(), Some(60));
    assert_eq!(result["categories"].as_array().unwrap().len(), 3);
    assert!((result["trend_statistic"].as_f64().unwrap() - 3.1052950170405937).abs() < 1e-12);
    assert!((result["p_value"].as_f64().unwrap() - 0.001900893250446667).abs() < 2e-7);
}

#[test]
fn mcnemar_cli_emits_snapshot_shape() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("mcnemar.csv");
    fs::write(
        &data,
        "var1,var2\n1,1\n1,1\n1,1\n1,1\n0,0\n0,0\n0,0\n0,0\n1,0\n1,0\n1,0\n0,1\n0,1\n0,1\n0,1\n0,1\n0,1\n0,1\n0,1\n0,1\n",
    )
    .unwrap();

    let json = run_json(&[
        "--json",
        "stats",
        "nonparam",
        "mcnemar",
        "--data",
        data.to_str().unwrap(),
        "--var1",
        "var1",
        "--var2",
        "var2",
    ]);
    let result = &json["result"];

    assert_eq!(json["command"].as_str(), Some("stats.nonparam.mcnemar"));
    assert_eq!(result["b"].as_u64(), Some(3));
    assert_eq!(result["c"].as_u64(), Some(9));
    assert_eq!(result["n_concordant"].as_u64(), Some(8));
    assert!((result["chi_square"].as_f64().unwrap() - 2.0833333333333335).abs() < 1e-12);
    assert!((result["exact_p_value"].as_f64().unwrap() - 0.14599609375).abs() < 1e-12);
}

#[test]
fn wilcoxon_cli_emits_snapshot_shape() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("wilcoxon.csv");
    fs::write(
        &data,
        "before,after\n20,22\n21,24\n22,26\n23,28\n24,30\n25,32\n",
    )
    .unwrap();

    let json = run_json(&[
        "--json",
        "stats",
        "nonparam",
        "wilcoxon",
        "--data",
        data.to_str().unwrap(),
        "--var1",
        "before",
        "--var2",
        "after",
    ]);
    let result = &json["result"];

    assert_eq!(json["command"].as_str(), Some("stats.nonparam.wilcoxon"));
    assert_eq!(result["n_used"].as_u64(), Some(6));
    assert!((result["w_plus"].as_f64().unwrap() - 21.0).abs() < 1e-12);
    assert!((result["z_statistic"].as_f64().unwrap() - 2.096569673443837).abs() < 1e-12);
    assert!((result["p_value"].as_f64().unwrap() - 0.03603168621823355).abs() < 2e-7);
}

#[test]
fn mannwhitney_cli_emits_snapshot_shape() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("mannwhitney.csv");
    fs::write(
        &data,
        "group,value\nA,12\nA,14\nA,15\nA,16\nB,20\nB,21\nB,23\nB,22\n",
    )
    .unwrap();

    let json = run_json(&[
        "--json",
        "stats",
        "nonparam",
        "mannwhitney",
        "--data",
        data.to_str().unwrap(),
        "--var",
        "value",
        "--group",
        "group",
    ]);
    let result = &json["result"];

    assert_eq!(json["command"].as_str(), Some("stats.nonparam.mannwhitney"));
    assert_eq!(result["group_a_label"].as_str(), Some("A"));
    assert_eq!(result["group_b_label"].as_str(), Some("B"));
    assert_eq!(result["n_a"].as_u64(), Some(4));
    assert_eq!(result["n_b"].as_u64(), Some(4));
    assert!((result["u_statistic"].as_f64().unwrap() - 0.0).abs() < 1e-12);
    assert!((result["p_value"].as_f64().unwrap() - 0.020921335337794014).abs() < 2e-7);
}

#[test]
fn mannwhitney_cli_rejects_more_than_two_groups() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("mannwhitney_bad.csv");
    fs::write(&data, "group,value\nA,12\nA,14\nB,20\nB,21\nC,25\nC,26\n").unwrap();

    let output = Command::new(stats_code_bin())
        .args([
            "--json",
            "stats",
            "nonparam",
            "mannwhitney",
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
    assert!(stderr.contains("exactly 2 groups"), "stderr={stderr}");
}
