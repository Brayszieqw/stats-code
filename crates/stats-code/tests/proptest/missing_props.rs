use std::fs;
use std::process::Command;

use proptest::prelude::*;
use serde_json::Value;

fn stats_code_bin() -> &'static str {
    env!("CARGO_BIN_EXE_stats-code")
}

fn write_correlation_csv(path: &std::path::Path, mask: &[(bool, bool)], reversed: bool) {
    let mut out = String::from("x,y\n");
    let iter: Box<dyn Iterator<Item = (usize, &(bool, bool))>> = if reversed {
        Box::new(mask.iter().enumerate().rev())
    } else {
        Box::new(mask.iter().enumerate())
    };
    for (i, (keep_x, keep_y)) in iter {
        let x = if *keep_x {
            (100 + i as i32).to_string()
        } else {
            String::new()
        };
        let y = if *keep_y {
            (200 + 2 * i as i32).to_string()
        } else {
            "NA".to_string()
        };
        out.push_str(&format!("{x},{y}\n"));
    }
    fs::write(path, out).unwrap();
}

fn run_correlation(path: &std::path::Path) -> Value {
    let output = Command::new(stats_code_bin())
        .args([
            "--json",
            "stats",
            "correlation",
            "--data",
            path.to_str().unwrap(),
            "--x",
            "x",
            "--y",
            "y",
            "--method",
            "pearson",
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
    fn missing_row_filtering_is_row_permutation_invariant(mask in prop::collection::vec((any::<bool>(), any::<bool>()), 4..24)) {
        let complete = mask.iter().filter(|(x, y)| *x && *y).count();
        prop_assume!(complete >= 3);

        let dir = tempfile::tempdir().unwrap();
        let original = dir.path().join("original.csv");
        let reversed = dir.path().join("reversed.csv");
        write_correlation_csv(&original, &mask, false);
        write_correlation_csv(&reversed, &mask, true);

        let left = run_correlation(&original);
        let right = run_correlation(&reversed);
        prop_assert_eq!(left["result"]["n_used"].as_u64(), Some(complete as u64));
        prop_assert_eq!(right["result"]["n_used"].as_u64(), Some(complete as u64));
        let left_r = left["result"]["r"].as_f64().unwrap();
        let right_r = right["result"]["r"].as_f64().unwrap();
        prop_assert!((left_r - right_r).abs() < 1e-10);
    }
}
