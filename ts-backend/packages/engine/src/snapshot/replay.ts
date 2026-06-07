// snapshot/replay.ts — Audit_Snapshot Replay (task 13.5).
// Transcribed from crates/stats-code/src/snapshot/replay.rs.
//
// Replay recomputes each contained artifact's SHA256 and compares it to the
// manifest, re-checks every workflow step's input/output artifact digests, and
// enforces that the replay performs NO port bind, NO browser open, and NO
// single-instance lock (those are fatal violations). The numeric re-execution
// drift gate is represented by the artifact-integrity gate (wave-1 parity with
// the Rust backend); a future wave re-runs each step through the engine.
//
// Gate ladder (Requirement 7):
//   0. read manifest.json
//   1. data.csv SHA256 == manifest.input_dataset_sha256        (Req 7.1, 7.2)
//   2. every recorded reference software is installed          (Req 7.3 support)
//   3. every step input artifact SHA256 matches                (Req 7.1)
//   4. every step output artifact SHA256 matches               (Req 7.3, 7.4)
//   side-effect guard: no port/browser/lock                    (Req 7.5, 7.6)
//
// _Requirements: 7.1, 7.2, 7.3, 7.4, 7.5, 7.6_

import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';

import { parse as parseWorkflow } from './workflow_yaml.js';

export interface ReplayPlan {
  /** Directory containing the extracted Audit_Snapshot file set. */
  extractedDir: string;
  /** Reference software installed on the host, as (name, version) tuples. */
  installedReferenceSoftware: ReadonlyArray<readonly [string, string]>;
  /**
   * Side-effect probes (Requirement 7.5/7.6). Replay must never bind a port,
   * open a browser, or create a lock; if any of these report that the action
   * was attempted, replay fails fatally. Default: all false (no side effects).
   */
  sideEffects?: {
    portBound?: boolean;
    browserOpened?: boolean;
    lockCreated?: boolean;
  };
}

export interface ReplayOutcome {
  stepsReplayed: number;
}

export class ReplayError extends Error {
  constructor(
    public readonly kind:
      | 'snapshot_io'
      | 'invalid_snapshot'
      | 'dataset_sha256_mismatch'
      | 'reference_software_unavailable'
      | 'input_artifact_sha256_mismatch'
      | 'output_artifact_sha256_mismatch'
      | 'forbidden_side_effect',
    message: string,
    public readonly detail?: {
      path?: string;
      expected?: string;
      actual?: string;
      missing?: string[];
    },
  ) {
    super(message);
    this.name = 'ReplayError';
  }
}

/** Lowercase-hex SHA256 of bytes. */
function sha256HexLower(bytes: Uint8Array): string {
  return createHash('sha256').update(bytes).digest('hex');
}

function readMember(dir: string, archivePath: string): Uint8Array {
  const full = join(dir, archivePath);
  try {
    return readFileSync(full);
  } catch (err) {
    throw new ReplayError('snapshot_io', `snapshot file io error at ${full}: ${(err as Error).message}`, {
      path: archivePath,
    });
  }
}

function verifyArtifactSha256(
  dir: string,
  archivePath: string,
  expectedHex: string,
  kind: 'input' | 'output',
): void {
  const bytes = readMember(dir, archivePath);
  const actualHex = sha256HexLower(bytes);
  if (actualHex !== expectedHex) {
    const errKind = kind === 'input' ? 'input_artifact_sha256_mismatch' : 'output_artifact_sha256_mismatch';
    throw new ReplayError(
      errKind,
      `${kind} artifact sha256 mismatch at ${archivePath}: expected ${expectedHex}, actual ${actualHex}`,
      { path: archivePath, expected: expectedHex, actual: actualHex },
    );
  }
}

interface ManifestShape {
  input_dataset_sha256: string;
}
interface VersionsShape {
  reference_software: { name: string; version: string }[];
}

/**
 * Execute a Replay plan against an extracted Audit_Snapshot directory. Returns
 * the number of steps that passed every gate, or throws a structured
 * ReplayError naming the offending gate. Never modifies the snapshot file set
 * and never performs any network/browser/lock side effect.
 */
export function executeReplay(plan: ReplayPlan): ReplayOutcome {
  const dir = plan.extractedDir;

  // Side-effect guard (Requirement 7.5/7.6): replay must be inert.
  const se = plan.sideEffects ?? {};
  if (se.portBound || se.browserOpened || se.lockCreated) {
    const violations = [
      se.portBound ? 'bound a network port' : null,
      se.browserOpened ? 'opened a browser' : null,
      se.lockCreated ? 'created a single-instance lock' : null,
    ].filter((v): v is string => v !== null);
    throw new ReplayError(
      'forbidden_side_effect',
      `replay performed forbidden side effect(s): ${violations.join(', ')}`,
    );
  }

  // Gate 0: manifest.json
  const manifestBytes = readMember(dir, 'manifest.json');
  let manifest: ManifestShape;
  try {
    manifest = JSON.parse(new TextDecoder().decode(manifestBytes)) as ManifestShape;
  } catch (e) {
    throw new ReplayError('invalid_snapshot', `manifest.json: ${(e as Error).message}`, {
      path: 'manifest.json',
    });
  }

  // Gate 1: data.csv SHA256 == manifest.input_dataset_sha256
  const dataCsv = readMember(dir, 'data.csv');
  const actualDatasetHex = sha256HexLower(dataCsv);
  if (actualDatasetHex !== manifest.input_dataset_sha256) {
    throw new ReplayError(
      'dataset_sha256_mismatch',
      `dataset sha256 mismatch at data.csv: expected ${manifest.input_dataset_sha256}, actual ${actualDatasetHex}`,
      { path: 'data.csv', expected: manifest.input_dataset_sha256, actual: actualDatasetHex },
    );
  }

  // Gate 2: every recorded reference software is installed at the same version
  const versionsBytes = readMember(dir, 'versions.json');
  let versions: VersionsShape;
  try {
    versions = JSON.parse(new TextDecoder().decode(versionsBytes)) as VersionsShape;
  } catch (e) {
    throw new ReplayError('invalid_snapshot', `versions.json: ${(e as Error).message}`, {
      path: 'versions.json',
    });
  }
  const missing: string[] = [];
  for (const required of versions.reference_software ?? []) {
    const installed = plan.installedReferenceSoftware.some(
      ([n, v]) => n === required.name && v === required.version,
    );
    if (!installed) {
      missing.push(`${required.name} ${required.version}`);
    }
  }
  if (missing.length > 0) {
    throw new ReplayError(
      'reference_software_unavailable',
      `reference software unavailable: missing ${JSON.stringify(missing)}`,
      { missing },
    );
  }

  // Gate 3 + 4: workflow input/output artifact digests
  const workflowBytes = readMember(dir, 'workflow.yaml');
  let workflow;
  try {
    workflow = parseWorkflow(new TextDecoder().decode(workflowBytes)).workflow;
  } catch (e) {
    throw new ReplayError('invalid_snapshot', `workflow.yaml: ${(e as Error).message}`, {
      path: 'workflow.yaml',
    });
  }

  for (const step of workflow.steps) {
    for (const input of step.inputs) {
      verifyArtifactSha256(dir, input.path, input.sha256, 'input');
    }
  }
  for (const step of workflow.steps) {
    for (const output of step.outputs) {
      verifyArtifactSha256(dir, output.path, output.sha256, 'output');
    }
  }

  return { stepsReplayed: workflow.steps.length };
}
