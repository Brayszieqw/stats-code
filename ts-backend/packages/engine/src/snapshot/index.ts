// snapshot/ — deterministic Audit_Snapshot writer + Replay (Phase 6, tasks 13.4, 13.5).

export interface SnapshotManifestEntry {
  path: string;
  sha256: string;
}
