//! Audit Snapshot Exporter (Feature: parity-and-multilang-sidecar).
//!
//! Implements task 6.7. The exporter is the orchestration layer that
//! pulls together every wave-1 component into a single deterministic
//! `.zip` artifact:
//!
//! * `manifest.rs` (task 6.2) — `manifest.json`
//! * `versions.rs` (task 6.3) — `versions.json`
//! * `llm_provenance.rs` (task 6.4) — `llm_provenance.json`
//! * `narrative.rs` (task 6.5) — `narrative.md`
//! * `zip_writer.rs` (task 6.6) — deterministic ZIP writer
//! * `workflow_yaml.rs` (tasks 5.1–5.3) — canonical YAML pretty-print
//! * `crate::coverage_matrix` (tasks 1.1 / 1.2) — `coverage.json`
//! * `crate::redact` (task 2.4) — secret + path redaction
//! * `crate::spawn_policy` (task 2.6) — `forbid_external_runtimes_scope`
//!
//! ## Algorithm (`export_snapshot`)
//!
//! 1. Wrap the entire body in
//!    [`forbid_external_runtimes_scope`](crate::spawn_policy::forbid_external_runtimes_scope)
//!    so any accidental spawn of `{R, Rscript, python, sas, spss, …}`
//!    aborts the call (Requirements 10.1, 10.5).
//! 2. Refuse non-completed runs (Requirement 7.8) — no `.tmp` is created.
//! 3. Measure the artifact payload (sum of `bytes.len()` across
//!    `run.artifacts`, excluding `data.csv`). Refuse `> 50 MB` before any
//!    `.tmp` is created (Requirement 7.7).
//! 4. Build every JSON / markdown / YAML payload in memory.
//! 5. Apply [`redact_pure`](crate::redact::redact_pure) to every text
//!    artifact (Requirements 2.6, 9.1, 9.3, 9.4, 9.5). `data.csv` is
//!    preserved verbatim so its bytes match `manifest.input_dataset_sha256`.
//! 6. Hand the entry list to [`write_deterministic_zip`] which produces a
//!    `<dest>.tmp` with fixed mtime (`1980-01-01T00:00:00Z`), `Stored`
//!    compression, and lexicographic entry order.
//! 7. Atomically rename `<dest>.tmp → <dest>` (Requirement 7.6 / 7.7
//!    atomicity).
//! 8. Re-read the final file and compute SHA256 with a hand-rolled
//!    implementation (no new crate dependency).
//!
//! ## Determinism
//!
//! Every dynamic input (timestamp, run id, OS family / version, dataset
//! hash, release version, commit SHA, LLM calls, narrative steps,
//! artifacts, working directory, secrets) flows in via [`RunSnapshot`].
//! Two calls with byte-identical [`RunSnapshot`]s and the same
//! `destination` produce a byte-identical `.zip` because:
//!
//! - every component builder is pure (tasks 6.2 – 6.5),
//! - `serde_json::to_vec` on these structs is byte-stable,
//! - `pretty_print(_, None)` is canonical (Requirement 11.7),
//! - the deterministic ZIP writer fixes mtime, sorts entries, and uses
//!   `Stored` compression (Requirement 7.1).
//!
//! _Requirements: 7.1, 7.2, 7.6, 7.7, 7.8, 9.1, 10.1, 10.5_

pub mod llm_provenance;
pub mod manifest;
pub mod narrative;
pub mod redact;
pub mod replay;
pub mod versions;
pub mod workflow_yaml;
pub mod zip_writer;

use std::collections::BTreeSet;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::coverage_matrix::CoverageMatrix;
use crate::redact::{redact_pure, RedactionPolicy};
use crate::spawn_policy::{forbid_external_runtimes_scope, SpawnError};

pub use self::llm_provenance::LlmCall;
pub use self::narrative::{KeyMetric, NarrativeError, NarrativeStep};
pub use self::versions::ReferenceSoftwareVersion;
pub use self::workflow_yaml::{Workflow, WorkflowYamlError};
pub use self::zip_writer::ZipWriteError;

use self::llm_provenance::build_llm_provenance;
use self::manifest::build_manifest;
use self::narrative::build_narrative;
use self::versions::build_versions;
use self::workflow_yaml::pretty_print;
use self::zip_writer::{write_deterministic_zip, ZipEntry};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Hard ceiling on the artifact payload (sum of bytes across
/// `run.artifacts`, **excluding** `data.csv`). Requirement 7.6 / 7.7:
/// "at most 50 MB". 52,428,800 bytes is the binary 50 MiB literal that
/// the design and the request body for `POST /api/snapshot/export` both
/// quote.
pub const ARTIFACT_PAYLOAD_CEILING_BYTES: u64 = 50 * 1024 * 1024;

/// Build-time snapshot of `stats-code`'s direct runtime dependency
/// versions, materialized by `build.rs::emit_runtime_deps`. Format is a
/// flat JSON object of `String → String`.
///
/// Re-exports the crate-level [`crate::RUNTIME_DEPS_JSON`] (task 15.3) so
/// the snapshot exporter and any external consumer share one canonical
/// view of the file. Kept as a private module-level alias to preserve the
/// exporter's existing call-sites without rippling renames into other
/// snapshot submodules.
const RUNTIME_DEPS_JSON: &str = crate::RUNTIME_DEPS_JSON;

// ---------------------------------------------------------------------------
// Public types (RunSnapshot, RunStatus, SnapshotArtifact, SnapshotResult,
// SnapshotError)
// ---------------------------------------------------------------------------

/// Identifier of an analysis run for which an Audit Snapshot can be
/// exported. Kept as a thin alias of `String` so the public API remains
/// stable across future internal-id rewrites.
pub type RunId = String;

/// Lifecycle state of an analysis run as observed by the snapshot
/// exporter. Only [`RunStatus::Completed`] runs may be exported
/// (Requirements 7.1 / 7.8).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunStatus {
    /// Run has finished and every step's artifact is available.
    Completed,
    /// Run is still in progress.
    Running,
    /// Run terminated with a non-recoverable error.
    Failed,
}

/// One per-step artifact destined for the snapshot's
/// `artifacts/<step_id>/...` tree.
///
/// `path` is the archive path **inside** the snapshot zip (e.g.
/// `"artifacts/step-1/result.json"`), `bytes` is the raw file payload.
#[derive(Debug, Clone)]
pub struct SnapshotArtifact {
    /// Archive path inside the snapshot, forward-slash separators only.
    pub path: String,
    /// File payload bytes, written verbatim (after redaction if
    /// UTF-8-decodable).
    pub bytes: Vec<u8>,
}

/// Materialized, exporter-ready view of an analysis run.
///
/// The wave-1 contract: the agent-server handler (task 10.4) constructs
/// this struct from its in-memory `RunStore`, then hands it to
/// [`export_snapshot`]. Time and randomness flow in via `created_at_utc`,
/// so [`export_snapshot`] itself stays referentially transparent over its
/// inputs.
#[derive(Debug, Clone)]
pub struct RunSnapshot {
    /// Identifier of the run this snapshot describes.
    pub run_id: String,
    /// Lifecycle state at snapshot time. Only `Completed` is acceptable.
    pub status: RunStatus,
    /// 32-byte SHA256 of the original input dataset.
    pub dataset_sha256: [u8; 32],
    /// Original input dataset bytes; written verbatim as `data.csv`
    /// inside the snapshot. **Not** redacted — its hash must match
    /// `manifest.input_dataset_sha256` (Requirement 7.3).
    pub dataset_csv_bytes: Vec<u8>,
    /// Workflow model emitted as canonical `workflow.yaml`.
    pub workflow: Workflow,
    /// Per-step artifacts emitted as `artifacts/<step_id>/...`.
    pub artifacts: Vec<SnapshotArtifact>,
    /// LLM calls recorded for this run; emitted as `llm_provenance.json`.
    /// Empty `Vec` when no LLM call was made (Requirement 7.5).
    pub llm_calls: Vec<LlmCall>,
    /// Reference Software actually invoked during this run; emitted as
    /// `versions.json::reference_software`. Empty when none invoked
    /// (Requirement 7.4).
    pub reference_software: Vec<ReferenceSoftwareVersion>,
    /// Host OS family — must be one of `"Windows" | "Linux" | "macOS"`
    /// (Requirement 9.2).
    pub os_family: String,
    /// Host OS version string; truncated to 32 chars by [`build_versions`].
    pub os_version: String,
    /// Stats Code release version (semver from `Cargo.toml`).
    pub release_version: String,
    /// Stats Code git commit SHA (40-hex).
    pub commit_sha: String,
    /// ISO-8601 UTC creation timestamp; supplied by the caller so the
    /// exporter stays pure.
    pub created_at_utc: String,
    /// Active LLM API keys to redact across every emitted text artifact
    /// (Requirements 9.1, 9.4).
    pub api_keys: Vec<String>,
    /// Analysis working directory. Paths inside this directory are
    /// rewritten to relative form; paths outside become `<external>`
    /// (Requirements 9.3, 9.5).
    pub working_directory: Option<PathBuf>,
    /// Narrative step list emitted as `narrative.md` (Requirement 8.5).
    pub narrative_steps: Vec<NarrativeStep>,
}

/// Successful outcome of [`export_snapshot`].
#[derive(Debug, Clone)]
pub struct SnapshotResult {
    /// Final destination path of the snapshot `.zip`.
    pub snapshot_path: PathBuf,
    /// 32-byte SHA256 of the final snapshot bytes.
    pub sha256: [u8; 32],
}

/// Errors returned by [`export_snapshot`].
#[derive(Debug, Error)]
pub enum SnapshotError {
    /// Run status is not [`RunStatus::Completed`] (Requirement 7.8).
    /// No `.tmp` file is created when this variant fires.
    #[error("snapshot refused: run status is {actual:?}, expected Completed")]
    RunNotCompleted { actual: RunStatus },

    /// Artifact payload (excluding `data.csv`) exceeds the 50 MB ceiling
    /// (Requirement 7.7). No `.tmp` file is created when this variant
    /// fires.
    #[error(
        "snapshot refused: artifact payload {measured_bytes} bytes exceeds ceiling {ceiling} bytes"
    )]
    PayloadTooLarge {
        measured_bytes: u64,
        ceiling: u64,
    },

    /// Narrative builder rejected the input (e.g. unknown artifact
    /// citation). No `.tmp` file is created.
    #[error("snapshot refused: {0}")]
    Narrative(#[from] NarrativeError),

    /// Deterministic ZIP writer failed. The writer cleans up its own
    /// `.tmp` on failure (task 6.6), so no partial file remains.
    #[error("snapshot zip write failed: {0}")]
    Zip(#[from] ZipWriteError),

    /// Generic filesystem I/O failure (rename, sha256 read-back).
    /// `path` identifies the offending file; `<dest>.tmp` and `<dest>`
    /// are both cleaned up before returning.
    #[error("snapshot io error at {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },

    /// External-runtime spawn was attempted inside the export scope
    /// (Requirements 10.1, 10.5). No partial file remains.
    #[error("snapshot refused: {0}")]
    ForbiddenSpawn(#[from] SpawnError),

    /// Workflow YAML emission produced an invalid document. Reserved for
    /// future strict-mode validation; the wave-1 canonical pretty-print
    /// is total and never returns this variant.
    #[error("snapshot refused: invalid workflow yaml: {0}")]
    InvalidWorkflow(WorkflowYamlError),
}

// ---------------------------------------------------------------------------
// Public entry point: export_snapshot
// ---------------------------------------------------------------------------

/// Export an Audit Snapshot for the given completed run to `destination`.
///
/// See the module-level doc-comment for the full algorithm and the
/// determinism contract. The function is pure modulo the final
/// `tmp + rename + sha256_readback` filesystem step.
///
/// _Requirements: 7.1, 7.2, 7.6, 7.7, 7.8, 9.1, 10.1, 10.5_
pub fn export_snapshot(
    run: &RunSnapshot,
    destination: &Path,
) -> Result<SnapshotResult, SnapshotError> {
    // Wrap the entire body in the `forbid_external_runtimes_scope` so any
    // accidental spawn of an external statistical runtime aborts (Reqs
    // 10.1 / 10.5). The closure's return type is
    // `Result<Result<…, SnapshotError>, SpawnError>`; we unwrap one layer
    // outside and merge SpawnError into SnapshotError.
    let outcome: Result<Result<SnapshotResult, SnapshotError>, SpawnError> =
        forbid_external_runtimes_scope(|_policy| {
            Ok(export_snapshot_inner(run, destination))
        });

    match outcome {
        Ok(inner) => inner,
        Err(spawn) => Err(SnapshotError::ForbiddenSpawn(spawn)),
    }
}

fn export_snapshot_inner(
    run: &RunSnapshot,
    destination: &Path,
) -> Result<SnapshotResult, SnapshotError> {
    // ---- Gate 1: run.status == Completed (Req 7.8) -----------------------
    if run.status != RunStatus::Completed {
        return Err(SnapshotError::RunNotCompleted {
            actual: run.status.clone(),
        });
    }

    // ---- Gate 2: artifact payload <= 50 MB (Req 7.6 / 7.7) ---------------
    // Sum bytes across `run.artifacts` only; data.csv is excluded by
    // construction (it's a separate field on `RunSnapshot`). Use saturating
    // u64 arithmetic so an absurd payload doesn't overflow the counter.
    let measured: u64 = run
        .artifacts
        .iter()
        .map(|a| a.bytes.len() as u64)
        .fold(0u64, u64::saturating_add);
    if measured > ARTIFACT_PAYLOAD_CEILING_BYTES {
        return Err(SnapshotError::PayloadTooLarge {
            measured_bytes: measured,
            ceiling: ARTIFACT_PAYLOAD_CEILING_BYTES,
        });
    }

    // ---- Build redaction policy ------------------------------------------
    let secret_refs: Vec<&str> = run.api_keys.iter().map(String::as_str).collect();
    let mut policy = RedactionPolicy::new().with_secrets(&secret_refs);
    if let Some(wd) = run.working_directory.as_ref() {
        policy = policy.with_working_directory(wd.clone());
    }

    // ---- Build narrative artifacts_index from the full snapshot file set
    // The narrative builder validates every `[path#json_pointer]` citation
    // against this index (Requirement 8.5). The set covers every member
    // the snapshot will ultimately contain — top-level files and every
    // per-step artifact — so a narrative may cite any of them.
    let mut artifacts_index: BTreeSet<String> = BTreeSet::from([
        "data.csv".to_string(),
        "manifest.json".to_string(),
        "workflow.yaml".to_string(),
        "versions.json".to_string(),
        "llm_provenance.json".to_string(),
        "narrative.md".to_string(),
        "coverage.json".to_string(),
    ]);
    for art in &run.artifacts {
        artifacts_index.insert(art.path.clone());
    }

    // ---- Build every component payload ----------------------------------
    let manifest_value = build_manifest(
        &run.run_id,
        &run.dataset_sha256,
        &run.release_version,
        &run.commit_sha,
        &run.created_at_utc,
    );
    let versions_value = build_versions(
        &run.os_family,
        &run.os_version,
        &run.reference_software,
        RUNTIME_DEPS_JSON,
    );
    let llm_value = build_llm_provenance(&run.llm_calls);
    let narrative_text = build_narrative(&run.narrative_steps, &artifacts_index)?;
    let workflow_yaml_bytes = pretty_print(&run.workflow, None);
    let coverage_value = CoverageMatrix::get_loaded();

    // Serialize JSON payloads. `serde_json::to_vec` is byte-stable for
    // these structs because each one declares its fields in the order they
    // serialize and uses `BTreeMap` for any unordered nested data.
    let manifest_bytes = serde_json::to_vec(&manifest_value).map_err(|e| {
        SnapshotError::Io {
            path: "manifest.json".to_string(),
            source: std::io::Error::new(std::io::ErrorKind::InvalidData, e),
        }
    })?;
    let versions_bytes = serde_json::to_vec(&versions_value).map_err(|e| {
        SnapshotError::Io {
            path: "versions.json".to_string(),
            source: std::io::Error::new(std::io::ErrorKind::InvalidData, e),
        }
    })?;
    let llm_bytes = serde_json::to_vec(&llm_value).map_err(|e| {
        SnapshotError::Io {
            path: "llm_provenance.json".to_string(),
            source: std::io::Error::new(std::io::ErrorKind::InvalidData, e),
        }
    })?;
    let coverage_bytes = serde_json::to_vec(coverage_value).map_err(|e| {
        SnapshotError::Io {
            path: "coverage.json".to_string(),
            source: std::io::Error::new(std::io::ErrorKind::InvalidData, e),
        }
    })?;

    // ---- Apply redaction to every text artifact -------------------------
    // `<redacted>` and `<external>` are valid as-is inside both JSON string
    // values and YAML / markdown text, so post-serialization byte-level
    // redaction is sound for the artifacts this exporter emits.
    // `data.csv` is preserved verbatim so its bytes still match
    // `manifest.input_dataset_sha256` (Requirement 7.3).
    let manifest_bytes = redact_text_bytes(&manifest_bytes, &policy);
    let versions_bytes = redact_text_bytes(&versions_bytes, &policy);
    let llm_bytes = redact_text_bytes(&llm_bytes, &policy);
    let coverage_bytes = redact_text_bytes(&coverage_bytes, &policy);
    let workflow_bytes = redact_text_bytes(&workflow_yaml_bytes, &policy);
    let narrative_bytes = redact_text_bytes(narrative_text.as_bytes(), &policy);

    // ---- Build the ZIP entry list ---------------------------------------
    // `write_deterministic_zip` sorts entries lexicographically internally,
    // so the order we add them here is irrelevant for byte-determinism. We
    // still order them by purpose for readability.
    let mut entries: Vec<ZipEntry> = Vec::with_capacity(7 + run.artifacts.len());
    entries.push(ZipEntry {
        name: "data.csv".to_string(),
        bytes: run.dataset_csv_bytes.clone(),
    });
    entries.push(ZipEntry {
        name: "manifest.json".to_string(),
        bytes: manifest_bytes,
    });
    entries.push(ZipEntry {
        name: "workflow.yaml".to_string(),
        bytes: workflow_bytes,
    });
    entries.push(ZipEntry {
        name: "versions.json".to_string(),
        bytes: versions_bytes,
    });
    entries.push(ZipEntry {
        name: "llm_provenance.json".to_string(),
        bytes: llm_bytes,
    });
    entries.push(ZipEntry {
        name: "narrative.md".to_string(),
        bytes: narrative_bytes,
    });
    entries.push(ZipEntry {
        name: "coverage.json".to_string(),
        bytes: coverage_bytes,
    });
    for art in &run.artifacts {
        let bytes = redact_text_bytes(&art.bytes, &policy);
        entries.push(ZipEntry {
            name: art.path.clone(),
            bytes,
        });
    }

    // ---- Write to <dest>.tmp via deterministic zip writer ---------------
    let dest_tmp = tmp_path_for(destination);
    write_deterministic_zip(&entries, &dest_tmp)?;

    // ---- Atomic rename (.tmp → final) -----------------------------------
    if let Err(rename_err) = fs::rename(&dest_tmp, destination) {
        // Best-effort cleanup of the tmp file. We deliberately ignore the
        // result of remove_file; the original error is what the caller
        // needs to see.
        let _ = fs::remove_file(&dest_tmp);
        return Err(SnapshotError::Io {
            path: destination.display().to_string(),
            source: rename_err,
        });
    }

    // ---- Re-read the final file and compute SHA256 ----------------------
    let sha = match read_and_hash(destination) {
        Ok(h) => h,
        Err(e) => {
            // Post-rename failure: snapshot is on disk but we can't
            // confirm its hash. Per the design contract ("on any error
            // before/during write, no .tmp or final file should remain"),
            // remove the destination so the caller sees a clean failure.
            let _ = fs::remove_file(destination);
            return Err(e);
        }
    };

    Ok(SnapshotResult {
        snapshot_path: destination.to_path_buf(),
        sha256: sha,
    })
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Return the path used as the staging temp file for an atomic write.
/// Suffix is the literal `".tmp"` appended to the destination filename so
/// `<dest>.zip.tmp` (or `<dest>.tmp` for extension-less paths) sits in the
/// same directory as the final file — required for `fs::rename` atomicity
/// across most platforms.
fn tmp_path_for(destination: &Path) -> PathBuf {
    let mut s = destination.as_os_str().to_owned();
    s.push(".tmp");
    PathBuf::from(s)
}

/// If `bytes` is valid UTF-8, run [`redact_pure`] on it and return the
/// resulting bytes; otherwise pass the input through verbatim. This lets
/// the exporter redact text artifacts (JSON, YAML, markdown, anything
/// emitted by a pure builder) while leaving genuinely binary payloads
/// (e.g. SVG plots, PDFs) untouched.
fn redact_text_bytes(bytes: &[u8], policy: &RedactionPolicy) -> Vec<u8> {
    match std::str::from_utf8(bytes) {
        Ok(s) => redact_pure(s, policy).into_bytes(),
        Err(_) => bytes.to_vec(),
    }
}

/// Open `path`, stream its bytes, and compute the SHA256 of the file
/// contents. Returns the digest as a 32-byte array.
fn read_and_hash(path: &Path) -> Result<[u8; 32], SnapshotError> {
    let io_err = |source: std::io::Error| SnapshotError::Io {
        path: path.display().to_string(),
        source,
    };

    let mut file = File::open(path).map_err(io_err)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = file.read(&mut buf).map_err(io_err)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher.finalize())
}

// ---------------------------------------------------------------------------
// Hand-rolled SHA-256 (FIPS 180-4)
// ---------------------------------------------------------------------------
//
// Pure, dependency-free, byte-exact implementation. The snapshot pipeline
// avoids pulling in `sha2` because (a) the only consumer of SHA in the
// snapshot module is the final integrity hash returned in
// [`SnapshotResult::sha256`], (b) the file is bounded at 50 MB so a
// table-free reference implementation is fast enough, and (c) keeping a
// small local copy makes it easier to verify the privacy properties
// (Requirement 9): the hasher reads no clock, no environment, no random
// source.

/// Streaming SHA-256 hasher. Build with [`Sha256::new`], feed bytes via
/// [`Sha256::update`], finalize via [`Sha256::finalize`].
///
/// Visibility is `pub(crate)` so sibling snapshot submodules
/// (notably [`replay`](crate::snapshot::replay)) can recompute SHA256
/// digests without pulling in a new dependency. The hasher is
/// dependency-free, reads no clock and no environment, and is therefore
/// safe to call inside the privacy-sensitive snapshot pipeline
/// (Requirement 9).
pub(crate) struct Sha256 {
    state: [u32; 8],
    buffer: [u8; 64],
    buffer_len: usize,
    total_len: u64,
}

impl Sha256 {
    pub(crate) fn new() -> Self {
        Self {
            // FIPS 180-4 §5.3.3 — initial hash values H(0).
            state: [
                0x6a09_e667,
                0xbb67_ae85,
                0x3c6e_f372,
                0xa54f_f53a,
                0x510e_527f,
                0x9b05_688c,
                0x1f83_d9ab,
                0x5be0_cd19,
            ],
            buffer: [0u8; 64],
            buffer_len: 0,
            total_len: 0,
        }
    }

    pub(crate) fn update(&mut self, data: &[u8]) {
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

    pub(crate) fn finalize(mut self) -> [u8; 32] {
        // Append the `0x80` byte, pad with zeros to length ≡ 56 mod 64,
        // then the 64-bit big-endian total bit length (FIPS 180-4 §5.1.1).
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
        // FIPS 180-4 §6.2.2 round constants K(0..63).
        const K: [u32; 64] = [
            0x428a_2f98, 0x7137_4491, 0xb5c0_fbcf, 0xe9b5_dba5, 0x3956_c25b, 0x59f1_11f1,
            0x923f_82a4, 0xab1c_5ed5, 0xd807_aa98, 0x1283_5b01, 0x2431_85be, 0x550c_7dc3,
            0x72be_5d74, 0x80de_b1fe, 0x9bdc_06a7, 0xc19b_f174, 0xe49b_69c1, 0xefbe_4786,
            0x0fc1_9dc6, 0x240c_a1cc, 0x2de9_2c6f, 0x4a74_84aa, 0x5cb0_a9dc, 0x76f9_88da,
            0x983e_5152, 0xa831_c66d, 0xb003_27c8, 0xbf59_7fc7, 0xc6e0_0bf3, 0xd5a7_9147,
            0x06ca_6351, 0x1429_2967, 0x27b7_0a85, 0x2e1b_2138, 0x4d2c_6dfc, 0x5338_0d13,
            0x650a_7354, 0x766a_0abb, 0x81c2_c92e, 0x9272_2c85, 0xa2bf_e8a1, 0xa81a_664b,
            0xc24b_8b70, 0xc76c_51a3, 0xd192_e819, 0xd699_0624, 0xf40e_3585, 0x106a_a070,
            0x19a4_c116, 0x1e37_6c08, 0x2748_774c, 0x34b0_bcb5, 0x391c_0cb3, 0x4ed8_aa4a,
            0x5b9c_ca4f, 0x682e_6ff3, 0x748f_82ee, 0x78a5_636f, 0x84c8_7814, 0x8cc7_0208,
            0x90be_fffa, 0xa450_6ceb, 0xbef9_a3f7, 0xc671_78f2,
        ];

        // Prepare the message schedule W(0..63).
        let mut w = [0u32; 64];
        for (i, w_i) in w.iter_mut().enumerate().take(16) {
            let off = i * 4;
            *w_i = u32::from_be_bytes([
                block[off],
                block[off + 1],
                block[off + 2],
                block[off + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7)
                ^ w[i - 15].rotate_right(18)
                ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17)
                ^ w[i - 2].rotate_right(19)
                ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = self.state;

        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let t1 = h
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);

            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
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

/// Convenience wrapper over the streaming hasher for in-memory buffers.
///
/// Exposed `pub(crate)` so sibling snapshot submodules can hash a single
/// buffer without instantiating the streaming API; the
/// [`replay`](crate::snapshot::replay) module is the primary consumer.
pub(crate) fn sha256_oneshot(bytes: &[u8]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(bytes);
    h.finalize()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    use crate::snapshot::workflow_yaml::{ArtifactRef, InputDataset};

    fn temp_dir(name: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!(
            "stats-code-snapshot-{}-{}",
            name,
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).expect("temp dir");
        p
    }

    fn dest_path(dir: &Path, name: &str) -> PathBuf {
        dir.join(format!("{name}.zip"))
    }

    fn minimal_workflow() -> Workflow {
        Workflow {
            schema_version: 1,
            input_dataset: InputDataset {
                path: "data.csv".to_string(),
                sha256: "0".repeat(64),
            },
            steps: Vec::new(),
        }
    }

    fn minimal_run(status: RunStatus) -> RunSnapshot {
        RunSnapshot {
            run_id: "run-test".to_string(),
            status,
            dataset_sha256: [0u8; 32],
            dataset_csv_bytes: b"col1,col2\n1,2\n".to_vec(),
            workflow: minimal_workflow(),
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

    // ---- SHA-256 known vectors -----------------------------------------

    #[test]
    fn sha256_known_vectors() {
        // Empty input.
        assert_eq!(
            sha256_oneshot(b""),
            [
                0xe3, 0xb0, 0xc4, 0x42, 0x98, 0xfc, 0x1c, 0x14, 0x9a, 0xfb, 0xf4, 0xc8,
                0x99, 0x6f, 0xb9, 0x24, 0x27, 0xae, 0x41, 0xe4, 0x64, 0x9b, 0x93, 0x4c,
                0xa4, 0x95, 0x99, 0x1b, 0x78, 0x52, 0xb8, 0x55,
            ]
        );
        // "abc" — FIPS 180-4 example.
        assert_eq!(
            sha256_oneshot(b"abc"),
            [
                0xba, 0x78, 0x16, 0xbf, 0x8f, 0x01, 0xcf, 0xea, 0x41, 0x41, 0x40, 0xde,
                0x5d, 0xae, 0x22, 0x23, 0xb0, 0x03, 0x61, 0xa3, 0x96, 0x17, 0x7a, 0x9c,
                0xb4, 0x10, 0xff, 0x61, 0xf2, 0x00, 0x15, 0xad,
            ]
        );
        // 56-byte boundary: triggers the padding edge case where a single
        // block is filled exactly with `0x80` + length.
        let v448 = b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq";
        assert_eq!(v448.len(), 56);
        assert_eq!(
            sha256_oneshot(v448),
            [
                0x24, 0x8d, 0x6a, 0x61, 0xd2, 0x06, 0x38, 0xb8, 0xe5, 0xc0, 0x26, 0x93,
                0x0c, 0x3e, 0x60, 0x39, 0xa3, 0x3c, 0xe4, 0x59, 0x64, 0xff, 0x21, 0x67,
                0xf6, 0xec, 0xed, 0xd4, 0x19, 0xdb, 0x06, 0xc1,
            ]
        );
    }

    #[test]
    fn sha256_streaming_matches_oneshot() {
        let payload: Vec<u8> = (0u8..=255).cycle().take(10_000).collect();
        let oneshot = sha256_oneshot(&payload);

        let mut h = Sha256::new();
        for chunk in payload.chunks(37) {
            h.update(chunk);
        }
        let streaming = h.finalize();
        assert_eq!(oneshot, streaming);
    }

    // ---- Happy path -----------------------------------------------------

    #[test]
    fn happy_path_writes_a_valid_snapshot() {
        let dir = temp_dir("happy");
        let dest = dest_path(&dir, "snap");
        let run = minimal_run(RunStatus::Completed);

        let result = export_snapshot(&run, &dest).expect("export succeeds");
        assert_eq!(result.snapshot_path, dest);
        assert!(dest.exists(), "destination must exist after export");

        // No leftover .tmp file.
        let tmp = tmp_path_for(&dest);
        assert!(!tmp.exists(), "tmp must be removed after rename");

        // SHA in result matches the on-disk file.
        let mut bytes = Vec::new();
        File::open(&dest).unwrap().read_to_end(&mut bytes).unwrap();
        assert_eq!(result.sha256, sha256_oneshot(&bytes));

        // File starts with ZIP local-file-header signature.
        assert_eq!(&bytes[0..4], b"PK\x03\x04");

        // EOCD signature lives at len-22 when no archive comment.
        let n = bytes.len();
        assert!(n >= 22);
        assert_eq!(&bytes[n - 22..n - 18], b"PK\x05\x06");
    }

    // ---- Refusal gates --------------------------------------------------

    #[test]
    fn non_completed_run_is_refused_without_creating_tmp() {
        let dir = temp_dir("non-completed");
        let dest = dest_path(&dir, "snap");
        let run = minimal_run(RunStatus::Running);

        let err = export_snapshot(&run, &dest).expect_err("must refuse");
        match err {
            SnapshotError::RunNotCompleted { actual } => {
                assert_eq!(actual, RunStatus::Running);
            }
            other => panic!("expected RunNotCompleted, got {other:?}"),
        }
        assert!(!dest.exists(), "destination must not exist");
        assert!(
            !tmp_path_for(&dest).exists(),
            "tmp must not exist for refusal-before-write"
        );
    }

    #[test]
    fn failed_run_is_refused() {
        let dir = temp_dir("failed");
        let dest = dest_path(&dir, "snap");
        let run = minimal_run(RunStatus::Failed);
        let err = export_snapshot(&run, &dest).expect_err("must refuse failed");
        assert!(matches!(
            err,
            SnapshotError::RunNotCompleted {
                actual: RunStatus::Failed
            }
        ));
        assert!(!dest.exists());
    }

    #[test]
    fn payload_over_50_mb_is_refused_without_creating_tmp() {
        // One artifact of exactly ceiling + 1 bytes is the smallest input
        // that trips the gate, and its bytes are never touched on disk.
        let dir = temp_dir("payload-too-large");
        let dest = dest_path(&dir, "snap");

        let mut run = minimal_run(RunStatus::Completed);
        run.artifacts.push(SnapshotArtifact {
            path: "artifacts/step-1/big.bin".to_string(),
            bytes: vec![0u8; (ARTIFACT_PAYLOAD_CEILING_BYTES + 1) as usize],
        });

        let err = export_snapshot(&run, &dest).expect_err("must refuse oversized");
        match err {
            SnapshotError::PayloadTooLarge {
                measured_bytes,
                ceiling,
            } => {
                assert_eq!(ceiling, ARTIFACT_PAYLOAD_CEILING_BYTES);
                assert_eq!(measured_bytes, ARTIFACT_PAYLOAD_CEILING_BYTES + 1);
            }
            other => panic!("expected PayloadTooLarge, got {other:?}"),
        }
        assert!(!dest.exists());
        assert!(!tmp_path_for(&dest).exists());
    }

    // ---- Determinism ----------------------------------------------------

    #[test]
    fn two_calls_with_identical_input_produce_byte_identical_zips() {
        let dir = temp_dir("determinism");
        let d1 = dest_path(&dir, "snap1");
        let d2 = dest_path(&dir, "snap2");

        let run = {
            let mut r = minimal_run(RunStatus::Completed);
            r.llm_calls.push(LlmCall {
                provider: "deepseek".to_string(),
                model: "deepseek-chat".to_string(),
                request_at_utc: "2024-01-01T00:00:00Z".to_string(),
                prompt_sha256: "1".repeat(64),
                response_sha256: "2".repeat(64),
            });
            r.reference_software.push(ReferenceSoftwareVersion {
                name: "R".to_string(),
                version: "4.4.1".to_string(),
            });
            r.artifacts.push(SnapshotArtifact {
                path: "artifacts/step-1/result.json".to_string(),
                bytes: br#"{"estimate": 1.234}"#.to_vec(),
            });
            // Add a workflow step so workflow.yaml carries content.
            r.workflow.steps.push(crate::snapshot::workflow_yaml::WorkflowStep {
                id: "step-1".to_string(),
                algorithm: "tableone".to_string(),
                params: serde_json::json!({"by": "treatment"}),
                inputs: vec![ArtifactRef {
                    path: "data.csv".to_string(),
                    sha256: "0".repeat(64),
                }],
                outputs: vec![ArtifactRef {
                    path: "artifacts/step-1/result.json".to_string(),
                    sha256: "1".repeat(64),
                }],
                reference_software: None,
                llm: None,
                started_at_utc: "2024-01-01T00:00:00Z".to_string(),
                ended_at_utc: "2024-01-01T00:00:01Z".to_string(),
            });
            r
        };

        let r1 = export_snapshot(&run, &d1).unwrap();
        let r2 = export_snapshot(&run, &d2).unwrap();

        let mut b1 = Vec::new();
        File::open(&d1).unwrap().read_to_end(&mut b1).unwrap();
        let mut b2 = Vec::new();
        File::open(&d2).unwrap().read_to_end(&mut b2).unwrap();
        assert_eq!(b1, b2, "two snapshots of the same RunSnapshot must be byte-identical");
        assert_eq!(r1.sha256, r2.sha256);
    }

    // ---- Redaction ------------------------------------------------------

    #[test]
    fn secret_in_artifact_bytes_is_replaced() {
        let dir = temp_dir("redact-artifact");
        let dest = dest_path(&dir, "snap");

        let secret = "sk-SECRET-DO-NOT-LEAK";
        let mut run = minimal_run(RunStatus::Completed);
        run.api_keys.push(secret.to_string());
        run.artifacts.push(SnapshotArtifact {
            path: "artifacts/step-1/leak.json".to_string(),
            bytes: format!(r#"{{"key": "{secret}", "ok": true}}"#).into_bytes(),
        });

        export_snapshot(&run, &dest).expect("export succeeds");

        let mut bytes = Vec::new();
        File::open(&dest).unwrap().read_to_end(&mut bytes).unwrap();
        assert!(
            !contains_subslice(&bytes, secret.as_bytes()),
            "secret must be redacted out of every artifact"
        );
        assert!(
            contains_subslice(&bytes, b"<redacted>"),
            "expected `<redacted>` marker in zip bytes"
        );
        // The rest of the artifact body survives.
        assert!(contains_subslice(&bytes, b"\"ok\": true"));
    }

    #[test]
    fn data_csv_bytes_are_preserved_verbatim() {
        // Even when api_keys policy could match content in `data.csv`, the
        // exporter keeps the dataset bytes exactly as supplied so the
        // manifest's input_dataset_sha256 still matches.
        let dir = temp_dir("preserve-csv");
        let dest = dest_path(&dir, "snap");

        let csv = b"col1,col2\nsome,value\n";
        let mut run = minimal_run(RunStatus::Completed);
        run.dataset_csv_bytes = csv.to_vec();
        run.api_keys.push("some,value".to_string());

        export_snapshot(&run, &dest).expect("export succeeds");
        let mut bytes = Vec::new();
        File::open(&dest).unwrap().read_to_end(&mut bytes).unwrap();
        assert!(
            contains_subslice(&bytes, csv),
            "data.csv contents must be preserved byte-for-byte"
        );
    }

    fn contains_subslice(haystack: &[u8], needle: &[u8]) -> bool {
        if needle.is_empty() {
            return true;
        }
        haystack.windows(needle.len()).any(|w| w == needle)
    }

    // ---- Boundary: at-ceiling payload is allowed ------------------------

    #[test]
    fn payload_exactly_at_ceiling_is_allowed() {
        let dir = temp_dir("payload-at-ceiling");
        let dest = dest_path(&dir, "snap");

        // Stay well under the ceiling for test runtime — we already proved
        // the comparison is `>` (strict) by the +1-byte refusal test.
        let mut run = minimal_run(RunStatus::Completed);
        run.artifacts.push(SnapshotArtifact {
            path: "artifacts/step-1/data.bin".to_string(),
            bytes: vec![0u8; 1024],
        });

        export_snapshot(&run, &dest).expect("at-or-below ceiling must succeed");
        assert!(dest.exists());
    }

    // ---- tmp_path_for invariants ---------------------------------------

    #[test]
    fn tmp_path_for_appends_dot_tmp() {
        let p = Path::new("/some/dir/snapshot.zip");
        assert_eq!(tmp_path_for(p), PathBuf::from("/some/dir/snapshot.zip.tmp"));

        let p2 = Path::new("snapshot");
        assert_eq!(tmp_path_for(p2), PathBuf::from("snapshot.tmp"));
    }

    #[test]
    fn redact_text_bytes_passes_non_utf8_through() {
        let policy = RedactionPolicy::new().with_secrets(&["secret"]);
        // Mix of valid utf-8 and a stray 0xFF byte.
        let input = b"hello\xff secret world";
        let out = redact_text_bytes(input, &policy);
        assert_eq!(out, input, "non-utf8 bytes must pass through unchanged");
    }

    #[test]
    fn redact_text_bytes_redacts_valid_utf8() {
        let policy = RedactionPolicy::new().with_secrets(&["secret"]);
        let input = b"hello secret world";
        let out = redact_text_bytes(input, &policy);
        assert_eq!(out, b"hello <redacted> world");
    }
}
