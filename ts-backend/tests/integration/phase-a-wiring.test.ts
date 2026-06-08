// tests/integration/phase-a-wiring.test.ts — Phase A production wiring (task 1.2).
//
// Boots the production AppState from `defaultState()` and asserts the
// coverage-matrix and sidecar routes respond as specified, with DTO shapes
// conforming to the Rust contract. Also confirms the snapshot route stays 503
// (no run resolver wired in production).
//
// _Requirements: 1.3, 1.4, 1.6_

import { describe, it, expect } from 'vitest';
import { buildRouter, contract } from '@stats-code/server';
import { defaultState } from '@stats-code/api';

describe('Phase A — production defaultState() wiring', () => {
  it('GET /api/coverage-matrix → 200 with a contract-conforming matrix body', async () => {
    const app = buildRouter({ state: defaultState() });
    const res = await app.inject({ method: 'GET', url: '/api/coverage-matrix' });
    expect(res.statusCode).toBe(200);
    const body = res.json();
    expect(contract.sidecar.coverageMatrix.safeParse(body).success).toBe(true);
    expect(body.algorithms.length).toBeGreaterThan(0);
    await app.close();
  });

  it('POST /api/sidecar/:algorithm_id → 200 with a contract-conforming snippet', async () => {
    const app = buildRouter({ state: defaultState() });
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
    await app.close();
  });

  it('POST /api/snapshot/export → 503 (no run resolver wired in production)', async () => {
    const app = buildRouter({ state: defaultState() });
    const res = await app.inject({
      method: 'POST',
      url: '/api/snapshot/export',
      payload: { run_id: 'run-1', destination: 'out.zip' },
    });
    expect(res.statusCode).toBe(503);
    await app.close();
  });
});
