// snapshot/ — deterministic Audit_Snapshot writer + Replay (Phase 6, tasks 13.4, 13.5).

export * as workflowYaml from './workflow_yaml.js';

export {
  type ZipEntry,
  ZipWriteError,
  crc32,
  encodeArchive,
  buildZipBytes,
  writeDeterministicZip,
  writeSnapshotAtomic,
  validateEntryName,
} from './zip_writer.js';

export {
  MANIFEST_SCHEMA_VERSION,
  RUN_STATUS_COMPLETED,
  type AuditSnapshotManifest,
  type SnapshotMemberDigest,
  encodeSha256HexLower,
  buildManifest,
} from './manifest.js';

export {
  VERSIONS_SCHEMA_VERSION,
  OS_VERSION_MAX_CHARS,
  type ReferenceSoftwareVersion,
  type Versions,
  truncateOsVersion,
  buildVersions,
} from './versions.js';

export {
  LLM_PROVENANCE_SCHEMA_VERSION,
  type LlmCall,
  type LlmProvenance,
  buildLlmProvenance,
} from './llm_provenance.js';

export {
  type KeyMetric,
  type NarrativeStep,
  NarrativeError,
  buildNarrative,
} from './narrative.js';

export {
  ARTIFACT_PAYLOAD_CEILING_BYTES,
  type RunStatus,
  type SnapshotArtifact,
  type RunSnapshot,
  type SnapshotResult,
  SnapshotError,
  sha256Hex,
  exportSnapshot,
  buildSnapshotBytes,
} from './exporter.js';

export {
  type ReplayPlan,
  type ReplayOutcome,
  ReplayError,
  executeReplay,
} from './replay.js';

export {
  SnapshotArchiveError,
  extractSnapshotZipBytes,
  replaySnapshotArchive,
  type ReplaySnapshotArchiveOptions,
  type ReplaySnapshotArchiveResult,
} from './zip_reader.js';
