use std::fs;
use std::process::Command;

use proptest::prelude::*;
use serde_json::Value;

fn stats_code_bin() -> &'static str {
    env!("CARGO_BIN_EXE_stats-code")
}

fn write_anova_csv(path: &std::path::Path, groups: &[(&str, Vec<i16>)]) {
    let mut out = String::from("group,value\n");
    for (group, values) in groups {
        for value in values {
            let shifted = 20.0 + f64::from(*value);
            out.push_str(&format!("{group},{shifted}\n"));
        }
    }
    fs::write(path, out).unwrap();
}

fn run_anova(path: &std::path::Path) -> Value {
    let output = Command::new(stats_code_bin())
        .args([
            "--json",
            "stats",
            "anova",
            "oneway",
            "--data",
            path.to_str().unwrap(),
            "--var",
            "value",
            "--group",
            "group",
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
    fn oneway_anova_ss_total_decomposes(
        a in prop::collection::vec(0i16..40i16, 2..10),
        b in prop::collection::vec(0i16..40i16, 2..10),
        c in prop::collection::vec(0i16..40i16, 2..10),
    ) {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("anova.csv");
        write_anova_csv(&data, &[("A", a), ("B", b), ("C", c)]);

        let json = run_anova(&data);
        let result = &json["result"];
        let ss_total = result["ss_total"].as_f64().unwrap();
        let ss_between = result["ss_between"].as_f64().unwrap();
        let ss_within = result["ss_within"].as_f64().unwrap();

        prop_assert!((ss_total - ss_between - ss_within).abs() < 1e-8);
    }
}
