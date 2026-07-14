// tests/integration/replay.test.ts — Audit_Snapshot Replay (task 13.5).
//
// Round-trips a real snapshot: export → extract → replay. Verifies the gate
// ladder (dataset hash, reference software, input/output artifact digests) and
// the side-effect prohibition (no port/browser/lock).
//
// _Requirements: 7.1, 7.2, 7.3, 7.4, 7.5, 7.6_

import { describe, it, expect, afterEach, vi } from 'vitest';
import { existsSync, mkdtempSync, readdirSync, rmSync, writeFileSync, readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { ChildProcess, execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { Server } from 'node:net';
import { snapshot } from '@stats-code/engine';

const {
  exportSnapshot,
  executeReplay,
  ReplayError,
  sha256Hex,
  encodeArchive,
  extractSnapshotZipBytes,
  replaySnapshotArchive,
} = snapshot;
type RunSnapshot = Parameters<typeof exportSnapshot>[0];

const tmpDirs: string[] = [];
afterEach(() => {
  for (const d of tmpDirs.splice(0)) rmSync(d, { recursive: true, force: true });
});
function freshTmp(): string {
  const d = mkdtempSync(join(tmpdir(), 'sc-replay-'));
  tmpDirs.push(d);
  return d;
}

function directoryFingerprint(root: string): string {
  const records: string[] = [];
  const walk = (dir: string, relative: string) => {
    for (const entry of readdirSync(dir, { withFileTypes: true }).sort((a, b) => a.name.localeCompare(b.name))) {
      const path = join(dir, entry.name);
      const childRelative = relative ? `${relative}/${entry.name}` : entry.name;
      if (entry.isDirectory()) walk(path, childRelative);
      else records.push(`${childRelative}:${createHash('sha256').update(readFileSync(path)).digest('hex')}`);
    }
  };
  walk(root, '');
  return createHash('sha256').update(records.join('\n')).digest('hex');
}

const enc = (s: string) => new TextEncoder().encode(s);
const sha32 = (b: Uint8Array) => createHash('sha256').update(b).digest();
const DATASET = enc('id,age\n1,42\n2,37\n');
const RESULT = enc('{"estimate":1.234}');
const RESULT_SHA = sha256Hex(RESULT);

function runWithStep(): RunSnapshot {
  return {
    runId: 'run-replay',
    status: 'completed',
    datasetSha256: sha32(DATASET),
    datasetCsvBytes: DATASET,
    workflow: {
      schemaVersion: 1,
      inputDataset: { path: 'data.csv', sha256: sha256Hex(DATASET) },
      steps: [
        {
          id: 'step-1',
          algorithm: 'ttest',
          params: {},
          inputs: [{ path: 'data.csv', sha256: sha256Hex(DATASET) }],
          outputs: [{ path: 'artifacts/step-1/result.json', sha256: RESULT_SHA }],
          startedAtUtc: '2024-01-01T00:00:00Z',
          endedAtUtc: '2024-01-01T00:00:01Z',
        },
      ],
    },
    artifacts: [{ path: 'artifacts/step-1/result.json', bytes: RESULT }],
    llmCalls: [],
    referenceSoftware: [],
    osFamily: 'Windows',
    osVersion: '10',
    releaseVersion: '0.5.0',
    commitSha: '0'.repeat(40),
    createdAtUtc: '2024-01-01T00:00:00Z',
    runtimeDependencies: {},
    apiKeys: [],
    narrativeSteps: [],
  };
}

/** Export a snapshot then extract it to a directory; return the dir. */
function exportAndExtract(run: RunSnapshot): string {
  const dir = freshTmp();
  const zip = join(dir, 'snap.zip');
  exportSnapshot(run, zip);
  const ex = join(dir, 'extracted');
  execFileSync('powershell', [
    '-NoProfile',
    '-Command',
    `Expand-Archive -LiteralPath '${zip}' -DestinationPath '${ex}' -Force`,
  ]);
  return ex;
}

describe('executeReplay — success', () => {
  it('replays a valid snapshot and reports the step count', () => {
    const ex = exportAndExtract(runWithStep());
    const outcome = executeReplay({ extractedDir: ex, installedReferenceSoftware: [] });
    expect(outcome.stepsReplayed).toBe(1);
  });

  it('hashes the exact redacted artifact bytes stored in the snapshot', () => {
    const run = runWithStep();
    const raw = enc('{"source":"C:/Users/alice/private.csv"}');
    run.artifacts[0] = { path: 'artifacts/step-1/result.json', bytes: raw };
    run.workflow.steps[0]!.outputs[0]!.sha256 = sha256Hex(raw);
    const ex = exportAndExtract(run);

    expect(readFileSync(join(ex, 'artifacts', 'step-1', 'result.json'), 'utf8')).toContain('<external>');
    expect(executeReplay({ extractedDir: ex, installedReferenceSoftware: [] })).toEqual({
      stepsReplayed: 1,
    });
  });
});

describe('executeReplay — integrity gates', () => {
  it('fails when data.csv is tampered (dataset hash mismatch)', () => {
    const ex = exportAndExtract(runWithStep());
    writeFileSync(join(ex, 'data.csv'), 'tampered\n');
    try {
      executeReplay({ extractedDir: ex, installedReferenceSoftware: [] });
      expect.unreachable();
    } catch (e) {
      expect(e).toBeInstanceOf(ReplayError);
      expect((e as InstanceType<typeof ReplayError>).kind).toBe('dataset_sha256_mismatch');
    }
  });

  it('fails when an output artifact is tampered (output hash mismatch)', () => {
    const ex = exportAndExtract(runWithStep());
    writeFileSync(join(ex, 'artifacts', 'step-1', 'result.json'), '{"estimate":9.999}');
    try {
      executeReplay({ extractedDir: ex, installedReferenceSoftware: [] });
      expect.unreachable();
    } catch (e) {
      expect((e as InstanceType<typeof ReplayError>).kind).toBe('output_artifact_sha256_mismatch');
    }
  });

  it('fails when a required reference software is not installed', () => {
    const run = runWithStep();
    run.referenceSoftware = [{ name: 'R', version: '4.4.1' }];
    const ex = exportAndExtract(run);
    try {
      executeReplay({ extractedDir: ex, installedReferenceSoftware: [] });
      expect.unreachable();
    } catch (e) {
      expect((e as InstanceType<typeof ReplayError>).kind).toBe('reference_software_unavailable');
      expect((e as InstanceType<typeof ReplayError>).detail?.missing).toContain('R 4.4.1');
    }
  });

  it('passes when the required reference software is installed at the same version', () => {
    const run = runWithStep();
    run.referenceSoftware = [{ name: 'R', version: '4.4.1' }];
    const ex = exportAndExtract(run);
    const outcome = executeReplay({
      extractedDir: ex,
      installedReferenceSoftware: [['R', '4.4.1']],
    });
    expect(outcome.stepsReplayed).toBe(1);
  });

  it('detects tampering in metadata members that workflow does not consume', () => {
    const ex = exportAndExtract(runWithStep());
    writeFileSync(join(ex, 'narrative.md'), '# changed\n');
    expect(() => executeReplay({ extractedDir: ex, installedReferenceSoftware: [] })).toThrowError(
      expect.objectContaining({ kind: 'member_sha256_mismatch' }),
    );
  });

  it('detects a valid manifest field edit when an external manifest anchor is supplied', () => {
    const ex = exportAndExtract(runWithStep());
    const path = join(ex, 'manifest.json');
    const original = readFileSync(path);
    const expectedManifestSha256 = sha256Hex(original);
    const manifest = JSON.parse(original.toString('utf8'));
    manifest.run_id = 'run-tampered';
    writeFileSync(path, JSON.stringify(manifest));
    expect(() => executeReplay({
      extractedDir: ex,
      installedReferenceSoftware: [],
      expectedManifestSha256,
    })).toThrowError(expect.objectContaining({ kind: 'manifest_sha256_mismatch' }));
  });

  it('rejects extra members and traversal paths referenced by workflow.yaml', () => {
    const withExtra = exportAndExtract(runWithStep());
    writeFileSync(join(withExtra, 'extra.txt'), 'not declared');
    expect(() => executeReplay({ extractedDir: withExtra, installedReferenceSoftware: [] })).toThrowError(
      expect.objectContaining({ kind: 'member_set_mismatch' }),
    );

    const withTraversal = exportAndExtract(runWithStep());
    const workflowPath = join(withTraversal, 'workflow.yaml');
    const workflow = readFileSync(workflowPath, 'utf8').replace(
      'artifacts/step-1/result.json',
      '../outside.json',
    );
    writeFileSync(workflowPath, workflow);
    expect(() => executeReplay({ extractedDir: withTraversal, installedReferenceSoftware: [] })).toThrowError(
      expect.objectContaining({ kind: 'invalid_snapshot' }),
    );
  });

  it('rejects unsupported manifest and versions schema versions', () => {
    const badManifestDir = exportAndExtract(runWithStep());
    const manifestPath = join(badManifestDir, 'manifest.json');
    const badManifest = JSON.parse(readFileSync(manifestPath, 'utf8'));
    badManifest.schema_version = 999;
    writeFileSync(manifestPath, JSON.stringify(badManifest));
    expect(() => executeReplay({ extractedDir: badManifestDir, installedReferenceSoftware: [] })).toThrowError(
      expect.objectContaining({ kind: 'invalid_snapshot' }),
    );

    const badVersionsDir = exportAndExtract(runWithStep());
    const versionsPath = join(badVersionsDir, 'versions.json');
    const badVersions = JSON.parse(readFileSync(versionsPath, 'utf8'));
    badVersions.schema_version = 999;
    const versionsBytes = Buffer.from(JSON.stringify(badVersions));
    writeFileSync(versionsPath, versionsBytes);
    const rootManifestPath = join(badVersionsDir, 'manifest.json');
    const rootManifest = JSON.parse(readFileSync(rootManifestPath, 'utf8'));
    rootManifest.members.find((member: { path: string }) => member.path === 'versions.json').sha256 = sha256Hex(versionsBytes);
    writeFileSync(rootManifestPath, JSON.stringify(rootManifest));
    expect(() => executeReplay({ extractedDir: badVersionsDir, installedReferenceSoftware: [] })).toThrowError(
      expect.objectContaining({ kind: 'invalid_snapshot' }),
    );
  });
});

describe('executeReplay — side-effect prohibition (Req 7.5, 7.6)', () => {
  it('probes real process/port surfaces and leaves the extracted snapshot byte-identical', () => {
    const ex = exportAndExtract(runWithStep());
    const replayWorkspace = dirname(ex);
    const before = directoryFingerprint(replayWorkspace);
    const listenSpy = vi.spyOn(Server.prototype, 'listen').mockImplementation(() => {
      throw new Error('replay attempted to bind a port');
    });
    const spawnSpy = vi.spyOn(ChildProcess.prototype, 'spawn').mockImplementation(() => {
      throw new Error('replay attempted to spawn a process or browser');
    });
    try {
      expect(executeReplay({ extractedDir: ex, installedReferenceSoftware: [] })).toEqual({ stepsReplayed: 1 });
      expect(listenSpy).not.toHaveBeenCalled();
      expect(spawnSpy).not.toHaveBeenCalled();
      expect(directoryFingerprint(replayWorkspace)).toBe(before);
    } finally {
      listenSpy.mockRestore();
      spawnSpy.mockRestore();
    }
  });
});

describe('executeReplay — io', () => {
  it('fails with snapshot_io when the directory has no manifest', () => {
    const dir = freshTmp();
    try {
      executeReplay({ extractedDir: dir, installedReferenceSoftware: [] });
      expect.unreachable();
    } catch (e) {
      expect((e as InstanceType<typeof ReplayError>).kind).toBe('snapshot_io');
    }
  });

  it('round-trips the manifest dataset digest deterministically', () => {
    const ex = exportAndExtract(runWithStep());
    const manifest = JSON.parse(readFileSync(join(ex, 'manifest.json'), 'utf8'));
    expect(manifest.input_dataset_sha256).toBe(sha256Hex(DATASET));
  });
});

describe('snapshot archive production replay entry', () => {
  it('verifies the exported archive SHA-256, safely extracts, and replays', () => {
    const dir = freshTmp();
    const zip = join(dir, 'snapshot.zip');
    const exported = exportSnapshot(runWithStep(), zip);
    expect(replaySnapshotArchive({ snapshotPath: zip, expectedSha256: exported.sha256 })).toMatchObject({
      outcome: { stepsReplayed: 1 },
      archiveSha256: exported.sha256,
      archiveAnchored: true,
    });
    expect(() => replaySnapshotArchive({ snapshotPath: zip, expectedSha256: '0'.repeat(64) })).toThrowError(
      expect.objectContaining({ kind: 'archive_sha256_mismatch' }),
    );
  });

  it('rejects traversal and duplicate entries before writing extraction output', () => {
    const traversal = Buffer.from(encodeArchive([{ name: 'safeaa.txt', bytes: enc('x') }]));
    const safeName = Buffer.from('safeaa.txt');
    const badName = Buffer.from('../bad.txt');
    for (let offset = 0; offset <= traversal.length - safeName.length; offset += 1) {
      if (traversal.subarray(offset, offset + safeName.length).equals(safeName)) badName.copy(traversal, offset);
    }
    const traversalRoot = freshTmp();
    expect(() => extractSnapshotZipBytes(traversal, join(traversalRoot, 'extract'))).toThrowError(
      expect.objectContaining({ kind: 'invalid_archive' }),
    );
    expect(existsSync(join(traversalRoot, 'bad.txt'))).toBe(false);

    const duplicate = encodeArchive([
      { name: 'same.txt', bytes: enc('a') },
      { name: 'same.txt', bytes: enc('b') },
    ]);
    expect(() => extractSnapshotZipBytes(duplicate, join(freshTmp(), 'extract'))).toThrowError(
      expect.objectContaining({ kind: 'invalid_archive' }),
    );
  });
});
