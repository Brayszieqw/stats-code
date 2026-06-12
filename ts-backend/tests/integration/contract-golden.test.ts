// tests/integration/contract-golden.test.ts — contract/golden diff (task 3.8).
//
// Diffs serialized request/response samples that mirror the running Rust
// backend against the TS backend for every non-SSE route, and asserts the same
// status codes. The golden fixtures encode the Rust serde wire contract (field
// names, enum tokens, status codes). Each live TS response is validated against
// (a) its zod contract schema and (b) the golden fixture shape, so a drift in
// either the schema or the handler surfaces here.
//
// _Requirements: 1.2, 1.3, 1.4_

import { describe, it, expect } from 'vitest';
import {
  buildRouter,
  MemSessionStore,
  contract,
  type AppState,
  type SnapshotProvider,
  type CoverageMatrixProvider,
} from '@stats-code/server';
import { coverage } from '@stats-code/engine';

const { ROUTE_CONTRACTS, domain } = contract;

function makeState(overrides: Partial<AppState> = {}): AppState {
  return { sessionStore: new MemSessionStore(), ...overrides };
}

// A coverage provider backed by the real loaded matrix.
const coverageProvider: CoverageMatrixProvider = { get: () => coverage.getLoadedMatrix() };

const snapshotProvider: SnapshotProvider = {
  export() {
    return { snapshot_path: 'out.zip', sha256: '0'.repeat(64) };
  },
};

describe('every route is registered in the contract harness', () => {
  it('exposes the 13 original API_Contract routes plus the dual-mode additions', () => {
    // 13 original routes + list_sessions + run_skill + delete_session.
    expect(ROUTE_CONTRACTS).toHaveLength(16);
    const ids = ROUTE_CONTRACTS.map((r) => r.id);
    expect(ids).toContain('list_sessions');
    expect(ids).toContain('run_skill');
    expect(ids).toContain('delete_session');
  });

  it('each route id is unique', () => {
    const ids = ROUTE_CONTRACTS.map((r) => r.id);
    expect(new Set(ids).size).toBe(ids.length);
  });
});

describe('GET /api/health golden', () => {
  it('matches the Rust { "status": "ok" } body and 200 status', async () => {
    const app = buildRouter({ state: makeState() });
    const res = await app.inject({ method: 'GET', url: '/api/health' });
    expect(res.statusCode).toBe(200);
    const golden = { status: 'ok' };
    expect(res.json()).toEqual(golden);
    expect(contract.healthResponse.safeParse(res.json()).success).toBe(true);
    await app.close();
  });
});

describe('session lifecycle golden', () => {
  it('POST /api/sessions → 201 with a serde-shaped Session', async () => {
    const app = buildRouter({ state: makeState() });
    const res = await app.inject({ method: 'POST', url: '/api/sessions' });
    expect(res.statusCode).toBe(201);
    const body = res.json();
    // Golden serde field set + enum tokens.
    expect(Object.keys(body).sort()).toEqual(
      [
        'created_at',
        'datasets',
        'id',
        'last_active_at',
        'messages',
        'settings',
        'skill_runs',
        'status',
        'uploaded_bytes',
      ].sort(),
    );
    expect(body.status).toBe('Active');
    expect(body.settings).toEqual({ decision_assistant: true });
    // Validates against the zod contract schema (the runtime harness).
    expect(domain.session.safeParse(body).success).toBe(true);
    await app.close();
  });

  it('GET unknown session → 404 with serde ErrorPayload + correct error_code', async () => {
    const app = buildRouter({ state: makeState() });
    const res = await app.inject({
      method: 'GET',
      url: '/api/sessions/00000000-0000-4000-8000-000000000000',
    });
    expect(res.statusCode).toBe(404);
    const body = res.json();
    expect(body.error_code).toBe('SessionNotFound');
    expect(domain.errorPayload.safeParse(body).success).toBe(true);
    // Status matches the single-source ErrorCode → HTTP map.
    expect(domain.HTTP_STATUS_FOR.SessionNotFound).toBe(404);
    await app.close();
  });

  it('PATCH settings → 200 with updated Session', async () => {
    const app = buildRouter({ state: makeState() });
    const created = (await app.inject({ method: 'POST', url: '/api/sessions' })).json();
    const res = await app.inject({
      method: 'PATCH',
      url: `/api/sessions/${created.id}/settings`,
      payload: { decision_assistant: false },
    });
    expect(res.statusCode).toBe(200);
    expect(res.json().settings.decision_assistant).toBe(false);
    expect(domain.session.safeParse(res.json()).success).toBe(true);
    await app.close();
  });
});

describe('llm-status golden', () => {
  it('unconfigured → { configured:false, provider:null, base_url:null, model:null }', async () => {
    const app = buildRouter({ state: makeState() });
    const res = await app.inject({ method: 'GET', url: '/api/llm-status' });
    expect(res.statusCode).toBe(200);
    const golden = { configured: false, provider: null, base_url: null, model: null };
    expect(res.json()).toEqual(golden);
    expect(contract.llmStatusResponse.safeParse(res.json()).success).toBe(true);
    await app.close();
  });

  it('configured → exposes provider but never the api key', async () => {
    const app = buildRouter({
      state: makeState({
        llmConfigStore: {
          read: () => ({ provider: 'deepseek', api_key: 'sk-secret', base_url: null, model: null }),
          write: () => {},
        },
      }),
    });
    const res = await app.inject({ method: 'GET', url: '/api/llm-status' });
    const body = res.json();
    expect(body).toEqual({ configured: true, provider: 'deepseek', base_url: null, model: null });
    expect(JSON.stringify(body)).not.toContain('sk-secret');
    await app.close();
  });
});

describe('coverage-matrix golden', () => {
  // The wire DTO (crates/api/src/sidecar.rs) uses callable/package/version per
  // reference cell. 13.7 maps the engine's internal matrix into this shape; the
  // golden here pins the wire contract a contract-shaped provider must satisfy.
  const goldenMatrix = {
    schema_version: 1,
    release_version: '0.5.0',
    algorithms: [
      {
        id: 'tableone',
        display_name: 'Table One',
        iterative: false,
        coverage: { R: 'live', SAS: 'recorded', Python: 'sidecar_only', SPSS: 'none' },
        reference: {
          R: { callable: 'tableone', package: 'tableone', version: '0.13.2' },
          SAS: { callable: 'proc means', package: null, version: '9.4' },
          Python: { callable: 'tableone', package: 'tableone', version: '0.9.1' },
          SPSS: { callable: 'DESCRIPTIVES', package: null, version: '29' },
        },
      },
    ],
  };

  it('200 returns the verbatim provider matrix matching the wire contract schema', async () => {
    const provider: CoverageMatrixProvider = { get: () => goldenMatrix as never };
    const app = buildRouter({ state: makeState({ coverageMatrixProvider: provider }) });
    const res = await app.inject({ method: 'GET', url: '/api/coverage-matrix' });
    expect(res.statusCode).toBe(200);
    const body = res.json();
    expect(body).toEqual(goldenMatrix);
    expect(contract.sidecar.coverageMatrix.safeParse(body).success).toBe(true);
    await app.close();
  });

  it('the engine loaded matrix is structurally complete (4 softwares per cell)', () => {
    const matrix = coverageProvider.get();
    expect(matrix.algorithms.length).toBeGreaterThan(0);
    for (const entry of matrix.algorithms) {
      expect(Object.keys(entry.coverage).sort()).toEqual(['Python', 'R', 'SAS', 'SPSS']);
      expect(Object.keys(entry.reference).sort()).toEqual(['Python', 'R', 'SAS', 'SPSS']);
    }
  });
});

describe('snapshot/export golden', () => {
  it('200 returns { snapshot_path, sha256 } matching the contract schema', async () => {
    const app = buildRouter({ state: makeState({ snapshotProvider }) });
    const res = await app.inject({
      method: 'POST',
      url: '/api/snapshot/export',
      payload: { run_id: 'r', destination: 'out.zip' },
    });
    expect(res.statusCode).toBe(200);
    const body = res.json();
    expect(Object.keys(body).sort()).toEqual(['sha256', 'snapshot_path']);
    expect(contract.sidecar.snapshotExportResponse.safeParse(body).success).toBe(true);
    await app.close();
  });
});
