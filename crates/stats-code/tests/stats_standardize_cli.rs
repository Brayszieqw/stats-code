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
fn direct_standardization_cli_emits_snapshot_shape() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("rates.csv");
    let standard_pop = dir.path().join("standard_pop.csv");
    fs::write(
        &data,
        "age_group,events,person_time\nyoung,5,1000\nmiddle,12,1200\nolder,30,1500\n",
    )
    .unwrap();
    fs::write(
        &standard_pop,
        "age_group,weight\nyoung,50\nmiddle,30\nolder,20\n",
    )
    .unwrap();

    let json = run_json(&[
        "--json",
        "stats",
        "epi",
        "standardize",
        "--data",
        data.to_str().unwrap(),
        "--method",
        "direct",
        "--event",
        "events",
        "--person-time",
        "person_time",
        "--age-group",
        "age_group",
        "--standard-pop",
        standard_pop.to_str().unwrap(),
    ]);
    let result = &json["result"];

    assert_eq!(json["status"].as_str(), Some("ok"));
    assert_eq!(json["command"].as_str(), Some("stats.epi.standardize"));
    assert_eq!(result["method"].as_str(), Some("direct"));
    assert_eq!(result["n_used"].as_u64(), Some(3));
    assert_eq!(result["strata"].as_array().unwrap().len(), 3);
    assert!((result["standardized_rate"].as_f64().unwrap() - 0.0095).abs() < 1e-12);
    assert!((result["direct_ci_lower"].as_f64().unwrap() - 0.0063804334148648495).abs() < 1e-10);
    assert!((result["direct_ci_upper"].as_f64().unwrap() - 0.01261956658513515).abs() < 1e-10);
}
