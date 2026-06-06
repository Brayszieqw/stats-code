pub mod bridge;
/// Internal math module exposed for integration testing only.
#[doc(hidden)]
pub mod math;
/// Release packaging helpers (Feature: single-command-launcher).
pub mod release;

/// Live Stats Code release version, embedded at compile time from
/// `CARGO_PKG_VERSION` via `build.rs::emit_release_version`
/// (Feature: parity-and-multilang-sidecar, task 15.3).
///
/// Surfaced for:
///
/// - the Equivalent Code Sidecar's `<SidecarFooter>` (Requirement 1.7),
/// - the Audit Snapshot's `manifest.json::stats_code_release_version`
///   (Requirement 7.3),
/// - `versions.json` headers and any other place the binary needs to
///   self-identify its release without re-reading `CARGO_PKG_VERSION`
///   at every call site.
///
/// The on-disk artifact at `$OUT_DIR/release_version.txt` is written
/// without a trailing newline by `build.rs`, so the embedded constant
/// equals the version string byte-for-byte (no `.trim_end()` needed).
pub const RELEASE_VERSION: &str =
    include_str!(concat!(env!("OUT_DIR"), "/release_version.txt"));

/// Git commit SHA at build time, embedded from `build.rs::emit_commit_sha`
/// (Feature: sidecar-snapshot-integration, Requirement 5.8 / task 8.1).
///
/// Surfaced for:
///
/// - the `RunEnvironment` recorded by the Run-State Store so each
///   Analysis Run carries the exact source revision that produced it,
/// - the Audit Snapshot's `versions.json::commit_sha`.
///
/// The value is the full 40-character hex SHA from `git rev-parse HEAD`,
/// or the literal `"unknown"` when git is unavailable at build time.
/// Written without a trailing newline by `build.rs`, so the embedded
/// constant equals the SHA string byte-for-byte.
pub const COMMIT_SHA: &str =
    include_str!(concat!(env!("OUT_DIR"), "/commit_sha.txt"));

/// Build-time snapshot of `stats-code`'s direct runtime dependency
/// versions, materialized by `build.rs::emit_runtime_deps`
/// (Feature: parity-and-multilang-sidecar, tasks 1.3 / 15.3).
///
/// Format: a flat JSON object mapping package name to version string,
/// keys in lexicographic order, two-space indent, LF newlines, trailing
/// newline. Consumed by [`snapshot::versions::build_versions`] under
/// `versions.json::runtime_dependencies` (Requirement 7.4).
///
/// Surfaced as `pub` here so callers outside the snapshot module (e.g.
/// future agent-server diagnostic endpoints) can read the same canonical
/// snapshot the exporter sees.
pub const RUNTIME_DEPS_JSON: &str =
    include_str!(concat!(env!("OUT_DIR"), "/runtime_deps.json"));

/// Stats Code Launcher 模块树（Feature: single-command-launcher，task 1.2）。
pub mod launcher;
/// Algorithm Coverage Matrix — single source of truth for parity coverage
/// (Feature: parity-and-multilang-sidecar, task 1.1).
/// Wave-0 skeleton: data types + embedded `matrix.toml`. Parser / loader land
/// in task 1.2; `release_version` injection from `build.rs` lands in task 1.3.
pub mod coverage_matrix;
/// Shared secret-and-path redaction policy
/// (Feature: parity-and-multilang-sidecar, task 2.4).
///
/// `redact_pure(text, &RedactionPolicy)` is the canonical implementation
/// re-exported by both `sidecar::redact` and `snapshot::redact` so the two
/// surfaces share a single deterministic rewriter
/// (Requirements 2.6, 9.1, 9.3, 9.4, 9.5).
pub mod redact;
/// `SpawnPolicy::forbid_external_runtimes()` 哨兵
/// (Feature: parity-and-multilang-sidecar, task 2.6).
///
/// 在 sidecar snippet 生成、SPA 渲染、Audit Snapshot 导出三条流水线的调用
/// 栈外面套一道运行时闸门：命中 `{R, Rscript, python, python3, pythonw,
/// sas, spss, pspp, pspp-cli, statistics, stats}` 命令名或对应共享库时返回
/// 结构化 `ForbiddenSpawn` 错误并立即中止
/// (Requirements 10.1, 10.2, 10.5)。Launcher 路径不在 scope 内
/// (Requirement 10.4)，仍由 `crate::launcher::browser` 自己管理。
pub mod spawn_policy;
/// Sidecar Code Generator 模块树（Feature: parity-and-multilang-sidecar，task 2.1）。
/// Wave-0 骨架；纯函数实现见 task 2.2–2.6。
pub mod sidecar;
/// Audit Snapshot Exporter 模块树（Feature: parity-and-multilang-sidecar，task 6.1）。
/// Wave-0 骨架；实现分散到 task 5.1（workflow_yaml）、6.2–6.7、7.2。
pub mod snapshot;
/// Parity validation subcommand support
/// (Feature: parity-and-multilang-sidecar, tasks 9.1 / 9.2 / 9.3).
///
/// Wave-1 lands only `parity::tolerance` — the per-algorithm Parity
/// Threshold loader for `validation/tolerance_config.yaml` (Requirement
/// 12.1, 12.2, 12.3, 12.6). The `parity::run_local` driver and the CLI
/// `Command::Parity` variant ship in tasks 9.1 / 9.2.
pub mod parity;
mod cli;
mod config;
mod cox;
mod diagnostic;
mod error;
mod handlers;
mod helpers;
mod linear;
mod logistic;
mod modeling;
mod power;
mod rate;
mod render;
mod report;
mod schema;
mod stats;
mod survival;
mod tableone;

pub use bridge::Engine;
pub use cli::{
    AiAskArgs, AiCommand, AuditCommand, AuditExplainArgs, AuthCommand, AuthDoctorArgs,
    AuthProvider, AuthSetArgs, CheckArgs, Cli, Command, ConfigCommand, ConfigModelArgs,
    DiagnosticCommand, DiagnosticRocArgs, InspectArgs, ModelCommand, ModelCoxArgs, ModelLinearArgs,
    ModelLogisticArgs, NaStrategy, OpenCommand, OpenReportArgs, ParityArgs, PlanArgs, PowerCommand,
    PowerOneProportionArgs, PowerTwoMeansArgs, PowerTwoProportionsArgs, RateArgs, ReplayArgs,
    ReportBuildArgs, ReportCommand, ReportVerifyArgs, RunCommand, RunScriptArgs, StatsCommand,
    SurvivalCommand, SurvivalKmArgs, TableOneArgs, WorkflowCommand, WorkflowRunArgs,
};
pub use error::{StatsCodeError, StatsCodeResult};
pub use handlers::{dispatch, run};
pub use schema::{
    AnalysisCheckResult, AnalysisSpec, DataFormat, ReportVerifyResult, WorkflowRunResult,
    // Study-context validation boundary (R8 / engineering-quality-hardening)
    validate_study_context, ClusteringUnit, MissingDataStrategy,
    // Statistical methods result types (task 3.1)
    TtestPairedResult, TtestOneSampleResult,
    AnovaGroupSummary, OneWayAnovaResult, RbdAnovaResult, RepeatedAnovaResult,
    PosthocPair, PosthocResult,
    McNemarResult, WilcoxonSignedRankResult, MannWhitneyResult,
    CategoryProportion, CochranArmitageResult,
    CorrelationResult,
    NormalityResult,
    GroupVarianceSummary, VarianceHomogeneityResult,
    TwoByTwoCells, MhStratum, OrRrResult,
    StandardizationStratum, StandardizationResult,
    AttributableRiskResult,
    DoseResponseCategory, DoseResponseResult,
    PoissonCoefficient, PoissonResult,
    MultinomialCoefficientGroup, OrdinalLogitResult, MultinomialLogitResult,
    MixedFixedEffect, MixedLmmResult,
    LifeTableRow, LifeTableResult,
    CompetingRisksCauseFit, CompetingRisksCif, CompetingRisksResult,
    PowerLogRankResult,
    MetaStudy, MetaAnalysisResult,
    KappaResult,
    BlandAltmanPoint, BlandAltmanResult,
    PcaComponent, PcaResult,
    LdaResult,
    ClusterAssignment, ClusterResult,
    PsmCovariateSmd, PsmResult,
};

#[cfg(test)]
mod release_version_tests {
    //! Tests for the build-time-injected `RELEASE_VERSION` and
    //! `RUNTIME_DEPS_JSON` constants (Feature:
    //! parity-and-multilang-sidecar, task 15.3 / Requirements 1.7, 7.3,
    //! 7.4).
    //!
    //! These are not property-based — the constants are deterministic
    //! function of `Cargo.toml` and `Cargo.lock`, so a small set of
    //! direct shape assertions is sufficient.

    use super::{RELEASE_VERSION, RUNTIME_DEPS_JSON};

    /// `RELEASE_VERSION` must equal the live `CARGO_PKG_VERSION` at the
    /// time the test binary itself was compiled. Because `build.rs`
    /// writes `OUT_DIR/release_version.txt` from `CARGO_PKG_VERSION` and
    /// the test binary's own `env!("CARGO_PKG_VERSION")` resolves the
    /// same value, the two must be byte-identical.
    #[test]
    fn release_version_matches_cargo_pkg_version() {
        assert_eq!(RELEASE_VERSION, env!("CARGO_PKG_VERSION"));
    }

    /// The constant must be free of trailing whitespace / newlines so
    /// downstream callers (footer rendering, manifest builder) don't have
    /// to `.trim_end()` a `&'static str` (impossible in `const` context).
    #[test]
    fn release_version_has_no_trailing_whitespace() {
        assert_eq!(RELEASE_VERSION, RELEASE_VERSION.trim_end());
        assert!(!RELEASE_VERSION.is_empty());
        assert!(!RELEASE_VERSION.contains('\n'));
        assert!(!RELEASE_VERSION.contains('\r'));
    }

    /// `RUNTIME_DEPS_JSON` must parse as a flat JSON object of
    /// `String → String`. The snapshot exporter relies on this shape
    /// (`snapshot::versions::build_versions` panics otherwise); pinning
    /// the assertion here gives a clearer failure if `build.rs`
    /// regresses than waiting for a snapshot integration test to fail.
    #[test]
    fn runtime_deps_json_parses_as_string_string_map() {
        let parsed: std::collections::BTreeMap<String, String> =
            serde_json::from_str(RUNTIME_DEPS_JSON)
                .expect("RUNTIME_DEPS_JSON must be a JSON object of String -> String");
        // The build script always inserts the crate itself as a baseline
        // entry, even if `Cargo.lock` somehow listed no deps.
        assert!(
            parsed.contains_key("stats-code"),
            "runtime_deps.json must contain at least the `stats-code` self-entry; got keys: {:?}",
            parsed.keys().collect::<Vec<_>>()
        );
        // The self-entry's version must match the live release.
        assert_eq!(parsed.get("stats-code").map(String::as_str), Some(RELEASE_VERSION));
    }

    /// The matrix and the standalone version artifact must agree —
    /// both are derived from `CARGO_PKG_VERSION` by `build.rs`, so any
    /// drift indicates one of the two emit paths regressed.
    #[test]
    fn release_version_agrees_with_coverage_matrix() {
        let matrix = crate::coverage_matrix::CoverageMatrix::get_loaded();
        assert_eq!(matrix.release_version(), RELEASE_VERSION);
    }
}
