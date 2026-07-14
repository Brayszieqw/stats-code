// snapshot/exporter.ts — Audit_Snapshot export orchestration (task 13.4).
// Transcribed from crates/stats-code/src/snapshot/mod.rs `export_snapshot`.
//
// The exporter pulls every component into a single deterministic .zip:
//   * manifest.json        (manifest.ts)
//   * versions.json        (versions.ts)
//   * llm_provenance.json  (llm_provenance.ts)
//   * narrative.md         (narrative.ts)
//   * workflow.yaml        (workflow_yaml.ts canonical pretty-print)
//   * coverage.json        (coverage matrix, loaded verbatim)
//   * data.csv             (original dataset, preserved verbatim)
//   * artifacts/<step>/... (per-step artifacts)
//
// Algorithm:
//   1. Wrap the body in `guardedSpawn` so any accidental spawn of an external
//      statistical runtime aborts (Requirement 8 / spawn policy).
//   2. Refuse non-completed runs (no .tmp created).
//   3. Measure the artifact payload (sum of artifact bytes, excluding data.csv).
//      Refuse > 50 MB before any .tmp is created (Requirement 6.5).
//   4. Build every JSON/markdown/YAML payload in memory.
//   5. Redact every text artifact (excludes hostname/OS-user/home paths and
//      secrets — Requirement 6.6). data.csv is preserved verbatim so its hash
//      matches manifest.input_dataset_sha256.
//   6. Hand the entry list to the deterministic ZIP writer (Stored, fixed
//      mtime, lexicographic order) writing <dest>.tmp + fsync.
//   7. Atomically rename <dest>.tmp → <dest> (no partial on failure).
//   8. Re-read the final file and compute its SHA256.
//
// _Requirements: 6.1, 6.2, 6.3, 6.4, 6.5, 6.6, 6.7_

import { createHash } from 'node:crypto';
import { readFileSync, renameSync, rmSync } from 'node:fs';

import { guardedSpawn } from '../spawn_policy.js';
import { getLoadedMatrix } from '../coverage/index.js';
import { redactPure, redactionPolicy, type RedactionPolicy } from '../redact.js';
import {
  writeDeterministicZip,
  buildZipBytes,
  ZipWriteError,
  type ZipEntry,
} from './zip_writer.js';
import { buildManifest } from './manifest.js';
import { buildVersions, type ReferenceSoftwareVersion } from './versions.js';
import { buildLlmProvenance, type LlmCall } from './llm_provenance.js';
import { buildNarrative, type NarrativeStep } from './narrative.js';
import { prettyPrint, type Workflow } from './workflow_yaml.js';

/** Hard ceiling on the artifact payload (excluding data.csv): 50 MB. */
export const ARTIFACT_PAYLOAD_CEILING_BYTES = 50 * 1024 * 1024;

export type RunStatus = 'completed' | 'running' | 'failed';

/** One per-step artifact destined for the snapshot's artifacts/<step>/ tree. */
export interface SnapshotArtifact {
  /** Archive path inside the snapshot, forward-slash separators only. */
  path: string;
  /** File payload bytes (redacted if UTF-8-decodable). */
  bytes: Uint8Array;
}

/** Materialized, exporter-ready view of an analysis run. */
export interface RunSnapshot {
  runId: string;
  status: RunStatus;
  /** 32-byte SHA256 of the original input dataset. */
  datasetSha256: Uint8Array;
  /** Original input dataset bytes; written verbatim as data.csv (NOT redacted). */
  datasetCsvBytes: Uint8Array;
  workflow: Workflow;
  artifacts: SnapshotArtifact[];
  llmCalls: LlmCall[];
  referenceSoftware: ReferenceSoftwareVersion[];
  /** "Windows" | "Linux" | "macOS". */
  osFamily: string;
  osVersion: string;
  releaseVersion: string;
  commitSha: string;
  /** ISO-8601 UTC creation timestamp; supplied by the caller (purity). */
  createdAtUtc: string;
  /** Direct runtime dependency name → version. */
  runtimeDependencies: Record<string, string>;
  /** Active LLM API keys to redact across every text artifact. */
  apiKeys: string[];
  /** Analysis working directory; paths inside → relative, outside → <external>. */
  workingDirectory?: string;
  narrativeSteps: NarrativeStep[];
}

export interface SnapshotResult {
  snapshotPath: string;
  /** 64-char lowercase hex SHA256 of the final snapshot bytes. */
  sha256: string;
}

export class SnapshotError extends Error {
  constructor(
    public readonly kind:
      | 'run_not_completed'
      | 'payload_too_large'
      | 'narrative'
      | 'workflow'
      | 'zip'
      | 'io'
      | 'forbidden_spawn',
    message: string,
    public readonly detail?: {
      actual?: RunStatus;
      measuredBytes?: number;
      ceilingBytes?: number;
    },
  ) {
    super(message);
    this.name = 'SnapshotError';
  }
}

/** Lowercase-hex SHA256 of the given bytes. */
export function sha256Hex(bytes: Uint8Array): string {
  return createHash('sha256').update(bytes).digest('hex');
}

/** Append ".tmp" to the destination filename (same directory → atomic rename). */
function tmpPathFor(destination: string): string {
  return `${destination}.tmp`;
}

/**
 * If `bytes` is valid UTF-8, redact it; otherwise pass through verbatim so
 * genuinely binary payloads (plots, PDFs) are untouched.
 */
function redactTextBytes(bytes: Uint8Array, policy: RedactionPolicy): Uint8Array {
  // TextDecoder with fatal:true throws on invalid UTF-8.
  let text: string;
  try {
    text = new TextDecoder('utf-8', { fatal: true }).decode(bytes);
  } catch {
    return bytes;
  }
  return new TextEncoder().encode(redactPure(text, policy));
}

function workflowWithFinalDigests(
  workflow: Workflow,
  memberBytes: ReadonlyMap<string, Uint8Array>,
): Workflow {
  const digestRef = (ref: { path: string; sha256: string }): { path: string; sha256: string } => {
    const bytes = memberBytes.get(ref.path);
    if (!bytes) {
      throw new SnapshotError('workflow', `snapshot workflow references missing member: ${ref.path}`);
    }
    return { ...ref, sha256: sha256Hex(bytes) };
  };
  return {
    ...workflow,
    inputDataset: digestRef(workflow.inputDataset),
    steps: workflow.steps.map((step) => ({
      ...step,
      inputs: step.inputs.map(digestRef),
      outputs: step.outputs.map(digestRef),
    })),
  };
}

/**
 * Export an Audit_Snapshot for the given completed run to `destination`.
 * Pure modulo the final tmp + rename + sha256 read-back filesystem step.
 */
export function exportSnapshot(run: RunSnapshot, destination: string): SnapshotResult {
  return guardedSpawn(() => exportSnapshotInner(run, destination));
}

function exportSnapshotInner(run: RunSnapshot, destination: string): SnapshotResult {
  // ---- Gate 1: run.status == completed (no .tmp on refusal) -------------
  if (run.status !== 'completed') {
    throw new SnapshotError(
      'run_not_completed',
      `snapshot refused: run status is ${run.status}, expected completed`,
      { actual: run.status },
    );
  }

  // ---- Gate 2: artifact payload <= 50 MB (data.csv excluded) ------------
  const measured = run.artifacts.reduce((sum, a) => sum + a.bytes.length, 0);
  if (measured > ARTIFACT_PAYLOAD_CEILING_BYTES) {
    throw new SnapshotError(
      'payload_too_large',
      `snapshot refused: artifact payload ${measured} bytes exceeds ceiling ${ARTIFACT_PAYLOAD_CEILING_BYTES} bytes`,
      { measuredBytes: measured, ceilingBytes: ARTIFACT_PAYLOAD_CEILING_BYTES },
    );
  }

  // ---- Build redaction policy -------------------------------------------
  const policy = redactionPolicy({
    secrets: run.apiKeys,
    workingDirectory: run.workingDirectory,
  });

  // ---- Build narrative artifacts index (every snapshot member) ----------
  const artifactsIndex = new Set<string>([
    'data.csv',
    'manifest.json',
    'workflow.yaml',
    'versions.json',
    'llm_provenance.json',
    'narrative.md',
    'coverage.json',
  ]);
  for (const art of run.artifacts) {
    artifactsIndex.add(art.path);
  }

  // ---- Build every component payload ------------------------------------
  const versions = buildVersions(
    run.osFamily,
    run.osVersion,
    run.referenceSoftware,
    run.runtimeDependencies,
  );
  const llm = buildLlmProvenance(run.llmCalls);

  let narrativeText: string;
  try {
    narrativeText = buildNarrative(run.narrativeSteps, artifactsIndex);
  } catch (err) {
    throw new SnapshotError('narrative', `snapshot refused: ${(err as Error).message}`);
  }

  // Redaction can change artifact bytes (for example an absolute path becomes
  // <external>). Workflow digests must be derived from the exact bytes stored.
  const redactedArtifacts = run.artifacts.map((artifact) => ({
    name: artifact.path,
    bytes: redactTextBytes(artifact.bytes, policy),
  }));
  const finalMemberBytes = new Map<string, Uint8Array>([['data.csv', run.datasetCsvBytes]]);
  for (const artifact of redactedArtifacts) finalMemberBytes.set(artifact.name, artifact.bytes);
  const workflowYamlText = prettyPrint(
    workflowWithFinalDigests(run.workflow, finalMemberBytes),
    undefined,
  );
  const coverage = getLoadedMatrix();

  // Compact JSON (no whitespace) mirrors serde_json::to_vec byte output and is
  // byte-stable for identical inputs because field/key order is fixed.
  const enc = new TextEncoder();
  const versionsBytes = enc.encode(JSON.stringify(versions));
  const llmBytes = enc.encode(JSON.stringify(llm));
  const coverageBytes = enc.encode(JSON.stringify(coverage));

  // ---- Apply redaction to every text artifact ---------------------------
  // data.csv stays verbatim so its hash matches manifest.input_dataset_sha256.
  const entriesWithoutManifest: ZipEntry[] = [
    { name: 'data.csv', bytes: run.datasetCsvBytes },
    { name: 'workflow.yaml', bytes: redactTextBytes(enc.encode(workflowYamlText), policy) },
    { name: 'versions.json', bytes: redactTextBytes(versionsBytes, policy) },
    { name: 'llm_provenance.json', bytes: redactTextBytes(llmBytes, policy) },
    { name: 'narrative.md', bytes: redactTextBytes(enc.encode(narrativeText), policy) },
    { name: 'coverage.json', bytes: redactTextBytes(coverageBytes, policy) },
  ];
  entriesWithoutManifest.push(...redactedArtifacts);
  const manifest = buildManifest(
    run.runId,
    run.datasetSha256,
    run.releaseVersion,
    run.commitSha,
    run.createdAtUtc,
    entriesWithoutManifest.map((entry) => ({ path: entry.name, sha256: sha256Hex(entry.bytes) })),
  );
  const entries: ZipEntry[] = [
    ...entriesWithoutManifest,
    {
      name: 'manifest.json',
      bytes: redactTextBytes(enc.encode(JSON.stringify(manifest)), policy),
    },
  ];

  // ---- Write to <dest>.tmp via the deterministic zip writer -------------
  const destTmp = tmpPathFor(destination);
  try {
    writeDeterministicZip(entries, destTmp);
  } catch (err) {
    if (err instanceof ZipWriteError) {
      throw new SnapshotError('zip', `snapshot zip write failed: ${err.message}`);
    }
    throw err;
  }

  // ---- Atomic rename (.tmp → final) -------------------------------------
  try {
    renameSync(destTmp, destination);
  } catch (err) {
    try {
      rmSync(destTmp, { force: true });
    } catch {
      /* best-effort cleanup */
    }
    throw new SnapshotError('io', `snapshot io error at ${destination}: ${(err as Error).message}`);
  }

  // ---- Re-read the final file and compute SHA256 ------------------------
  let sha: string;
  try {
    sha = sha256Hex(readFileSync(destination));
  } catch (err) {
    try {
      rmSync(destination, { force: true });
    } catch {
      /* best-effort cleanup */
    }
    throw new SnapshotError('io', `snapshot io error at ${destination}: ${(err as Error).message}`);
  }

  return { snapshotPath: destination, sha256: sha };
}

/** Build the deterministic snapshot zip bytes in-memory (no fs touch). */
export function buildSnapshotBytes(entries: readonly ZipEntry[]): Uint8Array {
  return buildZipBytes(entries);
}
