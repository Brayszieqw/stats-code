// tests/integration/replay.test.ts — Audit_Snapshot Replay (task 13.5).
//
// Round-trips a real snapshot: export → extract → replay. Verifies the gate
// ladder (dataset hash, reference software, input/output artifact digests) and
// the side-effect prohibition (no port/browser/lock).
//
// _Requirements: 7.1, 7.2, 7.3, 7.4, 7.5, 7.6_

import { describe, it, expect, afterEach } from 'vitest';
import { mkdtempSync, rmSync, writeFileSync, readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { snapshot } from '@stats-code/engine';

const { exportSnapshot, executeReplay, ReplayError, sha256Hex } = snapshot;
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
});

describe('executeReplay — side-effect prohibition (Req 7.5, 7.6)', () => {
  it('fails fatally if a port bind / browser / lock was attempted', () => {
    const ex = exportAndExtract(runWithStep());
    for (const se of [{ portBound: true }, { browserOpened: true }, { lockCreated: true }]) {
      try {
        executeReplay({ extractedDir: ex, installedReferenceSoftware: [], sideEffects: se });
        expect.unreachable();
      } catch (e) {
        expect((e as InstanceType<typeof ReplayError>).kind).toBe('forbidden_side_effect');
      }
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
