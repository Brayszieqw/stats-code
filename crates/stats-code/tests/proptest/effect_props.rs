use std::fs;
use std::process::Command;

use proptest::prelude::*;
use serde_json::Value;

fn stats_code_bin() -> &'static str {
    env!("CARGO_BIN_EXE_stats-code")
}

fn write_2x2_csv(path: &std::path::Path, counts: [usize; 4]) {
    let mut out = String::from("exposure,outcome\n");
    for (exposure, outcome, n) in [
        ("1", "1", counts[0]),
        ("1", "0", counts[1]),
        ("0", "1", counts[2]),
        ("0", "0", counts[3]),
    ] {
        for _ in 0..n {
            out.push_str(&format!("{exposure},{outcome}\n"));
        }
    }
    fs::write(path, out).unwrap();
}

fn run_or_rr(path: &std::path::Path) -> Value {
    let output = Command::new(stats_code_bin())
        .args([
            "--json",
            "stats",
            "epi",
            "or-rr",
            "--data",
            path.to_str().unwrap(),
            "--exposure",
            "exposure",
            "--outcome",
            "outcome",
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

fn assert_finite_effects(result: &Value) {
    for key in [
        "odds_ratio",
        "or_ci_lower",
        "or_ci_upper",
        "relative_risk",
        "rr_ci_lower",
        "rr_ci_upper",
        "chi_square",
        "chi_p_value",
    ] {
        assert!(result[key].as_f64().unwrap().is_finite(), "{key}");
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 16, failure_persistence: None, .. ProptestConfig::default() })]

    #[test]
    fn continuity_correction_keeps_effect_estimates_finite(
        a in 0usize..12,
        b in 0usize..12,
        c in 0usize..12,
        d in 0usize..12,
    ) {
        let counts = [a, b, c, d];
        let total: usize = counts.iter().sum();
        prop_assume!(total > 0);

        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("or_rr.csv");
        write_2x2_csv(&data, counts);

        let json = run_or_rr(&data);
        let result = &json["result"];
        prop_assert_eq!(result["n_used"].as_u64(), Some(total as u64));
        prop_assert_eq!(
            result["continuity_correction"].as_bool(),
            Some(counts.contains(&0))
        );
        assert_finite_effects(result);
    }
}
