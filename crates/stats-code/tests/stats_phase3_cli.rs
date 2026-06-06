//! CLI integration tests for Phase 3 Rust-native methods (tasks 32.4, 34.3).
//! Python-bridge methods (ordinal, multinomial, lda, mixed, competing) require
//! `--engine python` and a Python environment, so they are tested separately.

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
        "args={args:?}\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

// =========================================================================
// 32.4 — Cluster analysis CLI (k-means)
// =========================================================================

#[test]
fn cluster_kmeans_cli_emits_snapshot_shape() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("cluster.csv");
    fs::write(
        &data,
        "x1,x2\n1.0,1.0\n1.5,1.2\n1.2,0.8\n5.0,5.0\n5.5,5.2\n5.2,4.8\n9.0,9.0\n9.5,9.2\n9.2,8.8\n",
    )
    .unwrap();

    let json = run_json(&[
        "--json", "stats", "multivariate", "cluster",
        "--data", data.to_str().unwrap(),
        "--vars", "x1,x2",
        "--k", "3",
        "--method", "kmeans",
        "--seed", "42",
    ]);
    let result = &json["result"];
    assert_eq!(json["status"].as_str(), Some("ok"));
    assert_eq!(json["command"].as_str(), Some("stats.multivariate.cluster"));
    assert_eq!(result["method"].as_str(), Some("kmeans"));
    assert_eq!(result["k"].as_u64(), Some(3));
    assert_eq!(result["assignments"].as_array().unwrap().len(), 9);
    assert!(result["silhouette_avg"].as_f64().unwrap() > 0.5);
}

// =========================================================================
// 32.4 — Cluster analysis CLI (hierarchical)
// =========================================================================

#[test]
fn cluster_hierarchical_cli_emits_snapshot_shape() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("cluster.csv");
    fs::write(
        &data,
        "x1,x2\n1.0,1.0\n1.5,1.2\n1.2,0.8\n5.0,5.0\n5.5,5.2\n5.2,4.8\n9.0,9.0\n9.5,9.2\n9.2,8.8\n",
    )
    .unwrap();

    let json = run_json(&[
        "--json", "stats", "multivariate", "cluster",
        "--data", data.to_str().unwrap(),
        "--vars", "x1,x2",
        "--k", "3",
        "--method", "hierarchical",
        "--seed", "42",
    ]);
    let result = &json["result"];
    assert_eq!(json["status"].as_str(), Some("ok"));
    assert_eq!(result["method"].as_str(), Some("hierarchical"));
    assert_eq!(result["k"].as_u64(), Some(3));
    assert_eq!(result["assignments"].as_array().unwrap().len(), 9);
}

// =========================================================================
// 34.3 — PSM CLI
// =========================================================================

#[test]
fn psm_cli_emits_snapshot_shape() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("psm.csv");
    // Simple treatment/control with one covariate
    let mut csv = String::from("treatment,age\n");
    for i in 0..20 {
        let t = i32::from(i < 10);
        let age = 30 + i * 2 + t * 5;
        csv.push_str(&format!("{t},{age}\n"));
    }
    fs::write(&data, &csv).unwrap();

    let json = run_json(&[
        "--json", "stats", "psm",
        "--data", data.to_str().unwrap(),
        "--treatment", "treatment",
        "--covariates", "age",
        "--seed", "42",
    ]);
    let result = &json["result"];
    assert_eq!(json["status"].as_str(), Some("ok"));
    assert_eq!(json["command"].as_str(), Some("stats.psm"));
    assert!(result["n_treated"].as_u64().unwrap() > 0);
    assert!(result["n_matched_pairs"].as_u64().unwrap() > 0);
    assert!(result["balance"].as_array().is_some());
}
