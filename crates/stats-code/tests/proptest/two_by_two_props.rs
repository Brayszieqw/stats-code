use std::fs;
use std::process::Command;

use proptest::prelude::*;
use serde_json::Value;

fn stats_code_bin() -> &'static str {
    env!("CARGO_BIN_EXE_stats-code")
}

fn run_single_row_or_rr(exposure: bool, outcome: bool) -> Value {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("single.csv");
    fs::write(
        &data,
        format!(
            "exposure,outcome\n{},{}\n",
            if exposure { "1" } else { "0" },
            if outcome { "1" } else { "0" }
        ),
    )
    .unwrap();

    let output = Command::new(stats_code_bin())
        .args([
            "--json",
            "stats",
            "epi",
            "or-rr",
            "--data",
            data.to_str().unwrap(),
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

proptest! {
    #![proptest_config(ProptestConfig { cases: 16, failure_persistence: None, .. ProptestConfig::default() })]

    #[test]
    fn single_row_2x2_tables_are_continuity_corrected(exposure in any::<bool>(), outcome in any::<bool>()) {
        let json = run_single_row_or_rr(exposure, outcome);
        let result = &json["result"];

        prop_assert_eq!(result["n_used"].as_u64(), Some(1));
        prop_assert_eq!(result["continuity_correction"].as_bool(), Some(true));
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
            prop_assert!(result[key].as_f64().unwrap().is_finite(), "{key}");
        }
    }
}
