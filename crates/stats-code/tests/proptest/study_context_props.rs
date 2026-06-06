//! Property tests for the R8 validated study-context boundary.
//!
//! Covers three design properties over the pure parser/validator boundary:
//!
//! - **Property 2 (task 10.2)** — Study-context validation is total and
//!   idempotent. `parse` never panics and always yields a variant;
//!   `parse(value.as_token()) == value` for recognized variants.
//!   **Validates: Requirements 8.1, 8.2**
//!
//! - **Property 3 (task 10.4)** — Inconsistency detection is monotonic in
//!   declared structure: declaring a `Cluster` role while `clustering` parses
//!   to `None` yields an inconsistency naming the `clustering` field; removing
//!   the contradiction removes exactly that one issue.
//!   **Validates: Requirements 8.2, 8.3**
//!
//! - **Property 1 (task 10.5)** — Validation preserves numeric behavior for
//!   already-valid inputs: for any consistent spec, validation adds no
//!   inconsistency issue and the run proceeds unchanged.
//!   **Validates: Requirements 8.4, 8.5, 9.1**

use proptest::prelude::*;

use stats_code::{validate_study_context, AnalysisSpec, ClusteringUnit, MissingDataStrategy};

// ─── Property 2 (task 10.2): parser totality + idempotence ────────────────

proptest! {
    #![proptest_config(ProptestConfig { cases: 256, failure_persistence: None, .. ProptestConfig::default() })]

    /// `MissingDataStrategy::parse` is total: it never panics for ANY input
    /// and always yields a variant.
    #[test]
    fn missing_data_strategy_parse_is_total(raw in ".*") {
        let _variant = MissingDataStrategy::parse(&raw); // must not panic
    }

    /// `ClusteringUnit::parse` is total for ANY input.
    #[test]
    fn clustering_unit_parse_is_total(raw in ".*") {
        let _variant = ClusteringUnit::parse(&raw); // must not panic
    }

    /// Idempotence / round-trip for recognized `MissingDataStrategy` variants:
    /// `parse(value.as_token()) == value`.
    #[test]
    fn missing_data_strategy_round_trips(choice in 0u8..4, label in "[a-z][a-z0-9_]{0,12}") {
        let value = match choice {
            0 => MissingDataStrategy::CompleteCase,
            1 => MissingDataStrategy::AvailableCase,
            2 => MissingDataStrategy::Imputation(format!("imputation_{label}")),
            _ => MissingDataStrategy::Other(format!("other_{label}")),
        };
        let reparsed = MissingDataStrategy::parse(&value.as_token());
        prop_assert_eq!(reparsed, value);
    }

    /// Idempotence / round-trip for recognized `ClusteringUnit` variants.
    #[test]
    fn clustering_unit_round_trips(choice in 0u8..3, label in "[a-z][a-z0-9_]{0,12}") {
        let value = match choice {
            0 => ClusteringUnit::None,
            1 => ClusteringUnit::Individual,
            // A named unit must not collide with the none/individual tokens.
            _ => ClusteringUnit::Named(format!("site_{label}")),
        };
        let reparsed = ClusteringUnit::parse(&value.as_token());
        prop_assert_eq!(reparsed, value);
    }

    /// Parsing is idempotent under re-tokenization: parse → as_token → parse
    /// is a fixed point for any input.
    #[test]
    fn clustering_unit_parse_is_idempotent(raw in ".*") {
        let once = ClusteringUnit::parse(&raw);
        let twice = ClusteringUnit::parse(&once.as_token());
        prop_assert_eq!(once, twice);
    }
}

// ─── Spec construction helpers (YAML — every field has a serde default) ───

/// Build a minimal analysis spec with a clustered structure declared via a
/// `Cluster` variable role, and a caller-chosen `clustering` study-context
/// value. A blank `clustering` means the field is omitted entirely.
fn spec_with_cluster_role(clustering_value: &str) -> AnalysisSpec {
    let clustering_line = if clustering_value.is_empty() {
        String::new()
    } else {
        format!("  clustering: \"{clustering_value}\"\n")
    };
    let yaml = format!(
        "study:\n  title: T\n  design: cohort\n\
         study_context:\n\
         {clustering_line}\
         data:\n  path: data.csv\n  format: csv\n\
         variables:\n  - name: site\n    kind: categorical\n    roles: [cluster]\n\
         analyses:\n  - kind: inspect\n"
    );
    serde_yaml::from_str(&yaml).expect("valid minimal spec YAML")
}

/// Build a minimal analysis spec with NO clustered structure (no cluster role,
/// no survey cluster) and a caller-chosen `clustering` value.
fn spec_without_cluster_role(clustering_value: &str) -> AnalysisSpec {
    let clustering_line = if clustering_value.is_empty() {
        String::new()
    } else {
        format!("  clustering: \"{clustering_value}\"\n")
    };
    let yaml = format!(
        "study:\n  title: T\n  design: cohort\n\
         study_context:\n\
         {clustering_line}\
         data:\n  path: data.csv\n  format: csv\n\
         variables:\n  - name: age\n    kind: continuous\n    roles: [covariate]\n\
         analyses:\n  - kind: inspect\n"
    );
    serde_yaml::from_str(&yaml).expect("valid minimal spec YAML")
}

/// Count issues that name the `clustering` field as a cross-field
/// inconsistency (the hard-inconsistency message contains "inconsistent").
fn clustering_inconsistency_issues(spec: &AnalysisSpec) -> usize {
    validate_study_context(spec)
        .into_iter()
        .filter(|issue: &String| issue.contains("study_context.clustering is inconsistent"))
        .count()
}

// ─── Property 3 (task 10.4): monotonic inconsistency detection ────────────

proptest! {
    #![proptest_config(ProptestConfig { cases: 128, failure_persistence: None, .. ProptestConfig::default() })]

    /// Declaring a `Cluster` role while `clustering` parses to `None`
    /// (explicitly "none"/"no"/etc.) yields exactly one clustering
    /// inconsistency naming the `clustering` field. Removing the contradiction
    /// (using a real named cluster unit) removes exactly that one issue.
    #[test]
    fn cluster_role_with_none_clustering_is_inconsistent(
        none_token in prop::sample::select(vec!["none", "no", "false", "None", "NONE"]),
        named in "[a-z][a-z0-9_]{0,10}",
    ) {
        // Contradiction present: cluster role declared, clustering = none.
        let contradicting = spec_with_cluster_role(none_token);
        prop_assert!(
            matches!(ClusteringUnit::parse(none_token), ClusteringUnit::None),
            "test token must parse to None"
        );
        prop_assert_eq!(
            clustering_inconsistency_issues(&contradicting),
            1,
            "a cluster role + none clustering must yield exactly one clustering inconsistency"
        );

        // Contradiction removed: a real named cluster unit (must not be a
        // reserved none/individual token).
        let named_unit = format!("site_{named}");
        let consistent = spec_with_cluster_role(&named_unit);
        prop_assert_eq!(
            clustering_inconsistency_issues(&consistent),
            0,
            "naming a real cluster unit must remove the clustering inconsistency"
        );
    }
}

// ─── Property 1 (task 10.5): numeric-behavior preservation (pure part) ────

proptest! {
    #![proptest_config(ProptestConfig { cases: 128, failure_persistence: None, .. ProptestConfig::default() })]

    /// For any consistent spec (a real named cluster unit, or no clustered
    /// structure at all), validation adds NO clustering inconsistency issue.
    /// This is the formal half of "validation preserves behavior for valid
    /// inputs" — the numeric half is the parity suite (task 10.5 manual gate).
    #[test]
    fn consistent_specs_add_no_clustering_inconsistency(
        named in "[a-z][a-z0-9_]{0,10}",
        with_role in any::<bool>(),
    ) {
        let named_unit = format!("clinic_{named}");
        let spec = if with_role {
            spec_with_cluster_role(&named_unit)
        } else {
            // No cluster role and no clustering value → no contradiction.
            spec_without_cluster_role("")
        };
        prop_assert_eq!(
            clustering_inconsistency_issues(&spec),
            0,
            "a consistent spec must not produce a clustering inconsistency"
        );
    }
}
