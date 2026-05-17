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
fn grouped_lifetable_cli_emits_snapshot_shape() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("lifetable.csv");
    fs::write(
        &data,
        "interval,entering,events,withdrawals\n0-1,100,10,5\n1-2,85,8,7\n2-3,70,5,10\n",
    )
    .unwrap();

    let json = run_json(&[
        "--json",
        "stats",
        "survival",
        "lifetable",
        "--data",
        data.to_str().unwrap(),
        "--intervals",
        "interval",
        "--entering",
        "entering",
        "--events",
        "events",
        "--withdrawals",
        "withdrawals",
    ]);
    let result = &json["result"];
    let intervals = result["intervals"].as_array().unwrap();

    assert_eq!(json["status"].as_str(), Some("ok"));
    assert_eq!(json["command"].as_str(), Some("stats.survival.lifetable"));
    assert_eq!(result["time"].as_str(), Some("interval"));
    assert_eq!(result["n_used"].as_u64(), Some(3));
    assert_eq!(intervals.len(), 3);
    assert_eq!(intervals[0]["entering"].as_u64(), Some(100));
    assert!(
        (intervals[0]["cumulative_survival"].as_f64().unwrap() - 0.8974358974358975).abs() < 1e-12
    );
    assert!(
        (intervals[2]["cumulative_hazard"].as_f64().unwrap() - 0.2776466886896335).abs() < 1e-12
    );
}
