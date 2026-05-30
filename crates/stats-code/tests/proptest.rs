#[path = "proptest/missing_props.rs"]
mod missing_props;

#[path = "proptest/anova_props.rs"]
mod anova_props;

#[path = "proptest/effect_props.rs"]
mod effect_props;

#[path = "proptest/ttest_props.rs"]
mod ttest_props;

#[path = "proptest/two_by_two_props.rs"]
mod two_by_two_props;

#[path = "proptest/sidecar_header_props.rs"]
mod sidecar_header_props;

#[path = "proptest/redaction_props.rs"]
mod redaction_props;

#[path = "proptest/sidecar_determinism_props.rs"]
mod sidecar_determinism_props;

#[path = "proptest/sidecar_coverage_props.rs"]
mod sidecar_coverage_props;

#[path = "proptest/coverage_matrix_props.rs"]
mod coverage_matrix_props;

#[path = "proptest/forbidden_spawn_props.rs"]
mod forbidden_spawn_props;

#[path = "proptest/workflow_yaml_props.rs"]
mod workflow_yaml_props;

#[path = "proptest/snapshot_props.rs"]
mod snapshot_props;

#[path = "proptest/parity_exit_code_props.rs"]
mod parity_exit_code_props;
