//! `manifest.json` builder for the Audit Snapshot.
//!
//! Implements task 6.2 of `parity-and-multilang-sidecar`. See design.md
//! "Snapshot Manifest" / `AuditSnapshotManifest` and Requirements 7.3 / 8.1.
//!
//! `build_manifest` is a **pure function**: it reads no clock, no environment
//! variables, no random seeds. The `created_at_utc` field is supplied by the
//! caller (the snapshot exporter, task 6.7) so this module stays trivially
//! reproducible and PBT-friendly. The exporter is the only place where
//! `chrono::Utc::now()` (or equivalent) is read; that read is then threaded
//! through this builder.
//!
//! _Requirements: 7.3, 7.6, 8.1_

use serde::{Deserialize, Serialize};

/// Schema version of the `manifest.json` payload. Bumped on any breaking
/// change to the field set; new readers must reject unknown values.
pub const SCHEMA_VERSION: u32 = 1;

/// `run_status` value emitted by the exporter. Only completed runs may be
/// exported (Requirement 7.6 / 7.8); the exporter rejects non-completed runs
/// before this builder is ever called, so the manifest never carries a
/// non-`"completed"` status.
pub const RUN_STATUS_COMPLETED: &str = "completed";

/// Top-level shape of `manifest.json` inside an Audit Snapshot.
///
/// Field order matches `design.md` ("Snapshot Manifest") and Requirement 7.3.
/// Serialization uses the field order declared here, which gives byte-stable
/// JSON output for identical inputs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditSnapshotManifest {
    /// Always `1` for this revision. See [`SCHEMA_VERSION`].
    pub schema_version: u32,
    /// 64-character lowercase hex SHA256 of the original input dataset
    /// (matches the bytes stored as `data.csv` inside the snapshot).
    pub input_dataset_sha256: String,
    /// ISO-8601 UTC timestamp of snapshot creation, e.g.
    /// `"2024-01-01T12:34:56Z"`. Supplied by the caller.
    pub created_at_utc: String,
    /// Stats Code release version (semver string from `Cargo.toml`).
    pub stats_code_release_version: String,
    /// Stats Code commit SHA (40-hex git revision of the build).
    pub stats_code_commit_sha: String,
    /// Identifier of the analysis run this snapshot describes.
    pub run_id: String,
    /// Status of the run at snapshot time. Always [`RUN_STATUS_COMPLETED`]
    /// because only completed runs are exportable (Requirement 7.6 / 7.8).
    pub run_status: String,
}

/// Build the `manifest.json` payload for a snapshot.
///
/// Pure function: all dynamic inputs (the wall-clock timestamp, the run id,
/// the dataset hash, the build's release version and commit SHA) flow in via
/// arguments. Two calls with byte-identical inputs produce byte-identical
/// `AuditSnapshotManifest` values, and therefore byte-identical JSON when
/// serialized through `serde_json::to_vec` (Requirement 7.1 determinism
/// contract via the snapshot exporter, task 6.7).
///
/// `created_at_utc` is added to the original task signature so the function
/// can stay pure; the snapshot exporter (task 6.7) supplies the value with
/// `chrono::Utc::now().to_rfc3339()` (or equivalent) at the entry point of
/// the export pipeline. The string is stored verbatim — this builder does
/// not validate its format, on the assumption that the exporter formats it
/// canonically as ISO-8601 UTC.
///
/// _Requirements: 7.3, 7.6, 8.1_
#[must_use]
pub fn build_manifest(
    run_id: &str,
    dataset_sha256: &[u8; 32],
    release_version: &str,
    commit_sha: &str,
    created_at_utc: &str,
) -> AuditSnapshotManifest {
    AuditSnapshotManifest {
        schema_version: SCHEMA_VERSION,
        input_dataset_sha256: encode_sha256_hex_lower(dataset_sha256),
        created_at_utc: created_at_utc.to_owned(),
        stats_code_release_version: release_version.to_owned(),
        stats_code_commit_sha: commit_sha.to_owned(),
        run_id: run_id.to_owned(),
        run_status: RUN_STATUS_COMPLETED.to_owned(),
    }
}

/// Encode a 32-byte SHA256 digest as a 64-character lowercase hexadecimal
/// string. Hand-rolled to avoid a new dependency; the hex output matches
/// the format mandated by Requirement 1.7 / 7.3 / 8.1 ("64-character
/// lowercase hexadecimal").
fn encode_sha256_hex_lower(bytes: &[u8; 32]) -> String {
    let mut out = String::with_capacity(64);
    for b in bytes {
        // `{:02x}` pads to two lowercase hex digits per byte.
        use std::fmt::Write as _;
        let _ = write!(&mut out, "{b:02x}");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_sha256() -> [u8; 32] {
        // Mixed pattern that exercises both halves of every byte, including
        // a leading zero (`0x00`) and a high byte (`0xff`).
        let mut bytes = [0u8; 32];
        for (i, b) in bytes.iter_mut().enumerate() {
            *b = u8::try_from(i * 7 % 256).unwrap_or(0);
        }
        bytes[0] = 0x00;
        bytes[1] = 0x0a;
        bytes[30] = 0xde;
        bytes[31] = 0xff;
        bytes
    }

    #[test]
    fn happy_path_populates_every_field() {
        let dataset = sample_sha256();
        let manifest = build_manifest(
            "run-12345",
            &dataset,
            "0.5.0",
            "0123456789abcdef0123456789abcdef01234567",
            "2024-01-01T12:34:56Z",
        );

        assert_eq!(manifest.schema_version, 1);
        assert_eq!(manifest.input_dataset_sha256.len(), 64);
        assert!(
            manifest
                .input_dataset_sha256
                .chars()
                .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)),
            "sha256 hex must be lowercase hex digits only, got {:?}",
            manifest.input_dataset_sha256
        );
        // First two bytes 0x00, 0x0a → "000a"; last two 0xde, 0xff → "deff".
        assert!(manifest.input_dataset_sha256.starts_with("000a"));
        assert!(manifest.input_dataset_sha256.ends_with("deff"));

        assert_eq!(manifest.created_at_utc, "2024-01-01T12:34:56Z");
        assert_eq!(manifest.stats_code_release_version, "0.5.0");
        assert_eq!(
            manifest.stats_code_commit_sha,
            "0123456789abcdef0123456789abcdef01234567"
        );
        assert_eq!(manifest.run_id, "run-12345");
        assert_eq!(manifest.run_status, RUN_STATUS_COMPLETED);
    }

    #[test]
    fn hex_encodes_all_zero_digest_as_64_zeros() {
        let manifest = build_manifest(
            "run-zero",
            &[0u8; 32],
            "0.0.0",
            "0".repeat(40).as_str(),
            "1970-01-01T00:00:00Z",
        );
        assert_eq!(manifest.input_dataset_sha256, "0".repeat(64));
    }

    #[test]
    fn hex_encodes_all_ff_digest_as_64_f() {
        let manifest = build_manifest(
            "run-ff",
            &[0xffu8; 32],
            "9.9.9",
            "f".repeat(40).as_str(),
            "9999-12-31T23:59:59Z",
        );
        assert_eq!(manifest.input_dataset_sha256, "f".repeat(64));
    }

    #[test]
    fn serializes_to_json_round_trip() {
        let dataset = sample_sha256();
        let manifest = build_manifest(
            "run-rt",
            &dataset,
            "0.5.0",
            "abcdef0123456789abcdef0123456789abcdef01",
            "2024-06-15T08:00:00Z",
        );

        let json = serde_json::to_vec(&manifest).expect("manifest serializes");
        let parsed: AuditSnapshotManifest =
            serde_json::from_slice(&json).expect("manifest round-trips");

        assert_eq!(parsed, manifest);
    }

    #[test]
    fn json_serialization_is_deterministic() {
        let dataset = sample_sha256();
        let m1 = build_manifest(
            "run-det",
            &dataset,
            "0.5.0",
            "abcdef0123456789abcdef0123456789abcdef01",
            "2024-06-15T08:00:00Z",
        );
        let m2 = build_manifest(
            "run-det",
            &dataset,
            "0.5.0",
            "abcdef0123456789abcdef0123456789abcdef01",
            "2024-06-15T08:00:00Z",
        );

        assert_eq!(m1, m2);

        let j1 = serde_json::to_vec(&m1).unwrap();
        let j2 = serde_json::to_vec(&m2).unwrap();
        assert_eq!(j1, j2, "identical inputs must produce byte-identical JSON");
    }

    #[test]
    fn json_field_order_matches_struct_declaration() {
        let manifest = build_manifest(
            "run-order",
            &[0u8; 32],
            "0.5.0",
            "0".repeat(40).as_str(),
            "2024-01-01T00:00:00Z",
        );
        let json = serde_json::to_string(&manifest).unwrap();
        let pos = |needle: &str| {
            json.find(needle)
                .unwrap_or_else(|| panic!("missing field {needle} in {json}"))
        };
        let order = [
            pos("\"schema_version\""),
            pos("\"input_dataset_sha256\""),
            pos("\"created_at_utc\""),
            pos("\"stats_code_release_version\""),
            pos("\"stats_code_commit_sha\""),
            pos("\"run_id\""),
            pos("\"run_status\""),
        ];
        let mut sorted = order;
        sorted.sort_unstable();
        assert_eq!(
            order, sorted,
            "JSON fields must appear in struct declaration order; got {json}"
        );
    }

    #[test]
    fn run_status_is_always_completed() {
        let manifest = build_manifest(
            "any-run",
            &[1u8; 32],
            "0.5.0",
            "0".repeat(40).as_str(),
            "2024-01-01T00:00:00Z",
        );
        assert_eq!(manifest.run_status, "completed");
    }
}
