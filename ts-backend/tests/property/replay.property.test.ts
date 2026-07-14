// tests/property/replay.property.test.ts — Property 22 (task 13.15).
//
// Property 22 (Replay soundness): for ALL faithfully round-tripped snapshots,
// replay passes every integrity gate; and for ALL single-byte tamperings of any
// contained artifact, replay detects the mismatch and fails (Requirements 7.2,
// 7.4). Replay never performs a port bind / browser / lock side effect.
//
// Validates: Requirements 7.2, 7.4

import { describe, it, expect, afterEach } from 'vitest';
import fc from 'fast-check';
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
  const d = mkdtempSync(join(tmpdir(), 'sc-replayprop-'));
  tmpDirs.push(d);
  return d;
}

const enc = (s: string) => new TextEncoder().encode(s);
const sha32 = (b: Uint8Array) => createHash('sha256').update(b).digest();

function buildRun(datasetText: string, resultText: string): RunSnapshot {
  const dataset = enc(datasetText);
  const result = enc(resultText);
  return {
    runId: 'run-prop',
    status: 'completed',
    datasetSha256: sha32(dataset),
    datasetCsvBytes: dataset,
    workflow: {
      schemaVersion: 1,
      inputDataset: { path: 'data.csv', sha256: sha256Hex(dataset) },
      steps: [
        {
          id: 'step-1',
          algorithm: 'ttest',
          params: {},
          inputs: [{ path: 'data.csv', sha256: sha256Hex(dataset) }],
          outputs: [{ path: 'artifacts/step-1/result.json', sha256: sha256Hex(result) }],
          startedAtUtc: '2024-01-01T00:00:00Z',
          endedAtUtc: '2024-01-01T00:00:01Z',
        },
      ],
    },
    artifacts: [{ path: 'artifacts/step-1/result.json', bytes: result }],
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

describe('Property 22: replay soundness (Requirements 7.2, 7.4)', () => {
  it('a faithful round-trip always passes every gate', () => {
    fc.assert(
      fc.property(
        fc.stringMatching(/^[a-z,0-9\n]{1,40}$/),
        fc.stringMatching(/^[a-z0-9":,{} .]{1,40}$/),
        (datasetText, resultText) => {
          const ex = exportAndExtract(buildRun(`col\n${datasetText}`, `{"v":"${resultText}"}`));
          const outcome = executeReplay({ extractedDir: ex, installedReferenceSoftware: [] });
          expect(outcome.stepsReplayed).toBe(1);
        },
      ),
      { numRuns: 12 },
    );
  });

  it('tampering data.csv always fails the dataset gate', () => {
    fc.assert(
      fc.property(fc.stringMatching(/^[a-z0-9]{1,16}$/), (suffix) => {
        const ex = exportAndExtract(buildRun('col\na,b', '{"v":"x"}'));
        const original = readFileSync(join(ex, 'data.csv'));
        // Append bytes so the digest necessarily changes.
        writeFileSync(join(ex, 'data.csv'), Buffer.concat([original, Buffer.from(suffix)]));
        let threw = false;
        try {
          executeReplay({ extractedDir: ex, installedReferenceSoftware: [] });
        } catch (e) {
          threw = e instanceof ReplayError && (e as InstanceType<typeof ReplayError>).kind === 'dataset_sha256_mismatch';
        }
        return threw;
      }),
      { numRuns: 10 },
    );
  });

  it('tampering the output artifact always fails the output gate', () => {
    fc.assert(
      fc.property(fc.stringMatching(/^[a-z0-9]{1,16}$/), (suffix) => {
        const ex = exportAndExtract(buildRun('col\na,b', '{"v":"x"}'));
        const p = join(ex, 'artifacts', 'step-1', 'result.json');
        const original = readFileSync(p);
        writeFileSync(p, Buffer.concat([original, Buffer.from(suffix)]));
        let threw = false;
        try {
          executeReplay({ extractedDir: ex, installedReferenceSoftware: [] });
        } catch (e) {
          threw =
            e instanceof ReplayError &&
            (e as InstanceType<typeof ReplayError>).kind === 'output_artifact_sha256_mismatch';
        }
        return threw;
      }),
      { numRuns: 10 },
    );
  });

  it('tampering any snapshot member is detected when manifest is externally anchored', () => {
    const memberNames = [
      'manifest.json',
      'data.csv',
      'workflow.yaml',
      'versions.json',
      'llm_provenance.json',
      'narrative.md',
      'coverage.json',
      'artifacts/step-1/result.json',
    ] as const;
    for (const memberName of memberNames) {
      const ex = exportAndExtract(buildRun('col\na,b', '{"v":"x"}'));
      const manifestPath = join(ex, 'manifest.json');
      const expectedManifestSha256 = sha256Hex(readFileSync(manifestPath));
      const target = join(ex, ...memberName.split('/'));
      writeFileSync(target, Buffer.concat([readFileSync(target), Buffer.from('\n')]));
      expect(() => executeReplay({
        extractedDir: ex,
        installedReferenceSoftware: [],
        expectedManifestSha256,
      })).toThrow(ReplayError);
    }
  });
});
