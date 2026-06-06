//! CLI integration tests for Phase 2 methods (tasks 17.4, 18.3, 19.3, 20.3, 21.3, 22.3, 23.3, 24.3, 25.3).

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
// 17.4 — Post-hoc CLI
// =========================================================================

#[test]
fn posthoc_cli_emits_snapshot_shape() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("posthoc.csv");
    fs::write(
        &data,
        "group,value\nA,10\nA,12\nA,11\nA,13\nB,20\nB,22\nB,21\nB,23\nC,15\nC,17\nC,16\nC,18\n",
    )
    .unwrap();

    let json = run_json(&[
        "--json", "stats", "anova", "posthoc",
        "--data", data.to_str().unwrap(),
        "--var", "value", "--group", "group",
        "--method", "bonferroni",
    ]);
    let result = &json["result"];
    assert_eq!(json["status"].as_str(), Some("ok"));
    assert_eq!(json["command"].as_str(), Some("stats.anova.posthoc"));
    assert_eq!(result["method"].as_str(), Some("bonferroni"));
    assert_eq!(result["pairs"].as_array().unwrap().len(), 3);
}

// =========================================================================
// 18.3 — Repeated-measures ANOVA CLI
// =========================================================================

#[test]
fn repeated_anova_cli_emits_snapshot_shape() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("repeated.csv");
    // Use 5 subjects with 3 complete time points each to ensure all are included
    fs::write(
        &data,
        "subject,time,value\n\
         S1,T1,10\nS1,T2,12\nS1,T3,14\n\
         S2,T1,11\nS2,T2,13\nS2,T3,16\n\
         S3,T1,9\nS3,T2,11\nS3,T3,13\n\
         S4,T1,12\nS4,T2,14\nS4,T3,17\n\
         S5,T1,8\nS5,T2,10\nS5,T3,12\n",
    )
    .unwrap();

    let json = run_json(&[
        "--json", "stats", "anova", "repeated",
        "--data", data.to_str().unwrap(),
        "--var", "value", "--subject", "subject", "--time", "time",
    ]);
    let result = &json["result"];
    assert_eq!(json["status"].as_str(), Some("ok"));
    assert_eq!(json["command"].as_str(), Some("stats.anova.repeated"));
    assert!(result["n_subjects"].as_u64().unwrap() >= 3);
    assert_eq!(result["n_timepoints"].as_u64(), Some(3));
    assert_eq!(result["time_df1"].as_u64(), Some(2));
    assert!(result["time_f"].as_f64().unwrap() > 1.0);
    assert!(result["time_p"].as_f64().unwrap() < 0.05);
}

// =========================================================================
// 19.3 — Poisson GLM CLI
// =========================================================================

#[test]
fn poisson_glm_cli_emits_snapshot_shape() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("poisson.csv");
    // Simple count data without offset — converges reliably
    fs::write(
        &data,
        "y,x1\n2,1\n3,2\n5,3\n4,2\n6,4\n8,5\n7,4\n9,6\n3,1\n5,3\n",
    )
    .unwrap();

    let json = run_json(&[
        "--json", "stats", "model", "poisson",
        "--data", data.to_str().unwrap(),
        "--y", "y",
        "--x", "x1",
    ]);
    let result = &json["result"];
    assert_eq!(json["status"].as_str(), Some("ok"));
    assert_eq!(json["command"].as_str(), Some("stats.model.poisson"));
    assert!(result["converged"].as_bool().unwrap());
    assert!(result["coefficients"].as_array().unwrap().len() >= 2);
    assert!(result["deviance"].as_f64().is_some());
    assert!(result["log_likelihood"].as_f64().is_some());
}

// =========================================================================
// 20.3 — Dose-response CLI
// =========================================================================

#[test]
fn dose_response_cli_emits_snapshot_shape() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("dose.csv");
    fs::write(
        &data,
        "dose,events,person_time\nnone,5,1000\nlow,8,900\nmedium,15,800\nhigh,25,700\n",
    )
    .unwrap();

    let json = run_json(&[
        "--json", "stats", "epi", "dose-response",
        "--data", data.to_str().unwrap(),
        "--exposure", "dose",
        "--outcome", "events",
        "--person-time", "person_time",
    ]);
    let result = &json["result"];
    assert_eq!(json["status"].as_str(), Some("ok"));
    assert_eq!(json["command"].as_str(), Some("stats.epi.dose_response"));
    assert!(result["categories"].as_array().unwrap().len() >= 3);
    assert!(result["trend_p_value"].as_f64().is_some());
    assert!(result["trend_beta"].as_f64().is_some());
}

// =========================================================================
// 21.3 — Meta-analysis CLI
// =========================================================================

#[test]
fn meta_analysis_cli_emits_snapshot_shape() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("meta.csv");
    fs::write(
        &data,
        "study,effect,se\nStudy1,0.5,0.2\nStudy2,0.8,0.3\nStudy3,0.3,0.15\nStudy4,0.6,0.25\nStudy5,1.0,0.4\n",
    )
    .unwrap();

    let json = run_json(&[
        "--json", "stats", "meta",
        "--data", data.to_str().unwrap(),
        "--effect", "effect",
        "--se", "se",
        "--study-label", "study",
    ]);
    let result = &json["result"];
    assert_eq!(json["status"].as_str(), Some("ok"));
    assert_eq!(json["command"].as_str(), Some("stats.meta"));
    assert_eq!(result["studies"].as_array().unwrap().len(), 5);
    assert!(result["fixed_effect"].as_f64().is_some());
    assert!(result["random_effect"].as_f64().is_some());
    assert!(result["q_statistic"].as_f64().is_some());
    assert!(result["i_squared"].as_f64().is_some());
    assert!(result["tau_squared"].as_f64().is_some());
}

// =========================================================================
// 22.3 — Kappa CLI
// =========================================================================

#[test]
fn kappa_cli_emits_snapshot_shape() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("kappa.csv");
    let mut csv = String::from("rater1,rater2\n");
    for _ in 0..20 { csv.push_str("yes,yes\n"); }
    for _ in 0..5 { csv.push_str("yes,no\n"); }
    for _ in 0..10 { csv.push_str("no,yes\n"); }
    for _ in 0..15 { csv.push_str("no,no\n"); }
    fs::write(&data, &csv).unwrap();

    let json = run_json(&[
        "--json", "stats", "agreement", "kappa",
        "--data", data.to_str().unwrap(),
        "--rater1", "rater1",
        "--rater2", "rater2",
    ]);
    let result = &json["result"];
    assert_eq!(json["status"].as_str(), Some("ok"));
    assert_eq!(json["command"].as_str(), Some("stats.agreement.kappa"));
    assert!(result["kappa"].as_f64().is_some());
    let k = result["kappa"].as_f64().unwrap();
    assert!(k > 0.0 && k < 1.0, "kappa={k}");
}

// =========================================================================
// 23.3 — Bland-Altman CLI
// =========================================================================

#[test]
fn bland_altman_cli_emits_snapshot_shape() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("ba.csv");
    fs::write(
        &data,
        "method1,method2\n100,102\n105,104\n110,112\n115,113\n120,121\n125,127\n130,129\n135,136\n140,138\n145,146\n",
    )
    .unwrap();

    let json = run_json(&[
        "--json", "stats", "agreement", "bland-altman",
        "--data", data.to_str().unwrap(),
        "--method1", "method1",
        "--method2", "method2",
    ]);
    let result = &json["result"];
    assert_eq!(json["status"].as_str(), Some("ok"));
    assert_eq!(json["command"].as_str(), Some("stats.agreement.bland_altman"));
    assert!(result["bias"].as_f64().is_some());
    assert!(result["sd_difference"].as_f64().is_some());
    assert!(result["loa_lower"].as_f64().is_some());
    assert!(result["loa_upper"].as_f64().is_some());
    assert_eq!(result["points"].as_array().unwrap().len(), 10);
}

// =========================================================================
// 24.3 — PCA CLI
// =========================================================================

#[test]
fn pca_cli_emits_snapshot_shape() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("pca.csv");
    fs::write(
        &data,
        "x1,x2,x3\n1.0,2.0,3.0\n2.0,3.0,5.0\n3.0,5.0,7.0\n4.0,6.0,9.0\n5.0,8.0,11.0\n6.0,9.0,13.0\n7.0,11.0,15.0\n8.0,12.0,17.0\n",
    )
    .unwrap();

    let json = run_json(&[
        "--json", "stats", "multivariate", "pca",
        "--data", data.to_str().unwrap(),
        "--vars", "x1,x2,x3",
    ]);
    let result = &json["result"];
    assert_eq!(json["status"].as_str(), Some("ok"));
    assert_eq!(json["command"].as_str(), Some("stats.multivariate.pca"));
    assert!(!result["components"].as_array().unwrap().is_empty());
    // KMO may be null for small/collinear datasets
    assert!(result["components"].as_array().unwrap()[0]["eigenvalue"].as_f64().is_some());
}

// =========================================================================
// 25.3 — Sample size log-rank CLI
// =========================================================================

#[test]
fn sample_size_logrank_cli_emits_snapshot_shape() {
    let json = run_json(&[
        "--json", "stats", "sample-size", "log-rank",
        "--median1", "12",
        "--median2", "18",
        "--accrual", "24",
        "--followup", "12",
        "--power", "0.8",
    ]);
    let result = &json["result"];
    assert_eq!(json["status"].as_str(), Some("ok"));
    assert_eq!(json["command"].as_str(), Some("stats.sample_size.log_rank"));
    assert!(result["total_n"].as_u64().unwrap() > 100);
    assert!(result["effect_size"].as_f64().is_some());
    assert!(result["power"].as_f64().is_some());
}
