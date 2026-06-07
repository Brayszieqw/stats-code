// tests/integration/trust-routes.test.ts — wired trust-credential routes (task 13.7).
//
// Exercises GET /api/coverage-matrix, POST /api/sidecar/:id, and
// POST /api/snapshot/export against the REAL engine-backed providers
// (createCoverageMatrixProvider / createSidecarProvider / createSnapshotProvider),
// confirming the wire DTOs validate and the guarded pipelines produce output.
//
// _Requirements: 1.1, 5.3_

import { describe, it, expect, afterEach } from 'vitest';
import { mkdtempSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createHash } from 'node:crypto';
import {
  buildRouter,
  MemSessionStore,
  contract,
  createCoverageMatrixProvider,
  createSidecarProvider,
  createSnapshotProvider,
  type AppState,
} from '@stats-code/server';
import { snapshot } from '@stats-code/engine';

type RunSnapshot = Parameters<typeof snapshot.exportSnapshot>[0];

const tmpDirs: string[] = [];
afterEach(() => {
  for (const d of tmpDirs.splice(0)) rmSync(d, { recursive: true, force: true });
});
function freshTmp(): string {
  const d = mkdtempSync(join(tmpdir(), 'sc-trust-'));
  tmpDirs.push(d);
  return d;
}

const enc = (s: string) => new TextEncoder().encode(s);
const DATASET = enc('id,age\n1,42\n');

function completedRun(): RunSnapshot {
  return {
    runId: 'run-1',
    status: 'completed',
    datasetSha256: createHash('sha256').update(DATASET).digest(),
    datasetCsvBytes: DATASET,
    workflow: { schemaVersion: 1, inputDataset: { path: 'data.csv', sha256: '0'.repeat(64) }, steps: [] },
    artifacts: [],
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

function wiredState(): AppState {
  const runs = new Map<string, RunSnapshot>([['run-1', completedRun()]]);
  return {
    sessionStore: new MemSessionStore(),
    coverageMatrixProvider: createCoverageMatrixProvider(),
    sidecarProvider: createSidecarProvider(),
    snapshotProvider: createSnapshotProvider((id) => runs.get(id)),
  };
}

describe('GET /api/coverage-matrix (wired)', () => {
  it('returns the engine matrix mapped to the wire DTO and validates the schema', async () => {
    const app = buildRouter({ state: wiredState() });
    const res = await app.inject({ method: 'GET', url: '/api/coverage-matrix' });
    expect(res.statusCode).toBe(200);
    const body = res.json();
    expect(contract.sidecar.coverageMatrix.safeParse(body).success).toBe(true);
    expect(body.algorithms.length).toBeGreaterThan(0);
    // Reference cells use the wire shape (callable/version), not fn/proc.
    const first = body.algorithms[0];
    for (const sw of ['R', 'SAS', 'Python', 'SPSS']) {
      expect(typeof first.reference[sw].callable).toBe('string');
      expect(typeof first.reference[sw].version).toBe('string');
    }
    await app.close();
  });
});

describe('POST /api/sidecar/:id (wired)', () => {
  it('renders a snippet for a covered cell and validates the DTO', async () => {
    const app = buildRouter({ state: wiredState() });
    const res = await app.inject({
      method: 'POST',
      url: '/api/sidecar/tableone',
      payload: {
        software: 'R',
        dataset_sha256: 'a'.repeat(64),
        columns: [
          { name: 'age', dtype: 'numeric' },
          { name: 'sex', dtype: 'categorical' },
        ],
        params: {},
      },
    });
    expect(res.statusCode).toBe(200);
    const body = res.json();
    expect(contract.sidecar.sidecarSnippet.safeParse(body).success).toBe(true);
    expect(body.algorithm_id).toBe('tableone');
    expect(body.coverage_value).not.toBe('none');
    expect(typeof body.text).toBe('string');
    await app.close();
  });

  it('returns a copy-disabled placeholder (no text) for a none cell', async () => {
    const app = buildRouter({ state: wiredState() });
    const res = await app.inject({
      method: 'POST',
      url: '/api/sidecar/standardization',
      payload: { software: 'SPSS', dataset_sha256: 'b'.repeat(64), columns: [], params: {} },
    });
    expect(res.statusCode).toBe(200);
    const body = res.json();
    expect(body.coverage_value).toBe('none');
    expect(body.text).toBeUndefined();
    await app.close();
  });
});

describe('POST /api/snapshot/export (wired)', () => {
  it('exports a completed run and returns { snapshot_path, sha256 }', async () => {
    const dir = freshTmp();
    const dest = join(dir, 'out.zip');
    const app = buildRouter({ state: wiredState() });
    const res = await app.inject({
      method: 'POST',
      url: '/api/snapshot/export',
      payload: { run_id: 'run-1', destination: dest },
    });
    expect(res.statusCode).toBe(200);
    const body = res.json();
    expect(contract.sidecar.snapshotExportResponse.safeParse(body).success).toBe(true);
    expect(body.snapshot_path).toBe(dest);
    expect(body.sha256).toMatch(/^[0-9a-f]{64}$/);
    expect(existsSync(dest)).toBe(true);
    await app.close();
  });

  it('returns 500 for an unknown run id', async () => {
    const dir = freshTmp();
    const app = buildRouter({ state: wiredState() });
    const res = await app.inject({
      method: 'POST',
      url: '/api/snapshot/export',
      payload: { run_id: 'missing', destination: join(dir, 'x.zip') },
    });
    expect(res.statusCode).toBe(500);
    await app.close();
  });
});
