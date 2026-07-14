// snapshot/replay.ts — read-only Audit_Snapshot integrity replay.

import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve, sep } from 'node:path';

import { MANIFEST_SCHEMA_VERSION, type SnapshotMemberDigest } from './manifest.js';
import { VERSIONS_SCHEMA_VERSION } from './versions.js';
import { parse as parseWorkflow, type Workflow } from './workflow_yaml.js';
import { validateEntryName } from './zip_writer.js';

export interface ReplayPlan {
  /** Directory containing the safely extracted Audit_Snapshot file set. */
  extractedDir: string;
  /** Reference software installed on the host, as (name, version) tuples. */
  installedReferenceSoftware: ReadonlyArray<readonly [string, string]>;
  /** Optional external trust anchor for manifest.json. */
  expectedManifestSha256?: string;
  /** Backward-compatible test probe; production replay itself never performs these actions. */
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
      | 'manifest_sha256_mismatch'
      | 'member_set_mismatch'
      | 'member_sha256_mismatch'
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

interface ManifestShape {
  schema_version: number;
  input_dataset_sha256: string;
  members: SnapshotMemberDigest[];
}

interface VersionsShape {
  schema_version: number;
  reference_software: { name: string; version: string }[];
}

const SHA256_HEX = /^[0-9a-f]{64}$/;

function sha256HexLower(bytes: Uint8Array): string {
  return createHash('sha256').update(bytes).digest('hex');
}

function invalid(message: string, path?: string): never {
  throw new ReplayError('invalid_snapshot', message, path ? { path } : undefined);
}

function checkedArchivePath(archivePath: string): string {
  try {
    validateEntryName(archivePath);
    return archivePath;
  } catch (error) {
    return invalid(`invalid snapshot member path ${JSON.stringify(archivePath)}: ${(error as Error).message}`, archivePath);
  }
}

function readMember(dir: string, archivePath: string): Uint8Array {
  const safePath = checkedArchivePath(archivePath);
  const root = resolve(dir);
  const full = resolve(root, ...safePath.split('/'));
  if (full !== root && !full.startsWith(`${root}${sep}`)) {
    return invalid(`snapshot member escapes extraction root: ${archivePath}`, archivePath);
  }
  try {
    return readFileSync(full);
  } catch (error) {
    throw new ReplayError('snapshot_io', `snapshot file io error at ${full}: ${(error as Error).message}`, {
      path: archivePath,
    });
  }
}

function listMembers(dir: string): string[] {
  const root = resolve(dir);
  const members: string[] = [];
  const walk = (diskDir: string, prefix: string): void => {
    let entries;
    try {
      entries = readdirSync(diskDir, { withFileTypes: true });
    } catch (error) {
      throw new ReplayError('snapshot_io', `cannot enumerate snapshot directory ${diskDir}: ${(error as Error).message}`);
    }
    for (const entry of entries) {
      const archivePath = prefix ? `${prefix}/${entry.name}` : entry.name;
      if (entry.isSymbolicLink()) invalid(`snapshot must not contain symbolic links: ${archivePath}`, archivePath);
      if (entry.isDirectory()) {
        walk(resolve(diskDir, entry.name), archivePath);
      } else if (entry.isFile()) {
        members.push(checkedArchivePath(archivePath));
      } else {
        invalid(`snapshot contains unsupported filesystem entry: ${archivePath}`, archivePath);
      }
    }
  };
  walk(root, '');
  return members.sort();
}

function parseManifest(bytes: Uint8Array): ManifestShape {
  let value: unknown;
  try {
    value = JSON.parse(new TextDecoder().decode(bytes));
  } catch (error) {
    return invalid(`manifest.json: ${(error as Error).message}`, 'manifest.json');
  }
  if (!value || typeof value !== 'object' || Array.isArray(value)) invalid('manifest.json must be an object', 'manifest.json');
  const manifest = value as Record<string, unknown>;
  if (manifest.schema_version !== MANIFEST_SCHEMA_VERSION) {
    invalid(`manifest.json schema_version must be ${MANIFEST_SCHEMA_VERSION}`, 'manifest.json');
  }
  if (typeof manifest.input_dataset_sha256 !== 'string' || !SHA256_HEX.test(manifest.input_dataset_sha256)) {
    invalid('manifest.json input_dataset_sha256 is invalid', 'manifest.json');
  }
  if (!Array.isArray(manifest.members)) invalid('manifest.json members must be an array', 'manifest.json');
  const seen = new Set<string>();
  const members = manifest.members.map((item, index): SnapshotMemberDigest => {
    if (!item || typeof item !== 'object' || Array.isArray(item)) {
      return invalid(`manifest.json members[${index}] must be an object`, 'manifest.json');
    }
    const member = item as Record<string, unknown>;
    if (typeof member.path !== 'string') invalid(`manifest.json members[${index}].path is invalid`, 'manifest.json');
    const path = checkedArchivePath(member.path as string);
    if (path === 'manifest.json') invalid('manifest.json must not digest itself', 'manifest.json');
    if (seen.has(path)) invalid(`manifest.json contains duplicate member ${path}`, 'manifest.json');
    seen.add(path);
    if (typeof member.sha256 !== 'string' || !SHA256_HEX.test(member.sha256)) {
      invalid(`manifest.json members[${index}].sha256 is invalid`, 'manifest.json');
    }
    return { path, sha256: member.sha256 as string };
  });
  return {
    schema_version: MANIFEST_SCHEMA_VERSION,
    input_dataset_sha256: manifest.input_dataset_sha256 as string,
    members,
  };
}

function parseVersions(bytes: Uint8Array): VersionsShape {
  let value: unknown;
  try {
    value = JSON.parse(new TextDecoder().decode(bytes));
  } catch (error) {
    return invalid(`versions.json: ${(error as Error).message}`, 'versions.json');
  }
  if (!value || typeof value !== 'object' || Array.isArray(value)) invalid('versions.json must be an object', 'versions.json');
  const versions = value as Record<string, unknown>;
  if (versions.schema_version !== VERSIONS_SCHEMA_VERSION) {
    invalid(`versions.json schema_version must be ${VERSIONS_SCHEMA_VERSION}`, 'versions.json');
  }
  if (!Array.isArray(versions.reference_software)) invalid('versions.json reference_software must be an array', 'versions.json');
  const refs = versions.reference_software.map((item, index) => {
    if (!item || typeof item !== 'object' || Array.isArray(item)) {
      return invalid(`versions.json reference_software[${index}] is invalid`, 'versions.json');
    }
    const ref = item as Record<string, unknown>;
    if (typeof ref.name !== 'string' || typeof ref.version !== 'string') {
      return invalid(`versions.json reference_software[${index}] is invalid`, 'versions.json');
    }
    return { name: ref.name, version: ref.version };
  });
  return { schema_version: VERSIONS_SCHEMA_VERSION, reference_software: refs };
}

function verifyArtifactSha256(
  dir: string,
  archivePath: string,
  expectedHex: string,
  kind: 'input' | 'output',
): void {
  const actualHex = sha256HexLower(readMember(dir, archivePath));
  if (actualHex === expectedHex) return;
  const errorKind = kind === 'input' ? 'input_artifact_sha256_mismatch' : 'output_artifact_sha256_mismatch';
  throw new ReplayError(errorKind, `${kind} artifact sha256 mismatch at ${archivePath}`, {
    path: archivePath,
    expected: expectedHex,
    actual: actualHex,
  });
}

function parseWorkflowMember(dir: string): Workflow {
  try {
    return parseWorkflow(new TextDecoder().decode(readMember(dir, 'workflow.yaml'))).workflow;
  } catch (error) {
    if (error instanceof ReplayError) throw error;
    return invalid(`workflow.yaml: ${(error as Error).message}`, 'workflow.yaml');
  }
}

/** Verify a safely extracted snapshot without executing statistics or performing external side effects. */
export function executeReplay(plan: ReplayPlan): ReplayOutcome {
  const se = plan.sideEffects ?? {};
  if (se.portBound || se.browserOpened || se.lockCreated) {
    throw new ReplayError('forbidden_side_effect', 'replay performed a forbidden side effect');
  }

  const manifestBytes = readMember(plan.extractedDir, 'manifest.json');
  const manifest = parseManifest(manifestBytes);
  if (plan.expectedManifestSha256 !== undefined) {
    if (!SHA256_HEX.test(plan.expectedManifestSha256)) invalid('expected manifest sha256 is invalid');
    const actual = sha256HexLower(manifestBytes);
    if (actual !== plan.expectedManifestSha256) {
      throw new ReplayError('manifest_sha256_mismatch', 'manifest sha256 does not match external anchor', {
        path: 'manifest.json', expected: plan.expectedManifestSha256, actual,
      });
    }
  }

  const actualMembers = listMembers(plan.extractedDir).filter((path) => path !== 'manifest.json');
  const declaredMembers = manifest.members.map((member) => member.path).sort();
  if (JSON.stringify(actualMembers) !== JSON.stringify(declaredMembers)) {
    throw new ReplayError('member_set_mismatch', 'snapshot member set does not match manifest');
  }

  const workflow = parseWorkflowMember(plan.extractedDir);
  const inputPaths = new Set(workflow.steps.flatMap((step) => step.inputs.map((ref) => ref.path)));
  const outputPaths = new Set(workflow.steps.flatMap((step) => step.outputs.map((ref) => ref.path)));
  for (const member of manifest.members) {
    const actual = sha256HexLower(readMember(plan.extractedDir, member.path));
    if (actual === member.sha256) continue;
    const kind = member.path === 'data.csv'
      ? 'dataset_sha256_mismatch'
      : inputPaths.has(member.path)
        ? 'input_artifact_sha256_mismatch'
        : outputPaths.has(member.path)
          ? 'output_artifact_sha256_mismatch'
          : 'member_sha256_mismatch';
    throw new ReplayError(kind, `snapshot member sha256 mismatch at ${member.path}`, {
      path: member.path, expected: member.sha256, actual,
    });
  }

  const dataActual = sha256HexLower(readMember(plan.extractedDir, 'data.csv'));
  if (dataActual !== manifest.input_dataset_sha256) {
    throw new ReplayError('dataset_sha256_mismatch', 'data.csv does not match input_dataset_sha256', {
      path: 'data.csv', expected: manifest.input_dataset_sha256, actual: dataActual,
    });
  }

  const versions = parseVersions(readMember(plan.extractedDir, 'versions.json'));
  const missing = versions.reference_software
    .filter((required) => !plan.installedReferenceSoftware.some(
      ([name, version]) => name === required.name && version === required.version,
    ))
    .map((required) => `${required.name} ${required.version}`);
  if (missing.length > 0) {
    throw new ReplayError('reference_software_unavailable', `reference software unavailable: ${JSON.stringify(missing)}`, { missing });
  }

  for (const step of workflow.steps) {
    for (const input of step.inputs) verifyArtifactSha256(plan.extractedDir, input.path, input.sha256, 'input');
    for (const output of step.outputs) verifyArtifactSha256(plan.extractedDir, output.path, output.sha256, 'output');
  }

  return { stepsReplayed: workflow.steps.length };
}
