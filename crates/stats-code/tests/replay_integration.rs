//! Integration tests for `--replay` happy path and rejection path.
//!
//! Task 7.3: Happy path — build a valid snapshot fixture, replay it,
//! assert success and `steps_replayed` count.
//!
//! Task 7.4: Rejection path — corrupt `data.csv` (flip 1 byte), assert
//! replay refuses with non-zero / Err, error identifies mismatched
//! dataset path and both SHA256 values, snapshot file set unchanged.
//!
//! _Requirements: 8.3, 8.4_

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use stats_code::snapshot::replay::{execute_replay, ReplayError, ReplayOutcome, ReplayPlan};
use stats_code::snapshot::versions::{
    ReferenceSoftwareVersion, Versions, SCHEMA_VERSION as VERSIONS_SCHEMA_VERSION,
};
use stats_code::snapshot::workflow_yaml::{
    pretty_print, ArtifactRef, InputDataset, Workflow, WorkflowStep,
};

// We cannot use `snapshot::sha256_oneshot` from integration tests (pub(crate)),
// so we inline a minimal SHA-256 computation using the same FIPS 180-4 algorithm.
fn sha256(data: &[u8]) -> [u8; 32] {
    struct Sha256State {
        state: [u32; 8],
        buffer: [u8; 64],
        buffer_len: usize,
        total_len: u64,
    }

    impl Sha256State {
        fn new() -> Self {
            Self {
                state: [
                    0x6a09_e667, 0xbb67_ae85, 0x3c6e_f372, 0xa54f_f53a,
                    0x510e_527f, 0x9b05_688c, 0x1f83_d9ab, 0x5be0_cd19,
                ],
                buffer: [0u8; 64],
                buffer_len: 0,
                total_len: 0,
            }
        }

        fn update(&mut self, data: &[u8]) {
            self.total_len = self.total_len.wrapping_add(data.len() as u64);
            let mut cursor = 0usize;
            while cursor < data.len() {
                let take = (64 - self.buffer_len).min(data.len() - cursor);
                self.buffer[self.buffer_len..self.buffer_len + take]
                    .copy_from_slice(&data[cursor..cursor + take]);
                self.buffer_len += take;
                cursor += take;
                if self.buffer_len == 64 {
                    let block = self.buffer;
                    self.compress(&block);
                    self.buffer_len = 0;
                }
            }
        }

        fn finalize(mut self) -> [u8; 32] {
            let bit_len = self.total_len.wrapping_mul(8);
            self.update(&[0x80]);
            while self.buffer_len != 56 {
                self.update(&[0x00]);
            }
            self.update(&bit_len.to_be_bytes());
            let mut out = [0u8; 32];
            for (i, word) in self.state.iter().enumerate() {
                out[i * 4..i * 4 + 4].copy_from_slice(&word.to_be_bytes());
            }
            out
        }

        fn compress(&mut self, block: &[u8; 64]) {
            const K: [u32; 64] = [
                0x428a_2f98, 0x7137_4491, 0xb5c0_fbcf, 0xe9b5_dba5,
                0x3956_c25b, 0x59f1_11f1, 0x923f_82a4, 0xab1c_5ed5,
                0xd807_aa98, 0x1283_5b01, 0x2431_85be, 0x550c_7dc3,
                0x72be_5d74, 0x80de_b1fe, 0x9bdc_06a7, 0xc19b_f174,
                0xe49b_69c1, 0xefbe_4786, 0x0fc1_9dc6, 0x240c_a1cc,
                0x2de9_2c6f, 0x4a74_84aa, 0x5cb0_a9dc, 0x76f9_88da,
                0x983e_5152, 0xa831_c66d, 0xb003_27c8, 0xbf59_7fc7,
                0xc6e0_0bf3, 0xd5a7_9147, 0x06ca_6351, 0x1429_2967,
                0x27b7_0a85, 0x2e1b_2138, 0x4d2c_6dfc, 0x5338_0d13,
                0x650a_7354, 0x766a_0abb, 0x81c2_c92e, 0x9272_2c85,
                0xa2bf_e8a1, 0xa81a_664b, 0xc24b_8b70, 0xc76c_51a3,
                0xd192_e819, 0xd699_0624, 0xf40e_3585, 0x106a_a070,
                0x19a4_c116, 0x1e37_6c08, 0x2748_774c, 0x34b0_bcb5,
                0x391c_0cb3, 0x4ed8_aa4a, 0x5b9c_ca4f, 0x682e_6ff3,
                0x748f_82ee, 0x78a5_636f, 0x84c8_7814, 0x8cc7_0208,
                0x90be_fffa, 0xa450_6ceb, 0xbef9_a3f7, 0xc671_78f2,
            ];

            let mut w = [0u32; 64];
            for (i, w_i) in w.iter_mut().enumerate().take(16) {
                let off = i * 4;
                *w_i = u32::from_be_bytes([
                    block[off], block[off + 1], block[off + 2], block[off + 3],
                ]);
            }
            for i in 16..64 {
                let s0 = w[i - 15].rotate_right(7)
                    ^ w[i - 15].rotate_right(18)
                    ^ (w[i - 15] >> 3);
                let s1 = w[i - 2].rotate_right(17)
                    ^ w[i - 2].rotate_right(19)
                    ^ (w[i - 2] >> 10);
                w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
            }

            let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = self.state;
            for i in 0..64 {
                let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
                let ch = (e & f) ^ ((!e) & g);
                let t1 = h.wrapping_add(s1).wrapping_add(ch).wrapping_add(K[i]).wrapping_add(w[i]);
                let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
                let maj = (a & b) ^ (a & c) ^ (b & c);
                let t2 = s0.wrapping_add(maj);
                h = g; g = f; f = e; e = d.wrapping_add(t1);
                d = c; c = b; b = a; a = t1.wrapping_add(t2);
            }

            self.state[0] = self.state[0].wrapping_add(a);
            self.state[1] = self.state[1].wrapping_add(b);
            self.state[2] = self.state[2].wrapping_add(c);
            self.state[3] = self.state[3].wrapping_add(d);
            self.state[4] = self.state[4].wrapping_add(e);
            self.state[5] = self.state[5].wrapping_add(f);
            self.state[6] = self.state[6].wrapping_add(g);
            self.state[7] = self.state[7].wrapping_add(h);
        }
    }

    let mut hasher = Sha256State::new();
    hasher.update(data);
    hasher.finalize()
}

fn hex_lower(bytes: &[u8; 32]) -> String {
    let mut out = String::with_capacity(64);
    for b in bytes {
        use std::fmt::Write as _;
        let _ = write!(&mut out, "{b:02x}");
    }
    out
}

/// Per-test scratch directory under the OS temp root.
fn temp_dir(name: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "stats-code-replay-integ-{}-{}",
        name,
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&p);
    fs::create_dir_all(&p).expect("temp dir");
    p
}

/// Build a minimal valid extracted snapshot directory with one workflow
/// step and one artifact, all SHA256s computed correctly.
/// Returns (`installed_reference_software`, `csv_bytes`).
fn build_valid_snapshot(dir: &Path) -> (Vec<(String, String)>, Vec<u8>) {
    // dataset
    let csv_bytes = b"col1,col2\n1,2\n3,4\n5,6\n".to_vec();
    let dataset_sha = sha256(&csv_bytes);
    let dataset_hex = hex_lower(&dataset_sha);
    fs::write(dir.join("data.csv"), &csv_bytes).unwrap();

    // step-1 output artifact
    let out_bytes = br#"{"estimate": 2.5, "ci_lower": 1.1, "ci_upper": 3.9}"#.to_vec();
    let out_sha = sha256(&out_bytes);
    let out_sha_hex = hex_lower(&out_sha);
    let out_path = "artifacts/step-1/result.json";
    fs::create_dir_all(dir.join("artifacts/step-1")).unwrap();
    fs::write(dir.join(out_path), &out_bytes).unwrap();

    // step-1 input == data.csv
    let input_path = "data.csv".to_owned();
    let input_sha_hex = dataset_hex.clone();

    // manifest.json
    let manifest = serde_json::json!({
        "schema_version": 1,
        "input_dataset_sha256": dataset_hex,
        "created_at_utc": "2024-06-01T12:00:00Z",
        "stats_code_release_version": "0.5.0",
        "stats_code_commit_sha": "a".repeat(40),
        "run_id": "run-replay-integ",
        "run_status": "completed"
    });
    fs::write(
        dir.join("manifest.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();

    // versions.json
    let versions = Versions {
        schema_version: VERSIONS_SCHEMA_VERSION,
        os_family: "Windows".to_owned(),
        os_version: "10.0.22631".to_owned(),
        version_truncated: false,
        reference_software: vec![ReferenceSoftwareVersion {
            name: "R".to_owned(),
            version: "4.4.1".to_owned(),
        }],
        runtime_dependencies: BTreeMap::new(),
    };
    fs::write(
        dir.join("versions.json"),
        serde_json::to_vec(&versions).unwrap(),
    )
    .unwrap();

    // workflow.yaml
    let workflow = Workflow {
        schema_version: 1,
        input_dataset: InputDataset {
            path: "data.csv".to_owned(),
            sha256: dataset_hex,
        },
        steps: vec![WorkflowStep {
            id: "step-1".to_owned(),
            algorithm: "tableone".to_owned(),
            params: serde_json::json!({"by": "col1"}),
            inputs: vec![ArtifactRef {
                path: input_path,
                sha256: input_sha_hex,
            }],
            outputs: vec![ArtifactRef {
                path: out_path.to_owned(),
                sha256: out_sha_hex,
            }],
            reference_software: None,
            llm: None,
            started_at_utc: "2024-06-01T12:00:00Z".to_owned(),
            ended_at_utc: "2024-06-01T12:00:01Z".to_owned(),
        }],
    };
    fs::write(dir.join("workflow.yaml"), pretty_print(&workflow, None)).unwrap();

    // Host has R 4.4.1 installed
    let installed = vec![("R".to_owned(), "4.4.1".to_owned())];
    (installed, csv_bytes)
}

// ─────────────────────────────────────────────────────────────────────────────
// Task 7.3: --replay happy path
// _Requirements: 8.3_
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn replay_happy_path_succeeds_with_valid_snapshot() {
    let dir = temp_dir("happy");
    let (installed, _csv) = build_valid_snapshot(&dir);

    let outcome = execute_replay(ReplayPlan {
        extracted_dir: dir.clone(),
        installed_reference_software: installed,
    })
    .expect("replay of a valid snapshot must succeed");

    // The fixture has exactly 1 workflow step
    assert_eq!(outcome, ReplayOutcome { steps_replayed: 1 });
}

#[test]
fn replay_happy_path_all_snapshot_files_unchanged() {
    let dir = temp_dir("happy-unchanged");
    let (installed, csv_bytes) = build_valid_snapshot(&dir);

    // Record file contents before replay
    let manifest_before = fs::read(dir.join("manifest.json")).unwrap();
    let versions_before = fs::read(dir.join("versions.json")).unwrap();
    let workflow_before = fs::read(dir.join("workflow.yaml")).unwrap();

    let _outcome = execute_replay(ReplayPlan {
        extracted_dir: dir.clone(),
        installed_reference_software: installed,
    })
    .expect("replay must succeed");

    // Verify snapshot files are byte-identical after replay
    assert_eq!(fs::read(dir.join("manifest.json")).unwrap(), manifest_before);
    assert_eq!(fs::read(dir.join("versions.json")).unwrap(), versions_before);
    assert_eq!(fs::read(dir.join("workflow.yaml")).unwrap(), workflow_before);
    assert_eq!(fs::read(dir.join("data.csv")).unwrap(), csv_bytes);
}

// ─────────────────────────────────────────────────────────────────────────────
// Task 7.4: --replay rejection path (data.csv 1-byte flip)
// _Requirements: 8.4_
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn replay_rejects_corrupted_dataset_with_sha256_mismatch() {
    let dir = temp_dir("corrupt");
    let (installed, csv_bytes) = build_valid_snapshot(&dir);

    // Flip 1 byte in data.csv
    let mut corrupted = csv_bytes.clone();
    corrupted[0] ^= 0xFF; // flip first byte
    fs::write(dir.join("data.csv"), &corrupted).unwrap();

    // Record snapshot files before replay attempt
    let manifest_before = fs::read(dir.join("manifest.json")).unwrap();
    let versions_before = fs::read(dir.join("versions.json")).unwrap();
    let workflow_before = fs::read(dir.join("workflow.yaml")).unwrap();

    let err = execute_replay(ReplayPlan {
        extracted_dir: dir.clone(),
        installed_reference_software: installed,
    })
    .expect_err("replay must refuse corrupted data.csv");

    // Assert error identifies the mismatched dataset path and both SHA256 values
    match err {
        ReplayError::DatasetSha256Mismatch {
            ref path,
            ref expected,
            ref actual,
        } => {
            // Error identifies the dataset path
            assert_eq!(path, "data.csv");

            // Error contains both SHA256 values
            let expected_sha = hex_lower(&sha256(&csv_bytes));
            let actual_sha = hex_lower(&sha256(&corrupted));
            assert_eq!(*expected, expected_sha);
            assert_eq!(*actual, actual_sha);

            // The two SHA256 values are different
            assert_ne!(expected, actual);
        }
        other => panic!(
            "expected DatasetSha256Mismatch, got: {other:?}"
        ),
    }

    // Snapshot files are left untouched (Requirement 8.4)
    assert_eq!(fs::read(dir.join("manifest.json")).unwrap(), manifest_before);
    assert_eq!(fs::read(dir.join("versions.json")).unwrap(), versions_before);
    assert_eq!(fs::read(dir.join("workflow.yaml")).unwrap(), workflow_before);
}

#[test]
fn replay_rejection_error_display_contains_both_hashes() {
    let dir = temp_dir("corrupt-display");
    let (installed, csv_bytes) = build_valid_snapshot(&dir);

    // Flip 1 byte in data.csv
    let mut corrupted = csv_bytes.clone();
    corrupted[5] ^= 0x01;
    fs::write(dir.join("data.csv"), &corrupted).unwrap();

    let err = execute_replay(ReplayPlan {
        extracted_dir: dir,
        installed_reference_software: installed,
    })
    .expect_err("replay must refuse corrupted data.csv");

    // The Display impl should contain both SHA256 hex strings
    let display = format!("{err}");
    let expected_hex = hex_lower(&sha256(&csv_bytes));
    let actual_hex = hex_lower(&sha256(&corrupted));
    assert!(
        display.contains(&expected_hex),
        "error display must contain expected SHA256; got: {display}"
    );
    assert!(
        display.contains(&actual_hex),
        "error display must contain actual SHA256; got: {display}"
    );
    assert!(
        display.contains("data.csv"),
        "error display must identify the dataset path; got: {display}"
    );
}
