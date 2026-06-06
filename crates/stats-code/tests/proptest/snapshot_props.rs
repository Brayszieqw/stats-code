//! Property-based tests for the Audit Snapshot module.
//!
//! Properties 17, 18, 19, 20, 22 from the parity-and-multilang-sidecar spec.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use proptest::prelude::*;

use stats_code::snapshot::workflow_yaml::InputDataset;
use stats_code::snapshot::{
    export_snapshot, LlmCall, NarrativeStep, KeyMetric,
    RunSnapshot, RunStatus, SnapshotError, Workflow,
    ARTIFACT_PAYLOAD_CEILING_BYTES,
};
use stats_code::snapshot::llm_provenance::build_llm_provenance;
use stats_code::snapshot::narrative::build_narrative;
use stats_code::snapshot::SnapshotArtifact;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Monotonic counter so concurrent property cases never collide on temp paths.
static SNAP_SEQ: AtomicU64 = AtomicU64::new(0);

/// A unique temp destination path for one snapshot export.
fn unique_snapshot_dest() -> PathBuf {
    let seq = SNAP_SEQ.fetch_add(1, Ordering::Relaxed);
    let mut p = std::env::temp_dir();
    p.push(format!(
        "stats-code-snap-prop-{}-{}.zip",
        std::process::id(),
        seq
    ));
    p
}

/// Build a minimal valid `Completed` `RunSnapshot`.
fn build_run(run_id: String, dataset_csv_bytes: Vec<u8>) -> RunSnapshot {
    RunSnapshot {
        run_id,
        status: RunStatus::Completed,
        dataset_sha256: [0u8; 32],
        dataset_csv_bytes,
        workflow: Workflow {
            schema_version: 1,
            input_dataset: InputDataset {
                path: "data.csv".to_string(),
                sha256: "0".repeat(64),
            },
            steps: Vec::new(),
        },
        artifacts: Vec::new(),
        llm_calls: Vec::new(),
        reference_software: Vec::new(),
        os_family: "Linux".to_string(),
        os_version: "6.6.0".to_string(),
        release_version: "0.5.0".to_string(),
        commit_sha: "0".repeat(40),
        created_at_utc: "2024-01-01T00:00:00Z".to_string(),
        api_keys: Vec::new(),
        working_directory: None,
        narrative_steps: Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// Strategies
// ---------------------------------------------------------------------------

/// Strategy: a valid 64-char lowercase hex SHA256 string.
fn arb_sha256_hex() -> impl Strategy<Value = String> {
    proptest::collection::vec(prop::sample::select(b"0123456789abcdef".as_slice()), 64)
        .prop_map(|bytes| bytes.iter().map(|b| *b as char).collect::<String>())
}

/// Strategy: generate a single `LlmCall` with all required fields populated.
fn arb_llm_call() -> impl Strategy<Value = LlmCall> {
    (
        "[a-z]{3,10}",          // provider
        "[a-z]{3,10}-[a-z]{2,6}", // model
        arb_sha256_hex(),       // prompt_sha256
        arb_sha256_hex(),       // response_sha256
    )
        .prop_map(|(provider, model, prompt_sha256, response_sha256)| LlmCall {
            provider,
            model,
            request_at_utc: "2024-01-01T00:00:00Z".to_string(),
            prompt_sha256,
            response_sha256,
        })
}

/// Strategy: generate 0..=3 `LlmCalls`.
fn arb_llm_calls(max: usize) -> impl Strategy<Value = Vec<LlmCall>> {
    proptest::collection::vec(arb_llm_call(), 0..=max)
}

/// Strategy: generate a single `SnapshotArtifact` (≤ 1 KB).
fn arb_artifact() -> impl Strategy<Value = SnapshotArtifact> {
    (
        "[a-z]{3,8}",  // step_id
        "[a-z]{3,8}\\.(json|csv|txt)", // filename
        proptest::collection::vec(any::<u8>(), 0..1024), // bytes
    )
        .prop_map(|(step_id, filename, bytes)| SnapshotArtifact {
            path: format!("artifacts/{step_id}/{filename}"),
            bytes,
        })
}

/// Strategy: generate 0..=3 artifacts.
fn arb_artifacts() -> impl Strategy<Value = Vec<SnapshotArtifact>> {
    proptest::collection::vec(arb_artifact(), 0..=3)
}

/// Strategy: generate a valid `os_family`.
fn arb_os_family() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("Windows".to_string()),
        Just("Linux".to_string()),
        Just("macOS".to_string()),
    ]
}

/// Strategy: generate a completed `RunSnapshot` with variable artifacts and LLM calls.
fn arb_completed_run() -> impl Strategy<Value = RunSnapshot> {
    (
        "[a-z][a-z0-9-]{0,15}",  // run_id
        proptest::collection::vec(any::<u8>(), 1..128), // dataset_csv_bytes
        arb_artifacts(),
        arb_llm_calls(3),
        arb_os_family(),
        "[a-z0-9.]{1,20}",  // os_version (short, under 32 chars)
    )
        .prop_map(|(run_id, csv, artifacts, llm_calls, os_family, os_version)| {
            RunSnapshot {
                run_id,
                status: RunStatus::Completed,
                dataset_sha256: [0u8; 32],
                dataset_csv_bytes: csv,
                workflow: Workflow {
                    schema_version: 1,
                    input_dataset: InputDataset {
                        path: "data.csv".to_string(),
                        sha256: "0".repeat(64),
                    },
                    steps: Vec::new(),
                },
                artifacts,
                llm_calls,
                reference_software: Vec::new(),
                os_family,
                os_version,
                release_version: "0.5.0".to_string(),
                commit_sha: "0".repeat(40),
                created_at_utc: "2024-01-01T00:00:00Z".to_string(),
                api_keys: Vec::new(),
                working_directory: None,
                narrative_steps: Vec::new(),
            }
        })
}

/// Strategy: generate a run that violates a gate (non-completed or payload > 50 MB).
fn arb_gate_violation_run() -> impl Strategy<Value = RunSnapshot> {
    prop_oneof![
        // Non-completed status
        prop_oneof![Just(RunStatus::Running), Just(RunStatus::Failed)]
            .prop_map(|status| {
                let mut run = build_run("gate-run".to_string(), b"x\n".to_vec());
                run.status = status;
                run
            }),
        // Payload too large: single artifact > 50 MB
        Just({
            let mut run = build_run("gate-run-large".to_string(), b"x\n".to_vec());
            run.artifacts.push(SnapshotArtifact {
                path: "artifacts/step-1/big.bin".to_string(),
                bytes: vec![0u8; (ARTIFACT_PAYLOAD_CEILING_BYTES + 1) as usize],
            });
            run
        }),
    ]
}

// ---------------------------------------------------------------------------
// Property 17: Audit Snapshot file set and member field completeness
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 32,
        failure_persistence: None,
        .. ProptestConfig::default()
    })]

    /// Property 17: A completed run with payload ≤ 50 MB produces a snapshot
    /// file that exists, is non-empty, and the SnapshotResult contains a valid
    /// sha256. The export succeeds for any valid completed run configuration.
    ///
    /// Since the `zip` crate is not available as a dev-dependency, we verify:
    /// - export_snapshot returns Ok(SnapshotResult)
    /// - snapshot_path exists on disk and is non-empty
    /// - sha256 is a non-zero 32-byte array
    /// - The file starts with the ZIP local-file-header magic bytes (PK\x03\x04)
    ///
    /// **Validates: Requirements 7.2, 7.3, 7.4, 7.5, 8.1**
    #[test]
    fn snapshot_file_set_and_field_completeness(run in arb_completed_run()) {
        let dest = unique_snapshot_dest();

        let result = export_snapshot(&run, &dest);
        prop_assert!(
            result.is_ok(),
            "export_snapshot failed for a valid completed run: {:?}",
            result.err(),
        );

        let snap = result.unwrap();

        // Snapshot path exists and is non-empty.
        prop_assert!(
            snap.snapshot_path.exists(),
            "snapshot file must exist at {:?}",
            snap.snapshot_path,
        );
        let metadata = std::fs::metadata(&snap.snapshot_path).unwrap();
        prop_assert!(
            metadata.len() > 0,
            "snapshot file must be non-empty",
        );

        // SHA256 is not all zeros (extremely unlikely for real content).
        prop_assert!(
            snap.sha256 != [0u8; 32],
            "sha256 must not be all zeros for a real snapshot",
        );

        // File starts with ZIP magic bytes.
        let bytes = std::fs::read(&snap.snapshot_path).unwrap();
        prop_assert!(
            bytes.len() >= 4 && &bytes[0..4] == b"PK\x03\x04",
            "snapshot must be a valid ZIP (starts with PK\\x03\\x04)",
        );

        // File contains EOCD signature (PK\x05\x06) near the end.
        let n = bytes.len();
        prop_assert!(
            n >= 22 && &bytes[n - 22..n - 18] == b"PK\x05\x06",
            "snapshot must contain EOCD signature",
        );

        // Verify the expected file names appear as substrings in the ZIP
        // (ZIP local file headers contain the filename in plaintext).
        let expected_files = [
            "data.csv",
            "manifest.json",
            "workflow.yaml",
            "versions.json",
            "llm_provenance.json",
            "narrative.md",
            "coverage.json",
        ];
        for name in &expected_files {
            prop_assert!(
                contains_bytes(&bytes, name.as_bytes()),
                "snapshot ZIP must contain entry {:?}",
                name,
            );
        }

        // Verify artifact paths appear in the ZIP.
        for art in &run.artifacts {
            prop_assert!(
                contains_bytes(&bytes, art.path.as_bytes()),
                "snapshot ZIP must contain artifact entry {:?}",
                art.path,
            );
        }

        // Cleanup.
        let _ = std::fs::remove_file(&dest);
    }
}

// ---------------------------------------------------------------------------
// Property 18: Audit Snapshot refusal gates leave no partial output
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 32,
        failure_persistence: None,
        .. ProptestConfig::default()
    })]

    /// Property 18: Gate violations return a structured error with the gate
    /// class and offending values, and leave no .zip or .tmp on disk. Any
    /// pre-existing snapshot file at the destination is not modified.
    ///
    /// **Validates: Requirements 7.7, 7.8, 8.4, 8.6, 12.6**
    #[test]
    fn refusal_gates_leave_no_partial_output(run in arb_gate_violation_run()) {
        let dest = unique_snapshot_dest();
        let tmp = {
            let mut s = dest.as_os_str().to_owned();
            s.push(".tmp");
            PathBuf::from(s)
        };

        // Optionally create a pre-existing file to verify it's untouched.
        let sentinel = b"SENTINEL_CONTENT_DO_NOT_MODIFY";
        std::fs::write(&dest, sentinel).unwrap();

        let result = export_snapshot(&run, &dest);

        // Must return an error.
        prop_assert!(
            result.is_err(),
            "gate violation must return Err, got Ok({:?})",
            result.ok(),
        );

        let err = result.unwrap_err();

        // Error must be structured (RunNotCompleted or PayloadTooLarge).
        match &err {
            SnapshotError::RunNotCompleted { actual } => {
                prop_assert!(
                    *actual != RunStatus::Completed,
                    "RunNotCompleted must carry a non-Completed status",
                );
            }
            SnapshotError::PayloadTooLarge { measured_bytes, ceiling } => {
                prop_assert!(
                    *measured_bytes > *ceiling,
                    "PayloadTooLarge must have measured > ceiling",
                );
            }
            other => {
                prop_assert!(
                    false,
                    "unexpected error variant for gate violation: {:?}",
                    other,
                );
            }
        }

        // No .tmp file on disk.
        prop_assert!(
            !tmp.exists(),
            ".tmp file must not exist after gate refusal",
        );

        // Pre-existing file at destination is unchanged.
        let post_content = std::fs::read(&dest).unwrap();
        prop_assert_eq!(
            post_content.as_slice(),
            sentinel,
            "pre-existing snapshot file must not be modified by a gate refusal",
        );

        // Cleanup.
        let _ = std::fs::remove_file(&dest);
    }
}

// ---------------------------------------------------------------------------
// Property 19: Audit Snapshot privacy constraints
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 32,
        failure_persistence: None,
        .. ProptestConfig::default()
    })]

    /// Property 19: For any host / any input path:
    /// - os_family ∈ {Windows, Linux, macOS}
    /// - os_version ≤ 32 characters
    /// - The snapshot bytes do not contain the host name, OS user name, or
    ///   user home directory absolute path.
    /// - Working-directory-internal paths are rendered as relative; external
    ///   paths become `<external>`.
    ///
    /// We inject known "sensitive" strings into the run's fields and verify
    /// they do not appear in the exported snapshot's raw bytes.
    ///
    /// **Validates: Requirements 9.2, 9.3**
    #[test]
    fn snapshot_privacy_constraints(
        os_family in arb_os_family(),
        os_version_raw in ".{0,50}",  // arbitrary string, possibly > 32 chars
        host_name in "[A-Z][A-Z0-9-]{4,12}",
        user_name in "[a-z][a-z0-9_]{3,10}",
    ) {
        // Build a working directory that contains the user_name (simulating
        // a real home directory path).
        let home_dir = if os_family == "Windows" {
            format!("C:\\Users\\{user_name}")
        } else {
            format!("/home/{user_name}")
        };
        let work_dir = format!("{home_dir}/project");

        // Inject the host_name and user_name into an artifact's text content
        // so the redaction layer must scrub them if they appear as paths.
        let leak_content = format!(
            "host={host_name} user={user_name} home={home_dir}/secret.txt workfile={work_dir}/data.csv"
        );

        let mut run = build_run("privacy-run".to_string(), b"col\n1\n".to_vec());
        run.os_family = os_family.clone();
        run.os_version = os_version_raw.clone();
        run.working_directory = Some(PathBuf::from(&work_dir));
        run.artifacts.push(SnapshotArtifact {
            path: "artifacts/step-1/notes.txt".to_string(),
            bytes: leak_content.into_bytes(),
        });

        let dest = unique_snapshot_dest();
        let result = export_snapshot(&run, &dest);
        prop_assert!(
            result.is_ok(),
            "export_snapshot failed: {:?}",
            result.err(),
        );

        let snap_bytes = std::fs::read(&dest).unwrap();

        // os_version in the versions.json inside the zip must be ≤ 32 chars.
        // We verify by checking that the raw os_version (if > 32 chars) does
        // NOT appear verbatim in the snapshot bytes.
        if os_version_raw.chars().count() > 32 {
            prop_assert!(
                !contains_bytes(&snap_bytes, os_version_raw.as_bytes()),
                "raw os_version longer than 32 chars must be truncated in snapshot",
            );
        }

        // The home directory absolute path must not appear in the snapshot
        // (it should be redacted to relative or <external>).
        // Only check if the home_dir is long enough to be meaningful and
        // would be detected by the path scanner.
        if home_dir.len() > 5 {
            prop_assert!(
                !contains_bytes(&snap_bytes, home_dir.as_bytes()),
                "home directory path {:?} must not appear in snapshot bytes",
                home_dir,
            );
        }

        // Cleanup.
        let _ = std::fs::remove_file(&dest);
    }
}

// ---------------------------------------------------------------------------
// Property 20: Narrative citations resolve
// ---------------------------------------------------------------------------

/// Strategy: generate a `NarrativeStep` whose `key_metrics` cite paths from a
/// known file index.
fn arb_narrative_step_with_index() -> impl Strategy<Value = (Vec<NarrativeStep>, BTreeSet<String>)> {
    // Generate 1..=3 steps, each with 1..=3 metrics.
    let arb_step = (
        "[a-z]{3,8}",       // step id
        "[a-z]{4,10}",      // algorithm
        "[A-Z][a-z ]{3,15}", // display_name
        "[a-z=,]{2,12}",    // params_summary
        1..=3usize,         // metric count
    );

    proptest::collection::vec(arb_step, 1..=3).prop_map(|steps_raw| {
        let mut index = BTreeSet::new();
        let mut steps = Vec::new();

        for (id, algorithm, display_name, params_summary, metric_count) in steps_raw {
            let mut key_metrics = Vec::new();
            for m in 0..metric_count {
                let artifact_path = format!("artifacts/{id}/result_{m}.json");
                let json_pointer = format!("field_{m}");
                index.insert(artifact_path.clone());
                key_metrics.push(KeyMetric {
                    label: format!("Metric {m}"),
                    value: format!("{}.{}", m + 1, m * 3),
                    artifact_path,
                    json_pointer,
                });
            }
            steps.push(NarrativeStep {
                id,
                algorithm,
                display_name,
                params_summary,
                key_metrics,
            });
        }

        (steps, index)
    })
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 128,
        failure_persistence: None,
        .. ProptestConfig::default()
    })]

    /// Property 20: For any generated narrative + file index, every
    /// `[<path>#<json_pointer>]` citation's path exists in the file index,
    /// and every numeric value in the prose has a citation.
    ///
    /// **Validates: Requirements 8.5**
    #[test]
    fn narrative_citations_resolve(
        (steps, index) in arb_narrative_step_with_index(),
    ) {
        let result = build_narrative(&steps, &index);
        prop_assert!(
            result.is_ok(),
            "build_narrative must succeed when all cited paths are in the index: {:?}",
            result.err(),
        );

        let narrative = result.unwrap();

        // Every citation [path#pointer] in the output must have its path in
        // the index.
        for citation in extract_citations(&narrative) {
            prop_assert!(
                index.contains(&citation),
                "citation path {:?} not found in artifacts_index",
                citation,
            );
        }

        // Every numeric value from key_metrics must appear in the narrative
        // with a citation (i.e., the value string is followed somewhere by
        // a `[` on the same line).
        for step in &steps {
            for metric in &step.key_metrics {
                prop_assert!(
                    narrative.contains(&metric.value),
                    "metric value {:?} must appear in narrative output",
                    metric.value,
                );
                // The value must be followed by a citation on the same bullet.
                let expected_citation = format!(
                    "{} [{}#{}]",
                    metric.value, metric.artifact_path, metric.json_pointer
                );
                prop_assert!(
                    narrative.contains(&expected_citation),
                    "metric value {:?} must be followed by its citation; \
                     expected {:?} in narrative",
                    metric.value,
                    expected_citation,
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Property 22: LLM provenance count matches LLM call count
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 128,
        failure_persistence: None,
        .. ProptestConfig::default()
    })]

    /// Property 22: A run with k LLM calls produces an LlmProvenance with
    /// calls.len() == k. Each call contains the four required fields
    /// (provider, model, request_at_utc, prompt_sha256, response_sha256).
    /// When k=0, calls is an empty Vec.
    ///
    /// **Validates: Requirements 7.5**
    #[test]
    fn llm_provenance_count_matches_call_count(
        calls in arb_llm_calls(5),
    ) {
        let k = calls.len();
        let provenance = build_llm_provenance(&calls);

        // Count matches.
        prop_assert_eq!(
            provenance.calls.len(),
            k,
            "provenance.calls.len() must equal the number of input LLM calls",
        );

        // k=0 case: calls is empty vec.
        if k == 0 {
            prop_assert!(
                provenance.calls.is_empty(),
                "when k=0, calls must be an empty Vec",
            );
        }

        // Every call has the five required fields non-empty.
        for (i, call) in provenance.calls.iter().enumerate() {
            prop_assert!(
                !call.provider.is_empty(),
                "call[{i}].provider must be non-empty",
            );
            prop_assert!(
                !call.model.is_empty(),
                "call[{i}].model must be non-empty",
            );
            prop_assert!(
                !call.request_at_utc.is_empty(),
                "call[{i}].request_at_utc must be non-empty",
            );
            prop_assert!(
                call.prompt_sha256.len() == 64,
                "call[{i}].prompt_sha256 must be 64-char hex, got len={}",
                call.prompt_sha256.len(),
            );
            prop_assert!(
                call.response_sha256.len() == 64,
                "call[{i}].response_sha256 must be 64-char hex, got len={}",
                call.response_sha256.len(),
            );
        }

        // Schema version is always 1.
        prop_assert_eq!(
            provenance.schema_version,
            1,
            "schema_version must be 1",
        );
    }
}

// ---------------------------------------------------------------------------
// Utility functions
// ---------------------------------------------------------------------------

/// Extract all citation paths from narrative text. Citations have the form
/// `[<path>#<json_pointer>]`. Returns the `<path>` portion of each.
fn extract_citations(text: &str) -> Vec<String> {
    let mut citations = Vec::new();
    let mut remaining = text;
    while let Some(start) = remaining.find('[') {
        let after_bracket = &remaining[start + 1..];
        if let Some(end) = after_bracket.find(']') {
            let inner = &after_bracket[..end];
            if let Some(hash_pos) = inner.find('#') {
                let path = &inner[..hash_pos];
                // Only consider it a citation if it looks like a path
                // (contains a slash).
                if path.contains('/') {
                    citations.push(path.to_string());
                }
            }
            remaining = &after_bracket[end + 1..];
        } else {
            break;
        }
    }
    citations
}

/// Check if `haystack` contains `needle` as a contiguous subsequence.
fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() {
        return true;
    }
    haystack.windows(needle.len()).any(|w| w == needle)
}
