import { describe, it, expect, afterEach } from 'vitest';
import { mkdtempSync, existsSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { snapshot } from '@stats-code/engine';

const {
  buildZipBytes,
  buildSnapshotBytes,
  exportSnapshot,
  SnapshotError,
  sha256Hex,
  ARTIFACT_PAYLOAD_CEILING_BYTES,
  crc32,
} = snapshot;

type RunSnapshot = Parameters<typeof exportSnapshot>[0];

const tmpDirs: string[] = [];
afterEach(() => {
  for (const d of tmpDirs.splice(0)) rmSync(d, { recursive: true, force: true });
});
function freshTmp(): string {
  const d = mkdtempSync(join(tmpdir(), 'sc-snap-'));
  tmpDirs.push(d);
  return d;
}

const enc = (s: string) => new TextEncoder().encode(s);
const sha32 = (bytes: Uint8Array): Uint8Array => createHash('sha256').update(bytes).digest();

const DATASET = enc('id,age\n1,42\n2,37\n');

function minimalRun(overrides: Partial<RunSnapshot> = {}): RunSnapshot {
  return {
    runId: 'run-test',
    status: 'completed',
    datasetSha256: sha32(DATASET),
    datasetCsvBytes: DATASET,
    workflow: {
      schemaVersion: 1,
      inputDataset: { path: 'data.csv', sha256: '0'.repeat(64) },
      steps: [],
    },
    artifacts: [],
    llmCalls: [],
    referenceSoftware: [],
    osFamily: 'Windows',
    osVersion: '10.0.22631',
    releaseVersion: '0.5.0',
    commitSha: '0'.repeat(40),
    createdAtUtc: '2024-01-01T12:34:56Z',
    runtimeDependencies: { fastify: '5.0.0', zod: '3.23.8' },
    apiKeys: [],
    narrativeSteps: [],
    ...overrides,
  };
}

describe('crc32', () => {
  it('matches the known CRC32 of "123456789" (0xCBF43926)', () => {
    expect(crc32(enc('123456789')) >>> 0).toBe(0xcbf43926);
  });
});

describe('Property 17: snapshot determinism (byte-identical)', () => {
  it('same entry set in any order → byte-identical zip', () => {
    const e1 = [
      { name: 'b.txt', bytes: enc('beta') },
      { name: 'a.txt', bytes: enc('alpha') },
    ];
    const e2 = [
      { name: 'a.txt', bytes: enc('alpha') },
      { name: 'b.txt', bytes: enc('beta') },
    ];
    const z1 = buildZipBytes(e1);
    const z2 = buildZipBytes(e2);
    expect(Buffer.from(z1).equals(Buffer.from(z2))).toBe(true);
  });

  it('byte output is stable across repeated builds', () => {
    const entries = [{ name: 'data.csv', bytes: enc('x,y\n1,2\n') }];
    expect(sha256Hex(buildZipBytes(entries))).toBe(sha256Hex(buildZipBytes(entries)));
  });

  it('buildSnapshotBytes is an alias for the deterministic zip writer', () => {
    const entries = [{ name: 'a.txt', bytes: enc('x') }];
    expect(Buffer.from(buildSnapshotBytes(entries)).equals(Buffer.from(buildZipBytes(entries)))).toBe(
      true,
    );
  });

  it('rejects duplicate archive member names', () => {
    expect(() => buildZipBytes([
      { name: 'same.txt', bytes: enc('a') },
      { name: 'same.txt', bytes: enc('b') },
    ])).toThrow(/duplicate entry name/);
  });
});

describe('exportSnapshot assembles the full Audit_Snapshot', () => {
  it('writes data.csv + the six metadata members, readable by Expand-Archive', () => {
    const dir = freshTmp();
    const dest = join(dir, 'out.zip');
    const result = exportSnapshot(minimalRun(), dest);
    expect(existsSync(dest)).toBe(true);
    expect(result.sha256).toMatch(/^[0-9a-f]{64}$/);

    const extractDir = join(dir, 'extracted');
    execFileSync('powershell', [
      '-NoProfile',
      '-Command',
      `Expand-Archive -LiteralPath '${dest}' -DestinationPath '${extractDir}' -Force`,
    ]);

    // data.csv is preserved verbatim.
    expect(readFileSync(join(extractDir, 'data.csv'))).toEqual(Buffer.from(DATASET));

    // manifest.json v2 carries run metadata plus every non-manifest member digest.
    const manifest = JSON.parse(readFileSync(join(extractDir, 'manifest.json'), 'utf8'));
    expect(manifest.schema_version).toBe(2);
    expect(manifest.run_status).toBe('completed');
    expect(manifest.run_id).toBe('run-test');
    // input_dataset_sha256 must match the verbatim data.csv bytes.
    expect(manifest.input_dataset_sha256).toBe(sha256Hex(DATASET));
    expect(manifest.members.map((member: { path: string }) => member.path)).toEqual([
      'coverage.json',
      'data.csv',
      'llm_provenance.json',
      'narrative.md',
      'versions.json',
      'workflow.yaml',
    ]);

    const versions = JSON.parse(readFileSync(join(extractDir, 'versions.json'), 'utf8'));
    expect(versions.os_family).toBe('Windows');
    // runtime_dependencies key-sorted.
    expect(Object.keys(versions.runtime_dependencies)).toEqual(['fastify', 'zod']);

    const llm = JSON.parse(readFileSync(join(extractDir, 'llm_provenance.json'), 'utf8'));
    expect(llm.calls).toEqual([]);

    expect(readFileSync(join(extractDir, 'narrative.md'), 'utf8')).toBe(
      '# Audit Snapshot Narrative\n',
    );

    const coverage = JSON.parse(readFileSync(join(extractDir, 'coverage.json'), 'utf8'));
    expect(Array.isArray(coverage.algorithms)).toBe(true);

    // workflow.yaml round-trips to canonical form.
    expect(readFileSync(join(extractDir, 'workflow.yaml'), 'utf8')).toContain('schema_version: 1');
  });

  it('is byte-deterministic for identical inputs', () => {
    const dir = freshTmp();
    const a = join(dir, 'a.zip');
    const b = join(dir, 'b.zip');
    const r1 = exportSnapshot(minimalRun(), a);
    const r2 = exportSnapshot(minimalRun(), b);
    expect(r1.sha256).toBe(r2.sha256);
    expect(Buffer.from(readFileSync(a)).equals(Buffer.from(readFileSync(b)))).toBe(true);
  });

  it('redacts secrets and external paths from text members (data.csv excluded)', () => {
    const dir = freshTmp();
    const dest = join(dir, 'redacted.zip');
    const run = minimalRun({
      apiKeys: ['sk-supersecret-key'],
      workingDirectory: 'C:/work/run',
      artifacts: [
        {
          path: 'artifacts/step-1/result.json',
          bytes: enc('{"key":"sk-supersecret-key","p":"C:/elsewhere/data.bin"}'),
        },
      ],
    });
    exportSnapshot(run, dest);
    const extractDir = join(dir, 'ex');
    execFileSync('powershell', [
      '-NoProfile',
      '-Command',
      `Expand-Archive -LiteralPath '${dest}' -DestinationPath '${extractDir}' -Force`,
    ]);
    const artifact = readFileSync(join(extractDir, 'artifacts', 'step-1', 'result.json'), 'utf8');
    expect(artifact).not.toContain('sk-supersecret-key');
    expect(artifact).toContain('<redacted>');
    expect(artifact).toContain('<external>');
  });
});

describe('Property 18: no partial artifact on failure', () => {
  it('payload exceeding 50 MB throws and writes nothing', () => {
    const dir = freshTmp();
    const dest = join(dir, 'too-big.zip');
    const big = new Uint8Array(ARTIFACT_PAYLOAD_CEILING_BYTES + 1);
    const run = minimalRun({ artifacts: [{ path: 'artifacts/big.bin', bytes: big }] });
    expect(() => exportSnapshot(run, dest)).toThrow(SnapshotError);
    expect(existsSync(dest)).toBe(false);
    expect(existsSync(`${dest}.tmp`)).toBe(false);
  });

  it('a non-completed run is refused before any file is created', () => {
    const dir = freshTmp();
    const dest = join(dir, 'running.zip');
    const run = minimalRun({ status: 'running' });
    expect(() => exportSnapshot(run, dest)).toThrow(SnapshotError);
    expect(existsSync(dest)).toBe(false);
    expect(existsSync(`${dest}.tmp`)).toBe(false);
  });

  it('an invalid artifact entry name throws before any file is created', () => {
    const dir = freshTmp();
    const dest = join(dir, 'bad.zip');
    const run = minimalRun({ artifacts: [{ path: '../escape.txt', bytes: enc('x') }] });
    expect(() => exportSnapshot(run, dest)).toThrow();
    expect(existsSync(dest)).toBe(false);
    expect(existsSync(`${dest}.tmp`)).toBe(false);
  });

  it('a narrative citing an unknown artifact is refused', () => {
    const dir = freshTmp();
    const dest = join(dir, 'narr.zip');
    const run = minimalRun({
      narrativeSteps: [
        {
          id: 'step-1',
          algorithm: 'ttest',
          displayName: 'T-test',
          paramsSummary: 'y=age',
          keyMetrics: [
            {
              label: 'p',
              value: '0.04',
              artifactPath: 'artifacts/step-1/MISSING.json',
              jsonPointer: 'p_value',
            },
          ],
        },
      ],
    });
    expect(() => exportSnapshot(run, dest)).toThrow(SnapshotError);
    expect(existsSync(dest)).toBe(false);
    expect(existsSync(`${dest}.tmp`)).toBe(false);
  });
});

describe('manifest integrity', () => {
  it('input_dataset_sha256 matches the verbatim data.csv bytes', () => {
    const dir = freshTmp();
    const dest = join(dir, 'm.zip');
    exportSnapshot(minimalRun(), dest);
    const extractDir = join(dir, 'ex');
    execFileSync('powershell', [
      '-NoProfile',
      '-Command',
      `Expand-Archive -LiteralPath '${dest}' -DestinationPath '${extractDir}' -Force`,
    ]);
    const manifest = JSON.parse(readFileSync(join(extractDir, 'manifest.json'), 'utf8'));
    const csv = readFileSync(join(extractDir, 'data.csv'));
    expect(manifest.input_dataset_sha256).toBe(sha256Hex(csv));
  });
});
