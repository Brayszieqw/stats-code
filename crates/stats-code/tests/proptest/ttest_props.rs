use std::fs;
use std::process::Command;

use proptest::prelude::*;
use serde_json::Value;

fn stats_code_bin() -> &'static str {
    env!("CARGO_BIN_EXE_stats-code")
}

fn write_paired_csv(path: &std::path::Path, diffs: &[i16], sign: f64) {
    let mut out = String::from("before,after\n");
    for (i, diff) in diffs.iter().enumerate() {
        let before = i as f64 + 20.0;
        let after = before + sign * f64::from(*diff);
        out.push_str(&format!("{before},{after}\n"));
    }
    fs::write(path, out).unwrap();
}

fn run_paired(path: &std::path::Path) -> Value {
    let output = Command::new(stats_code_bin())
        .args([
            "--json",
            "stats",
            "ttest",
            "paired",
            "--data",
            path.to_str().unwrap(),
            "--before",
            "before",
            "--after",
            "after",
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

proptest! {
    #![proptest_config(ProptestConfig { cases: 32, failure_persistence: None, .. ProptestConfig::default() })]

    #[test]
    fn paired_t_sign_flip_negates_t_and_preserves_p(diffs in prop::collection::vec(-20i16..20i16, 4..24)) {
        prop_assume!(diffs.iter().any(|d| *d != 0));
        prop_assume!(diffs.iter().any(|d| *d != diffs[0]));

        let dir = tempfile::tempdir().unwrap();
        let positive = dir.path().join("positive.csv");
        let negative = dir.path().join("negative.csv");
        write_paired_csv(&positive, &diffs, 1.0);
        write_paired_csv(&negative, &diffs, -1.0);

        let left = run_paired(&positive);
        let right = run_paired(&negative);
        let n = diffs.len() as f64;
        prop_assert_eq!(left["result"]["df"].as_f64(), Some(n - 1.0));
        prop_assert_eq!(right["result"]["df"].as_f64(), Some(n - 1.0));

        let t_left = left["result"]["t_statistic"].as_f64().unwrap();
        let t_right = right["result"]["t_statistic"].as_f64().unwrap();
        let p_left = left["result"]["p_value"].as_f64().unwrap();
        let p_right = right["result"]["p_value"].as_f64().unwrap();
        prop_assert!((t_left + t_right).abs() < 1e-10);
        prop_assert!((t_left.abs() - t_right.abs()).abs() < 1e-10);
        prop_assert!((p_left - p_right).abs() < 1e-10);
    }
}
