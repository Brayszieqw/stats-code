//! `--replay <audit_snapshot.zip>` execution path.
//!
//! Implements task 7.2 of `parity-and-multilang-sidecar`. See design.md
//! §"`ReplayPlan`" and Requirements 8.3, 8.4, 8.6, 8.7.
//!
//! ## Wave-1 simplification
//!
//! The full `--replay` flow accepts a `.zip` snapshot, extracts it to a
//! scratch directory, then runs the gates below. Wave-1 splits that into
//! two halves:
//!
//! 1. The CLI / agent-server handler is responsible for extracting the
//!    snapshot zip into a directory — that lets us avoid pulling in a
//!    zip *reader* dependency this wave (the deterministic *writer* in
//!    `zip_writer.rs` is hand-rolled). When wave-2 lands we will swap
//!    the input shape to `snapshot_path: PathBuf` and unzip in-process.
//! 2. [`execute_replay`] takes the already-extracted directory and runs
//!    every pre-flight gate against the on-disk file set.
//!
//! Both decisions are explicit in [`ReplayPlan::extracted_dir`] and in
//! the doc-comments below. No behavior described in Requirements 8.3 /
//! 8.4 / 8.6 / 8.7 is dropped — only the zip-extraction step is
//! delegated.
//!
//! ## Gate ladder
//!
//! Pre-flight gates (Requirements 8.3, 8.4, 8.6):
//!
//! 1. Read `manifest.json`. Compute SHA256 of `data.csv`. If the digest
//!    does not match `manifest.input_dataset_sha256`, refuse with
//!    [`ReplayError::DatasetSha256Mismatch`] (Requirement 8.4). The
//!    snapshot file set is left untouched.
//! 2. Read `versions.json`. For each entry in
//!    `versions.reference_software`, look it up in
//!    `plan.installed_reference_software`. Any missing entry or version
//!    mismatch produces a single
//!    [`ReplayError::ReferenceSoftwareUnavailable`] listing every
//!    offender (Requirement 8.6).
//! 3. Read `workflow.yaml`. For each step's `inputs[*]`, recompute the
//!    SHA256 of `<extracted_dir>/<input.path>` and compare it to the
//!    declared sha256. On any mismatch, abort with
//!    [`ReplayError::InputArtifactSha256Mismatch`].
//!
//! Execution gate (Requirement 8.7):
//!
//! 4. **Wave-1 stub.** The full implementation re-runs each step
//!    through the Stats Engine and checks every metric against the
//!    active Parity Threshold (raising
//!    [`ReplayError::NumericDrift`] on failure). Wave-1 ships only the
//!    integrity gates for outputs: each step's `outputs[*]` SHA256 is
//!    recomputed from disk and compared, so a tampered output bytes
//!    payload still aborts the replay. Wave-2 will plug in the
//!    re-execution loop.
//!
//! ## Privacy / spawn policy
//!
//! `execute_replay` is purely filesystem + hashing; it never spawns a
//! child process, so it does not need a [`crate::spawn_policy`] scope
//! guard. The reference-software probe is delegated to the caller via
//! `plan.installed_reference_software` precisely so the replay function
//! itself stays free of subprocess invocations
//! (Requirements 10.1 / 10.5 are honored at the caller).
//!
//! _Requirements: 8.3, 8.4, 8.6, 8.7_

use std::fs;
use std::path::{Path, PathBuf};

use thiserror::Error;

use super::manifest::AuditSnapshotManifest;
use super::sha256_oneshot;
use super::versions::Versions;
use super::workflow_yaml;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Plan describing a single `--replay` invocation.
///
/// Wave-1 contract: the caller extracts the snapshot zip into
/// [`extracted_dir`](ReplayPlan::extracted_dir) and provides a snapshot of
/// what reference software the local host has installed. Wave-2 will
/// switch the input shape to a `snapshot_path: PathBuf` once an in-process
/// zip reader is available; the gate algorithm itself is unchanged.
#[derive(Debug, Clone)]
pub struct ReplayPlan {
    /// Directory containing the extracted Audit Snapshot file set
    /// (`manifest.json`, `versions.json`, `workflow.yaml`, `data.csv`,
    /// per-step artifacts under `artifacts/<step>/...`).
    ///
    /// Wave-2 will replace this with `snapshot_path: PathBuf` and unzip
    /// in-process; for wave-1 the caller (CLI / agent-server) is the
    /// extractor.
    pub extracted_dir: PathBuf,

    /// Reference Software the local host has installed, expressed as
    /// `(name, version)` tuples. Names are matched against
    /// `versions.reference_software[*].name`; versions must match
    /// exactly (Requirement 8.6).
    pub installed_reference_software: Vec<(String, String)>,
}

/// Outcome of a successful replay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayOutcome {
    /// Number of workflow steps whose pre-flight + integrity gates
    /// passed. In wave-1 this equals `workflow.steps.len()` because the
    /// re-execution loop has not been wired yet (see module-level
    /// "Execution gate" note).
    pub steps_replayed: usize,
}

/// Error returned by the replay executor.
#[derive(Debug, Error)]
pub enum ReplayError {
    /// Filesystem I/O failure while opening / reading a snapshot member.
    /// The caller should treat this as a refusal: the snapshot has not
    /// been re-executed and no artifact has been produced.
    #[error("snapshot file io error at {path}: {source}")]
    SnapshotIo {
        path: String,
        #[source]
        source: std::io::Error,
    },

    /// `manifest.input_dataset_sha256` does not match the recomputed
    /// SHA256 of `data.csv` (Requirement 8.4). The snapshot file set is
    /// left untouched.
    #[error(
        "dataset sha256 mismatch at {path}: expected {expected}, actual {actual}"
    )]
    DatasetSha256Mismatch {
        path: String,
        expected: String,
        actual: String,
    },

    /// One or more entries in `versions.reference_software` are absent
    /// from the local host or installed at a version that differs from
    /// the recorded one (Requirement 8.6).
    #[error("reference software unavailable: missing {missing:?}")]
    ReferenceSoftwareUnavailable {
        /// One element per offending entry, formatted as
        /// `"<name> <expected_version>"`.
        missing: Vec<String>,
    },

    /// A step's input artifact bytes do not match the declared SHA256.
    #[error(
        "input artifact sha256 mismatch at {path}: expected {expected}, actual {actual}"
    )]
    InputArtifactSha256Mismatch {
        path: String,
        expected: String,
        actual: String,
    },

    /// A step's output artifact bytes do not match the declared SHA256.
    /// Wave-1 surfaces this as a stand-in for the full numeric-drift
    /// gate (Requirement 8.7); wave-2 will additionally re-run the step
    /// through the Stats Engine and surface
    /// [`ReplayError::NumericDrift`] for in-bounds-but-different
    /// outputs.
    #[error(
        "output artifact sha256 mismatch at {path}: expected {expected}, actual {actual}"
    )]
    OutputArtifactSha256Mismatch {
        path: String,
        expected: String,
        actual: String,
    },

    /// A re-executed step's metric drifted past the active Parity
    /// Threshold (Requirement 8.7). Reserved for wave-2; wave-1 never
    /// produces this variant directly because it does not yet re-run
    /// steps.
    #[error(
        "numeric drift in step {step_id}, metric {metric}: expected {expected}, actual {actual}"
    )]
    NumericDrift {
        step_id: String,
        metric: String,
        expected: f64,
        actual: f64,
    },

    /// A snapshot member is structurally invalid (malformed JSON / YAML,
    /// schema violation). The wrapped string identifies the offending
    /// file and rule.
    #[error("invalid snapshot: {0}")]
    InvalidSnapshot(String),
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Execute a `--replay` plan against the recorded Audit Snapshot.
///
/// Runs the gate ladder documented at the module level. On success
/// returns a [`ReplayOutcome`] reporting how many steps passed every
/// gate. On any failure returns a structured [`ReplayError`] that names
/// the gate class and the offending values; the snapshot file set is
/// never modified by this function.
///
/// _Requirements: 8.3, 8.4, 8.6, 8.7_
pub fn execute_replay(plan: ReplayPlan) -> Result<ReplayOutcome, ReplayError> {
    let dir = plan.extracted_dir.as_path();

    // ---- Gate 0: read manifest.json ------------------------------------
    let manifest_path = dir.join("manifest.json");
    let manifest_bytes = read_member(&manifest_path)?;
    let manifest: AuditSnapshotManifest =
        serde_json::from_slice(&manifest_bytes).map_err(|e| {
            ReplayError::InvalidSnapshot(format!(
                "manifest.json: {e} (path={})",
                manifest_path.display()
            ))
        })?;

    // ---- Gate 1: data.csv SHA256 == manifest.input_dataset_sha256 ------
    let data_csv_path = dir.join("data.csv");
    let data_csv_bytes = read_member(&data_csv_path)?;
    let actual_dataset_hex = encode_hex_lower(&sha256_oneshot(&data_csv_bytes));
    if actual_dataset_hex != manifest.input_dataset_sha256 {
        return Err(ReplayError::DatasetSha256Mismatch {
            path: relative_archive_path(&data_csv_path, dir),
            expected: manifest.input_dataset_sha256,
            actual: actual_dataset_hex,
        });
    }

    // ---- Gate 2: every recorded reference software is installed -------
    let versions_path = dir.join("versions.json");
    let versions_bytes = read_member(&versions_path)?;
    let versions: Versions = serde_json::from_slice(&versions_bytes).map_err(|e| {
        ReplayError::InvalidSnapshot(format!(
            "versions.json: {e} (path={})",
            versions_path.display()
        ))
    })?;
    let mut missing: Vec<String> = Vec::new();
    for required in &versions.reference_software {
        let installed = plan
            .installed_reference_software
            .iter()
            .any(|(n, v)| n == &required.name && v == &required.version);
        if !installed {
            missing.push(format!("{} {}", required.name, required.version));
        }
    }
    if !missing.is_empty() {
        return Err(ReplayError::ReferenceSoftwareUnavailable { missing });
    }

    // ---- Gate 3: every step's input artifact SHA256 matches -----------
    let workflow_path = dir.join("workflow.yaml");
    let workflow_bytes = read_member(&workflow_path)?;
    let (workflow, _doc) = workflow_yaml::parse(&workflow_bytes).map_err(|e| {
        ReplayError::InvalidSnapshot(format!(
            "workflow.yaml: {e} (path={})",
            workflow_path.display()
        ))
    })?;

    for step in &workflow.steps {
        for input in &step.inputs {
            verify_artifact_sha256(dir, &input.path, &input.sha256, ArtifactKind::Input)?;
        }
    }

    // ---- Gate 4 (wave-1): every recorded output artifact SHA256
    // matches. Wave-2 will *additionally* re-execute the step through
    // the Stats Engine and surface NumericDrift on metric drift; until
    // then the integrity gate at least proves that the bytes the
    // snapshot recorded are still on disk and unmodified. This is the
    // wave-1 stand-in for Requirement 8.7's full re-execution.
    for step in &workflow.steps {
        for output in &step.outputs {
            verify_artifact_sha256(dir, &output.path, &output.sha256, ArtifactKind::Output)?;
        }
    }

    Ok(ReplayOutcome {
        steps_replayed: workflow.steps.len(),
    })
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Distinguishes input vs output artifacts so [`verify_artifact_sha256`]
/// can route to the right [`ReplayError`] variant.
#[derive(Copy, Clone)]
enum ArtifactKind {
    Input,
    Output,
}

fn verify_artifact_sha256(
    dir: &Path,
    archive_path: &str,
    expected_hex: &str,
    kind: ArtifactKind,
) -> Result<(), ReplayError> {
    let on_disk = dir.join(archive_path);
    let bytes = read_member(&on_disk)?;
    let actual_hex = encode_hex_lower(&sha256_oneshot(&bytes));
    if actual_hex != expected_hex {
        let err = match kind {
            ArtifactKind::Input => ReplayError::InputArtifactSha256Mismatch {
                path: archive_path.to_owned(),
                expected: expected_hex.to_owned(),
                actual: actual_hex,
            },
            ArtifactKind::Output => ReplayError::OutputArtifactSha256Mismatch {
                path: archive_path.to_owned(),
                expected: expected_hex.to_owned(),
                actual: actual_hex,
            },
        };
        return Err(err);
    }
    Ok(())
}

fn read_member(path: &Path) -> Result<Vec<u8>, ReplayError> {
    fs::read(path).map_err(|source| ReplayError::SnapshotIo {
        path: path.display().to_string(),
        source,
    })
}

/// Encode a 32-byte digest as 64-character lowercase hexadecimal — the
/// representation used in `manifest.json::input_dataset_sha256` and in
/// every workflow artifact reference.
fn encode_hex_lower(bytes: &[u8; 32]) -> String {
    let mut out = String::with_capacity(64);
    for b in bytes {
        use std::fmt::Write as _;
        let _ = write!(&mut out, "{b:02x}");
    }
    out
}

/// Project an absolute (or relative) `path` back onto its archive-style
/// path inside the snapshot directory, falling back to the lossy display
/// form on platforms where the path can't be expressed as UTF-8.
fn relative_archive_path(path: &Path, root: &Path) -> String {
    path.strip_prefix(root)
        .ok()
        .and_then(|p| p.to_str()).map_or_else(|| path.display().to_string(), |s| s.replace('\\', "/"))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::BTreeMap;

    use crate::snapshot::manifest::{
        AuditSnapshotManifest, RUN_STATUS_COMPLETED, SCHEMA_VERSION as MANIFEST_SCHEMA_VERSION,
    };
    use crate::snapshot::versions::{
        ReferenceSoftwareVersion, Versions, SCHEMA_VERSION as VERSIONS_SCHEMA_VERSION,
    };
    use crate::snapshot::workflow_yaml::{
        pretty_print, ArtifactRef, InputDataset, Workflow, WorkflowStep,
    };

    /// Per-test scratch dir under the OS temp root. Reset before each
    /// invocation so re-running the suite from a previous failure does
    /// not accidentally inherit stale state.
    fn temp_dir(name: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!(
            "stats-code-replay-{}-{}",
            name,
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).expect("temp dir");
        p
    }

    /// Build a minimal extracted snapshot directory with one workflow
    /// step and one artifact, all SHA256s computed correctly. Returns
    /// the directory path.
    fn build_extracted_snapshot(dir: &Path) -> (Vec<(String, String)>, Vec<u8>) {
        // dataset
        let csv_bytes = b"col1,col2\n1,2\n".to_vec();
        let dataset_sha = sha256_oneshot(&csv_bytes);
        let dataset_hex = encode_hex_lower(&dataset_sha);
        fs::write(dir.join("data.csv"), &csv_bytes).unwrap();

        // step-1 input == data.csv
        let input_path = "data.csv".to_owned();
        let input_sha_hex = dataset_hex.clone();

        // step-1 output
        let out_bytes = br#"{"estimate": 1.234}"#.to_vec();
        let out_sha = sha256_oneshot(&out_bytes);
        let out_sha_hex = encode_hex_lower(&out_sha);
        let out_path = "artifacts/step-1/result.json";
        fs::create_dir_all(dir.join("artifacts/step-1")).unwrap();
        fs::write(dir.join(out_path), &out_bytes).unwrap();

        // manifest.json
        let manifest = AuditSnapshotManifest {
            schema_version: MANIFEST_SCHEMA_VERSION,
            input_dataset_sha256: dataset_hex.clone(),
            created_at_utc: "2024-01-01T00:00:00Z".to_owned(),
            stats_code_release_version: "0.5.0".to_owned(),
            stats_code_commit_sha: "0".repeat(40),
            run_id: "run-replay".to_owned(),
            run_status: RUN_STATUS_COMPLETED.to_owned(),
        };
        fs::write(
            dir.join("manifest.json"),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();

        // versions.json
        let mut runtime_dependencies = BTreeMap::new();
        runtime_dependencies.insert("axum".to_owned(), "0.7.5".to_owned());
        let versions = Versions {
            schema_version: VERSIONS_SCHEMA_VERSION,
            os_family: "Linux".to_owned(),
            os_version: "6.6.0".to_owned(),
            version_truncated: false,
            reference_software: vec![ReferenceSoftwareVersion {
                name: "R".to_owned(),
                version: "4.4.1".to_owned(),
            }],
            runtime_dependencies,
        };
        fs::write(
            dir.join("versions.json"),
            serde_json::to_vec(&versions).unwrap(),
        )
        .unwrap();

        // workflow.yaml — canonical pretty-print so the parser can
        // round-trip it.
        let workflow = Workflow {
            schema_version: 1,
            input_dataset: InputDataset {
                path: "data.csv".to_owned(),
                sha256: dataset_hex.clone(),
            },
            steps: vec![WorkflowStep {
                id: "step-1".to_owned(),
                algorithm: "tableone".to_owned(),
                params: serde_json::json!({"by": "treatment"}),
                inputs: vec![ArtifactRef {
                    path: input_path,
                    sha256: input_sha_hex,
                }],
                outputs: vec![ArtifactRef {
                    path: out_path.to_owned(),
                    sha256: out_sha_hex.clone(),
                }],
                reference_software: None,
                llm: None,
                started_at_utc: "2024-01-01T00:00:00Z".to_owned(),
                ended_at_utc: "2024-01-01T00:00:01Z".to_owned(),
            }],
        };
        fs::write(dir.join("workflow.yaml"), pretty_print(&workflow, None)).unwrap();

        // Match the host R 4.4.1 — happy path expects this.
        let installed = vec![("R".to_owned(), "4.4.1".to_owned())];
        (installed, csv_bytes)
    }

    // ---- Happy path -----------------------------------------------------

    #[test]
    fn happy_path_returns_steps_replayed_count() {
        let dir = temp_dir("happy");
        let (installed, _csv) = build_extracted_snapshot(&dir);

        let outcome = execute_replay(ReplayPlan {
            extracted_dir: dir.clone(),
            installed_reference_software: installed,
        })
        .expect("happy path replay must succeed");
        assert_eq!(outcome, ReplayOutcome { steps_replayed: 1 });
    }

    // ---- Gate 1: dataset SHA256 mismatch --------------------------------

    #[test]
    fn dataset_sha256_mismatch_is_refused() {
        let dir = temp_dir("dataset-mismatch");
        let (installed, _csv) = build_extracted_snapshot(&dir);

        // Tamper data.csv after manifest was written, so its on-disk
        // SHA256 no longer matches manifest.input_dataset_sha256.
        let tampered = b"col1,col2\n9,9\n".to_vec();
        fs::write(dir.join("data.csv"), &tampered).unwrap();

        let err = execute_replay(ReplayPlan {
            extracted_dir: dir.clone(),
            installed_reference_software: installed,
        })
        .expect_err("tampered data.csv must be refused");

        match err {
            ReplayError::DatasetSha256Mismatch {
                path,
                expected,
                actual,
            } => {
                assert_eq!(path, "data.csv");
                assert_ne!(expected, actual);
                assert_eq!(actual, encode_hex_lower(&sha256_oneshot(&tampered)));
            }
            other => panic!("expected DatasetSha256Mismatch, got {other:?}"),
        }

        // Snapshot file set is left untouched (Requirement 8.4: "leave
        // the snapshot file unmodified").
        assert!(dir.join("manifest.json").exists());
        assert!(dir.join("workflow.yaml").exists());
        assert!(dir.join("versions.json").exists());
    }

    // ---- Gate 2: reference software unavailable -------------------------

    #[test]
    fn missing_reference_software_lists_every_offender() {
        let dir = temp_dir("ref-missing");
        let (_installed, _csv) = build_extracted_snapshot(&dir);

        // Pass an empty installed list — R 4.4.1 is recorded in the
        // snapshot but absent on the "host", so the gate must refuse and
        // name the offender.
        let err = execute_replay(ReplayPlan {
            extracted_dir: dir,
            installed_reference_software: Vec::new(),
        })
        .expect_err("missing reference software must be refused");

        match err {
            ReplayError::ReferenceSoftwareUnavailable { missing } => {
                assert_eq!(missing, vec!["R 4.4.1".to_owned()]);
            }
            other => panic!("expected ReferenceSoftwareUnavailable, got {other:?}"),
        }
    }

    #[test]
    fn version_mismatch_counts_as_unavailable() {
        let dir = temp_dir("ref-version-mismatch");
        let (_installed, _csv) = build_extracted_snapshot(&dir);

        // Right name, wrong version → still missing per Requirement 8.6
        // ("missing or version-mismatched").
        let err = execute_replay(ReplayPlan {
            extracted_dir: dir,
            installed_reference_software: vec![("R".to_owned(), "4.3.0".to_owned())],
        })
        .expect_err("version mismatch must be refused");

        match err {
            ReplayError::ReferenceSoftwareUnavailable { missing } => {
                assert_eq!(missing, vec!["R 4.4.1".to_owned()]);
            }
            other => panic!("expected ReferenceSoftwareUnavailable, got {other:?}"),
        }
    }

    // ---- Gate 3: input artifact SHA256 mismatch -------------------------

    #[test]
    fn input_artifact_sha256_mismatch_is_refused() {
        // The input for step-1 is `data.csv`. To exercise the *input*
        // gate independently, we build a snapshot whose workflow step
        // declares a *different* input artifact and then tamper that
        // artifact's bytes. The dataset gate (`data.csv`) still passes
        // because we leave the dataset content alone.
        let dir = temp_dir("input-sha-mismatch");

        // dataset
        let csv_bytes = b"col1,col2\n1,2\n".to_vec();
        let dataset_sha_hex = encode_hex_lower(&sha256_oneshot(&csv_bytes));
        fs::write(dir.join("data.csv"), &csv_bytes).unwrap();

        // separate input artifact, recorded with its true sha256
        let input_bytes = b"intermediate-payload".to_vec();
        let input_sha_hex = encode_hex_lower(&sha256_oneshot(&input_bytes));
        let input_path = "artifacts/step-0/intermediate.bin";
        fs::create_dir_all(dir.join("artifacts/step-0")).unwrap();
        fs::write(dir.join(input_path), &input_bytes).unwrap();

        // output artifact
        let out_bytes = b"out".to_vec();
        let out_sha_hex = encode_hex_lower(&sha256_oneshot(&out_bytes));
        let out_path = "artifacts/step-1/result.json";
        fs::create_dir_all(dir.join("artifacts/step-1")).unwrap();
        fs::write(dir.join(out_path), &out_bytes).unwrap();

        // manifest, versions
        let manifest = AuditSnapshotManifest {
            schema_version: MANIFEST_SCHEMA_VERSION,
            input_dataset_sha256: dataset_sha_hex.clone(),
            created_at_utc: "2024-01-01T00:00:00Z".to_owned(),
            stats_code_release_version: "0.5.0".to_owned(),
            stats_code_commit_sha: "0".repeat(40),
            run_id: "run-input".to_owned(),
            run_status: RUN_STATUS_COMPLETED.to_owned(),
        };
        fs::write(
            dir.join("manifest.json"),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();

        let versions = Versions {
            schema_version: VERSIONS_SCHEMA_VERSION,
            os_family: "Linux".to_owned(),
            os_version: "6.6.0".to_owned(),
            version_truncated: false,
            reference_software: Vec::new(),
            runtime_dependencies: BTreeMap::new(),
        };
        fs::write(
            dir.join("versions.json"),
            serde_json::to_vec(&versions).unwrap(),
        )
        .unwrap();

        // workflow with an input artifact that's NOT data.csv, so the
        // *input* gate fires after the dataset gate passes.
        let workflow = Workflow {
            schema_version: 1,
            input_dataset: InputDataset {
                path: "data.csv".to_owned(),
                sha256: dataset_sha_hex,
            },
            steps: vec![WorkflowStep {
                id: "step-1".to_owned(),
                algorithm: "tableone".to_owned(),
                params: serde_json::json!({}),
                inputs: vec![ArtifactRef {
                    path: input_path.to_owned(),
                    sha256: input_sha_hex.clone(),
                }],
                outputs: vec![ArtifactRef {
                    path: out_path.to_owned(),
                    sha256: out_sha_hex,
                }],
                reference_software: None,
                llm: None,
                started_at_utc: "2024-01-01T00:00:00Z".to_owned(),
                ended_at_utc: "2024-01-01T00:00:01Z".to_owned(),
            }],
        };
        fs::write(dir.join("workflow.yaml"), pretty_print(&workflow, None)).unwrap();

        // Tamper the input artifact bytes.
        let tampered = b"tampered-input-bytes".to_vec();
        fs::write(dir.join(input_path), &tampered).unwrap();

        let err = execute_replay(ReplayPlan {
            extracted_dir: dir,
            installed_reference_software: Vec::new(),
        })
        .expect_err("tampered input artifact must be refused");

        match err {
            ReplayError::InputArtifactSha256Mismatch {
                path,
                expected,
                actual,
            } => {
                assert_eq!(path, input_path);
                assert_eq!(expected, input_sha_hex);
                assert_eq!(actual, encode_hex_lower(&sha256_oneshot(&tampered)));
            }
            other => panic!("expected InputArtifactSha256Mismatch, got {other:?}"),
        }
    }

    // ---- Snapshot member missing ---------------------------------------

    #[test]
    fn missing_manifest_returns_io_error() {
        let dir = temp_dir("missing-manifest");
        // Don't build any files — `execute_replay` should fail on the
        // first read (manifest.json).
        let err = execute_replay(ReplayPlan {
            extracted_dir: dir.clone(),
            installed_reference_software: Vec::new(),
        })
        .expect_err("missing manifest must be refused");
        match err {
            ReplayError::SnapshotIo { path, .. } => {
                assert!(path.contains("manifest.json"), "got path={path}");
            }
            other => panic!("expected SnapshotIo, got {other:?}"),
        }
    }

    // ---- Hex encoding sanity -------------------------------------------

    #[test]
    fn encode_hex_lower_matches_known_vector() {
        // SHA256("") = e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
        let digest = sha256_oneshot(b"");
        let hex = encode_hex_lower(&digest);
        assert_eq!(
            hex,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }
}
