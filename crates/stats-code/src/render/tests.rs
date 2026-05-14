use crate::schema::{
    AnalysisCheckItem, AnalysisCheckLevel, AuthSetResult, ColumnInspection, ConfigResult,
    CoxCoefficient, CoxResult, DiagnosticRocResult, DiagnosticThresholdMetrics, DoctorResult,
    InspectResult, LinearCoefficient, LinearResult, LogRankResult, LogisticCoefficient,
    LogisticResult, PowerResult, RateResult, RateRow, ReportBuildResult, ReportVerifyResult,
    RocPoint, SurvivalKmResult, SurvivalKmStep, TableOneResult, WorkflowRunResult,
    WorkflowStepRunResult,
};

use super::admin::{render_auth_set_text, render_config_text, render_doctor_text};
use super::data::{
    render_inspect_text, render_power_text, render_rate_text, render_survival_km_text,
    render_tableone_text,
};
use super::report::{
    render_report_build_text, render_report_verify_text, render_workflow_run_text,
};
use super::stats::{
    render_cox_text, render_diagnostic_roc_text, render_linear_text, render_logistic_text,
};

use crate::schema::DataFormat;
use crate::schema::VariableKind;

#[test]
fn test_render_logistic_text() {
    let result = LogisticResult {
        status: "success".into(),
        validity_status: String::new(),
        data_path: "data.csv".into(),
        analysis_path: Some("analysis.yaml".into()),
        formula: "outcome ~ age + sex".into(),
        outcome: "outcome".into(),
        predictors: vec!["age".into(), "sex".into()],
        survey_weight: None,
        n_total: 100,
        n_used: 95,
        n_excluded_missing: 5,
        n_excluded_invalid: 0,
        n_events: 40,
        n_nonevents: 55,
        iterations: 6,
        converged: true,
        log_likelihood: -50.123,
        null_log_likelihood: Some(-60.0),
        pseudo_r2_nagelkerke: Some(0.25),
        aic: Some(106.25),
        bic: Some(112.0),
        c_statistic: Some(0.78),
        coefficients: vec![LogisticCoefficient {
            term: "age".into(),
            variable: "age".into(),
            level: None,
            reference: None,
            beta: 0.05,
            standard_error: 0.02,
            odds_ratio: 1.051,
            ci_lower: 1.01,
            ci_upper: 1.09,
            p_value: 0.012,
        }],
        notes: vec![],
        diagnostics: vec![],
        warnings: vec![],
    };
    let output = render_logistic_text(&result);
    assert!(output.contains("Logistic Model"));
    assert!(output.contains("success"));
    assert!(output.contains("data.csv"));
    assert!(output.contains("outcome ~ age + sex"));
    assert!(output.contains("converged=true"));
    assert!(output.contains("age"));
    assert!(output.contains("OR=1.0510"));
}

#[test]
fn test_render_cox_text() {
    let result = CoxResult {
        status: "success".into(),
        data_path: "survival.csv".into(),
        analysis_path: None,
        formula: "Surv(time, event) ~ treatment".into(),
        time: "time".into(),
        event: "event".into(),
        predictors: vec!["treatment".into()],
        survey_weight: None,
        n_total: 200,
        n_used: 190,
        n_excluded_missing: 10,
        n_excluded_invalid: 0,
        n_events: 80,
        n_censored: 110,
        tied_event_times: 5,
        iterations: 4,
        converged: true,
        log_partial_likelihood: -300.5,
        concordance: Some(0.65),
        coefficients: vec![CoxCoefficient {
            term: "treatment".into(),
            variable: "treatment".into(),
            level: None,
            reference: None,
            beta: -0.3,
            standard_error: 0.15,
            hazard_ratio: 0.741,
            ci_lower: 0.55,
            ci_upper: 0.99,
            p_value: 0.045,
        }],
        ph_diagnostics: vec![],
        notes: vec![],
        warnings: vec![],
    };
    let output = render_cox_text(&result);
    assert!(output.contains("Cox Model"));
    assert!(output.contains("survival.csv"));
    assert!(output.contains("Surv(time, event) ~ treatment"));
    assert!(output.contains("converged=true"));
    assert!(output.contains("HR=0.7410"));
    assert!(output.contains("Concordance"));
}

#[test]
fn test_render_linear_text() {
    let result = LinearResult {
        status: "success".into(),
        data_path: "regression.csv".into(),
        analysis_path: None,
        formula: "y ~ x1 + x2".into(),
        outcome: "y".into(),
        predictors: vec!["x1".into(), "x2".into()],
        survey_weight: None,
        n_total: 50,
        n_used: 50,
        n_excluded_missing: 0,
        n_excluded_invalid: 0,
        converged: true,
        r_squared: 0.85,
        adjusted_r_squared: 0.84,
        f_statistic: Some(120.5),
        f_p_value: Some(0.0001),
        residual_std_error: 2.3,
        aic: Some(200.0),
        bic: Some(210.0),
        coefficients: vec![LinearCoefficient {
            term: "x1".into(),
            variable: "x1".into(),
            level: None,
            reference: None,
            beta: 3.5,
            standard_error: 0.5,
            t_statistic: 7.0,
            ci_lower: 2.5,
            ci_upper: 4.5,
            p_value: 0.0001,
        }],
        notes: vec![],
        warnings: vec![],
    };
    let output = render_linear_text(&result);
    assert!(output.contains("Linear Model"));
    assert!(output.contains("regression.csv"));
    assert!(output.contains("y ~ x1 + x2"));
    assert!(output.contains("R²=0.8500"));
    assert!(output.contains("x1"));
    assert!(output.contains("beta=3.5000"));
}

#[test]
fn test_render_diagnostic_roc_text() {
    let youden = DiagnosticThresholdMetrics {
        threshold: 0.5,
        tp: 30,
        fp: 10,
        tn: 50,
        fn_count: 10,
        sensitivity: 0.75,
        specificity: 0.833,
        ppv: 0.75,
        npv: 0.833,
        accuracy: 0.8,
        balanced_accuracy: 0.79,
        f1_score: 0.75,
        positive_likelihood_ratio: Some(4.5),
        negative_likelihood_ratio: Some(0.3),
        diagnostic_odds_ratio: Some(15.0),
        youden_j: 0.583,
    };
    let result = DiagnosticRocResult {
        status: "success".into(),
        data_path: "diag.csv".into(),
        analysis_path: None,
        truth: "disease".into(),
        score: "marker".into(),
        n_total: 100,
        n_used: 100,
        n_excluded_missing: 0,
        n_excluded_invalid: 0,
        n_cases: 40,
        n_controls: 60,
        auc: 0.85,
        roc_points: vec![RocPoint {
            threshold: 0.5,
            sensitivity: 0.75,
            specificity: 0.833,
            false_positive_rate: 0.167,
            true_positive_rate: 0.75,
        }],
        youden,
        threshold_metrics: None,
        notes: vec![],
        warnings: vec![],
    };
    let output = render_diagnostic_roc_text(&result);
    assert!(output.contains("Diagnostic ROC"));
    assert!(output.contains("diag.csv"));
    assert!(output.contains("AUC"));
    assert!(output.contains("0.8500"));
    assert!(output.contains("disease"));
    assert!(output.contains("marker"));
}

#[test]
fn test_render_inspect_text() {
    let result = InspectResult {
        status: "success".into(),
        data_path: "dataset.csv".into(),
        format: DataFormat::Csv,
        rows: Some(1000),
        columns: 5,
        variables: vec![ColumnInspection {
            name: "age".into(),
            inferred_kind: VariableKind::Continuous,
            missing_count: 2,
            non_missing_count: 998,
            distinct_count: 80,
            sample_values: vec!["25".into(), "30".into(), "45".into()],
            numeric_summary: None,
            warnings: vec![],
        }],
        notes: vec![],
    };
    let output = render_inspect_text(&result);
    assert!(output.contains("Inspect"));
    assert!(output.contains("dataset.csv"));
    assert!(output.contains("age"));
    assert!(output.contains("continuous"));
    assert!(output.contains("missing=2"));
}

#[test]
fn test_render_tableone_text() {
    let result = TableOneResult {
        status: "success".into(),
        data_path: "trial.csv".into(),
        analysis_path: None,
        by: "group".into(),
        survey_weight: None,
        group_levels: vec!["control".into(), "treatment".into()],
        rows: vec![],
        notes: vec!["Sample table".into()],
    };
    let output = render_tableone_text(&result);
    assert!(output.contains("Table 1"));
    assert!(output.contains("trial.csv"));
    assert!(output.contains("group"));
    assert!(output.contains("control, treatment"));
    assert!(output.contains("Sample table"));
}

#[test]
fn test_render_rate_text() {
    let result = RateResult {
        status: "success".into(),
        data_path: "events.csv".into(),
        analysis_path: None,
        event: "death".into(),
        person_time: "follow_up_years".into(),
        strata: vec![],
        survey_weight: None,
        rows: vec![RateRow {
            stratum: "overall".into(),
            total_records: 500,
            included_records: 480,
            events: 25.0,
            person_time: 1200.0,
            rate: 0.0208,
            rate_per_1000: 20.833,
            lower_ci_per_1000: 13.5,
            upper_ci_per_1000: 30.7,
        }],
        notes: vec![],
    };
    let output = render_rate_text(&result);
    assert!(output.contains("Rate"));
    assert!(output.contains("events.csv"));
    assert!(output.contains("death"));
    assert!(output.contains("follow_up_years"));
    assert!(output.contains("overall"));
}

#[test]
fn test_render_survival_km_text() {
    let result = SurvivalKmResult {
        status: "success".into(),
        data_path: "surv.csv".into(),
        analysis_path: None,
        time: "months".into(),
        event: "death".into(),
        group: Some("arm".into()),
        n_total: 100,
        n_used: 95,
        n_excluded_missing: 5,
        n_excluded_invalid: 0,
        groups: vec!["A".into(), "B".into()],
        steps: vec![SurvivalKmStep {
            group: "A".into(),
            time: 6.0,
            n_risk: 50,
            n_event: 3,
            n_censored: 2,
            survival: 0.94,
            standard_error: 0.03,
            ci_lower: 0.88,
            ci_upper: 1.0,
        }],
        log_rank: Some(LogRankResult {
            chi_square: 4.5,
            degrees_freedom: 1,
            p_value: 0.034,
            groups: vec!["A".into(), "B".into()],
        }),
        notes: vec![],
        warnings: vec![],
    };
    let output = render_survival_km_text(&result);
    assert!(output.contains("Kaplan-Meier Survival"));
    assert!(output.contains("surv.csv"));
    assert!(output.contains("months"));
    assert!(output.contains("Log-rank"));
    assert!(output.contains("chi_square=4.5000"));
}

#[test]
fn test_render_power_text() {
    let result = PowerResult {
        status: "success".into(),
        method: "two_sample_t".into(),
        alpha: 0.05,
        power: Some(0.8),
        allocation_ratio: Some(1.0),
        total_n: 128,
        group1_n: Some(64),
        group2_n: Some(64),
        effect_size: Some(0.5),
        notes: vec![],
        warnings: vec![],
    };
    let output = render_power_text(&result);
    assert!(output.contains("Power / Sample Size"));
    assert!(output.contains("two_sample_t"));
    assert!(output.contains("0.0500"));
    assert!(output.contains("128"));
    assert!(output.contains("n1=64 n2=64"));
}

#[test]
fn test_render_report_build_text() {
    let result = ReportBuildResult {
        status: "success".into(),
        analysis_path: "study/analysis.yaml".into(),
        output_dir: "output/report".into(),
        written_files: vec!["report.html".into(), "report.pdf".into()],
        notes: vec![],
    };
    let output = render_report_build_text(&result);
    assert!(output.contains("Report Build"));
    assert!(output.contains("study/analysis.yaml"));
    assert!(output.contains("output/report"));
    assert!(output.contains("report.html"));
    assert!(output.contains("report.pdf"));
}

#[test]
fn test_render_report_verify_text() {
    let result = ReportVerifyResult {
        status: "success".into(),
        artifacts_dir: "artifacts/".into(),
        accepted_count: 3,
        rejected_count: 1,
        error_count: 0,
        warning_count: 1,
        items: vec![AnalysisCheckItem {
            level: AnalysisCheckLevel::Warning,
            code: "W001".into(),
            message: "Missing covariate adjustment".into(),
        }],
        notes: vec![],
    };
    let output = render_report_verify_text(&result);
    assert!(output.contains("Report Verify"));
    assert!(output.contains("artifacts/"));
    assert!(output.contains("accepted=3"));
    assert!(output.contains("rejected=1"));
    assert!(output.contains("WARNING"));
    assert!(output.contains("W001"));
}

#[test]
fn test_render_workflow_run_text() {
    let result = WorkflowRunResult {
        status: "success".into(),
        run_id: "run-001".into(),
        analysis_path: "analysis.yaml".into(),
        data_path: "data.csv".into(),
        artifacts_dir: "artifacts/run-001".into(),
        report_output_dir: "output/".into(),
        steps: vec![WorkflowStepRunResult {
            step_index: 0,
            command: "logistic".into(),
            artifact_dir: "artifacts/run-001/step-0".into(),
            status: "success".into(),
            notes: vec![],
        }],
        report: ReportBuildResult {
            status: "success".into(),
            analysis_path: "analysis.yaml".into(),
            output_dir: "output/".into(),
            written_files: vec![],
            notes: vec![],
        },
        notes: vec![],
    };
    let output = render_workflow_run_text(&result);
    assert!(output.contains("Workflow Run"));
    assert!(output.contains("run-001"));
    assert!(output.contains("analysis.yaml"));
    assert!(output.contains("logistic"));
    assert!(output.contains("#0"));
}

#[test]
fn test_render_auth_set_text() {
    let result = AuthSetResult {
        status: "success".into(),
        provider: "openai".into(),
        config_path: "/home/user/.config/stats-code/auth.json".into(),
        api_key_env: "OPENAI_API_KEY".into(),
        base_url_env: None,
        notes: vec![],
    };
    let output = render_auth_set_text(&result);
    assert!(output.contains("Auth Set"));
    assert!(output.contains("openai"));
    assert!(output.contains("OPENAI_API_KEY"));
    assert!(output.contains("/home/user/.config/stats-code/auth.json"));
}

#[test]
fn test_render_config_text() {
    let result = ConfigResult {
        status: "success".into(),
        action: "show".into(),
        config_path: "/home/user/.config/stats-code/config.json".into(),
        default_model: Some("gpt-4".into()),
        saved_models: vec!["gpt-4".into(), "claude-3".into()],
        message: "Configuration loaded".into(),
        notes: vec![],
    };
    let output = render_config_text(&result);
    assert!(output.contains("Config"));
    assert!(output.contains("show"));
    assert!(output.contains("gpt-4"));
    assert!(output.contains("claude-3"));
    assert!(output.contains("Configuration loaded"));
}

#[test]
fn test_render_doctor_text() {
    let result = DoctorResult {
        status: "success".into(),
        version: "0.5.0".into(),
        current_dir: "/home/user/project".into(),
        executable: "/usr/local/bin/stats-code".into(),
        error_count: 0,
        warning_count: 1,
        items: vec![AnalysisCheckItem {
            level: AnalysisCheckLevel::Warning,
            code: "D002".into(),
            message: "R not found in PATH".into(),
        }],
        notes: vec![],
    };
    let output = render_doctor_text(&result);
    assert!(output.contains("Doctor"));
    assert!(output.contains("0.5.0"));
    assert!(output.contains("/home/user/project"));
    assert!(output.contains("errors=0"));
    assert!(output.contains("warnings=1"));
    assert!(output.contains("WARNING"));
    assert!(output.contains("D002"));
    assert!(output.contains("R not found in PATH"));
}

#[test]
fn test_render_logistic_text_with_warnings() {
    let result = LogisticResult {
        status: "success".into(),
        validity_status: String::new(),
        data_path: "data.csv".into(),
        analysis_path: None,
        formula: "y ~ x".into(),
        outcome: "y".into(),
        predictors: vec!["x".into()],
        survey_weight: None,
        n_total: 50,
        n_used: 50,
        n_excluded_missing: 0,
        n_excluded_invalid: 0,
        n_events: 20,
        n_nonevents: 30,
        iterations: 10,
        converged: false,
        log_likelihood: -30.0,
        null_log_likelihood: None,
        pseudo_r2_nagelkerke: None,
        aic: None,
        bic: None,
        c_statistic: None,
        coefficients: vec![],
        notes: vec!["Model may be unstable".into()],
        diagnostics: vec![],
        warnings: vec!["Did not converge".into()],
    };
    let output = render_logistic_text(&result);
    assert!(output.contains("converged=false"));
    assert!(output.contains("Did not converge"));
    assert!(output.contains("Model may be unstable"));
}

#[test]
fn test_render_survival_km_text_no_log_rank() {
    let result = SurvivalKmResult {
        status: "success".into(),
        data_path: "km.csv".into(),
        analysis_path: None,
        time: "time".into(),
        event: "status".into(),
        group: None,
        n_total: 50,
        n_used: 50,
        n_excluded_missing: 0,
        n_excluded_invalid: 0,
        groups: vec!["overall".into()],
        steps: vec![SurvivalKmStep {
            group: "overall".into(),
            time: 12.0,
            n_risk: 50,
            n_event: 5,
            n_censored: 0,
            survival: 0.9,
            standard_error: 0.04,
            ci_lower: 0.82,
            ci_upper: 0.98,
        }],
        log_rank: None,
        notes: vec![],
        warnings: vec![],
    };
    let output = render_survival_km_text(&result);
    assert!(output.contains("Kaplan-Meier Survival"));
    assert!(output.contains("km.csv"));
    assert!(output.contains("survival=0.9000"));
    // No log-rank section when None
    assert!(!output.contains("Log-rank"));
}
