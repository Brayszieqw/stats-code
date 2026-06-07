// snapshot/manifest.ts — `manifest.json` builder for the Audit_Snapshot (task 13.4).
// Transcribed 1:1 from crates/stats-code/src/snapshot/manifest.rs.
//
// `buildManifest` is a PURE function: it reads no clock, no environment, no
// random seeds. `createdAtUtc` is supplied by the caller (the snapshot
// exporter) so this module stays trivially reproducible. Serialization through
// JSON.stringify (compact, no spaces) reproduces serde_json::to_vec byte-for-
// byte because the fields are declared in serialization order.
//
// _Requirements: 6.1, 6.7_

/** Schema version of the `manifest.json` payload. */
export const MANIFEST_SCHEMA_VERSION = 1;

/** `run_status` value emitted by the exporter; only completed runs export. */
export const RUN_STATUS_COMPLETED = 'completed';

/**
 * Top-level shape of `manifest.json` inside an Audit_Snapshot. Field order
 * matches the Rust `AuditSnapshotManifest` so JSON.stringify is byte-stable
 * and cross-implementation identical.
 */
export interface AuditSnapshotManifest {
  schema_version: number;
  input_dataset_sha256: string;
  created_at_utc: string;
  stats_code_release_version: string;
  stats_code_commit_sha: string;
  run_id: string;
  run_status: string;
}

/** Encode a 32-byte SHA256 digest as 64-char lowercase hex. */
export function encodeSha256HexLower(bytes: Uint8Array): string {
  let out = '';
  for (let i = 0; i < bytes.length; i += 1) {
    out += bytes[i]!.toString(16).padStart(2, '0');
  }
  return out;
}

/**
 * Build the `manifest.json` payload. Pure: all dynamic inputs flow in via
 * arguments. `datasetSha256` is the raw 32-byte digest of the input dataset.
 */
export function buildManifest(
  runId: string,
  datasetSha256: Uint8Array,
  releaseVersion: string,
  commitSha: string,
  createdAtUtc: string,
): AuditSnapshotManifest {
  return {
    schema_version: MANIFEST_SCHEMA_VERSION,
    input_dataset_sha256: encodeSha256HexLower(datasetSha256),
    created_at_utc: createdAtUtc,
    stats_code_release_version: releaseVersion,
    stats_code_commit_sha: commitSha,
    run_id: runId,
    run_status: RUN_STATUS_COMPLETED,
  };
}
