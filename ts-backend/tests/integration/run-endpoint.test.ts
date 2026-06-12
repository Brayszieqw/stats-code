// tests/integration/run-endpoint.test.ts — POST /api/sessions/:sid/run (task 3.5).
//
// Success returns a SkillResult; missing args → 422 SkillInvalidArgs; timeout →
// 504 SkillTimeout; archived → 409; unknown session → 404.
//
// _Requirements: 12.3, 12.4, 12.5, 12.6_

import { describe, it, expect } from 'vitest';
import { createHash } from 'node:crypto';
import {
  buildRouter,
  MemSessionStore,
  SkillRegistry,
  SkillRunner,
  SkillRunErrorException,
  contract,
  type AppState,
  type DatasetStore,
  type DatasetSummary,
} from '@stats-code/server';

const CSV = 'y,x,arm\n1,1,A\n2,2,A\n3,3.1,A\n4,3.9,B\n5,5,B\n6,6,B\n';
const BYTES = new TextEncoder().encode(CSV);

function fakeSummary(datasetId: string): DatasetSummary {
  return {
    dataset_id: datasetId,
    file_name: 'data.csv',
    size_bytes: BYTES.byteLength,
    encoding: 'Utf8',
    row_count: 6,
    columns: [
      { name: 'y', inferred_type: 'Numeric', missing_count: 0 },
      { name: 'x', inferred_type: 'Numeric', missing_count: 0 },
      { name: 'arm', inferred_type: 'Categorical', missing_count: 0 },
    ],
    uploaded_at: '2026-01-01T00:00:00Z',
    sha256: createHash('sha256').update(BYTES).digest('hex'),
  };
}

/** A trivial in-memory DatasetStore returning the fixed CSV bytes. */
function memDatasetStore(): DatasetStore {
  return {
    saveAndParse: async (_sid, _file, _bytes) => fakeSummary('unused'),
    readRawById: async () => BYTES,
  };
}

function fullState(overrides: Partial<AppState> = {}): AppState {
  const registry = SkillRegistry.withDefaults();
  return {
    sessionStore: new MemSessionStore(),
    datasetStore: memDatasetStore(),
    skillRegistry: registry,
    skillRunner: new SkillRunner(registry),
    ...overrides,
  };
}

/** Create a session and attach a dataset summary, returning [app, sid, datasetId]. */
async function sessionWithDataset(state: AppState) {
  const app = buildRouter({ state });
  const created = (await app.inject({ method: 'POST', url: '/api/sessions' })).json();
  const datasetId = '33333333-3333-4333-8333-333333333333';
  await state.sessionStore.appendDataset(created.id, fakeSummary(datasetId));
  return { app, sid: created.id as string, datasetId };
}

describe('POST /api/sessions/:sid/run (Requirements 12.3–12.6)', () => {
  it('returns a SkillResult on success (R12.3)', async () => {
    const state = fullState();
    const { app, sid, datasetId } = await sessionWithDataset(state);
    const res = await app.inject({
      method: 'POST',
      url: `/api/sessions/${sid}/run`,
      payload: { skill_id: 'model_linear', dataset_id: datasetId, args: { outcome: 'y', predictors: ['x'], dataset_id: datasetId } },
    });
    expect(res.statusCode).toBe(200);
    const body = res.json();
    expect(contract.domain.skillResult.safeParse(body).success).toBe(true);
    expect(body.analysis.algorithm_id).toBe('linear');
    await app.close();
  });

  it('accepts algorithm ids and top-level dataset_id, then returns frontend-ready analysis meta', async () => {
    const state = fullState();
    const { app, sid, datasetId } = await sessionWithDataset(state);
    const res = await app.inject({
      method: 'POST',
      url: `/api/sessions/${sid}/run`,
      payload: { skill_id: 'linear', dataset_id: datasetId, args: { outcome: 'y', predictors: ['x'] } },
    });

    expect(res.statusCode).toBe(200);
    const body = res.json();
    expect(body.analysis).toEqual(
      expect.objectContaining({
        algorithm_id: 'linear',
        dataset_id: datasetId,
        dataset_sha256: expect.any(String),
        run_id: expect.any(String),
        run_status: 'completed',
      }),
    );
    expect(body.analysis.columns).toEqual(fakeSummary(datasetId).columns);
    expect(body.analysis.params).toEqual({
      dataset_id: datasetId,
      outcome: 'y',
      predictors: ['x'],
    });
    await app.close();
  });

  it('runs tableone and ttest skill ids used by the pro configurator', async () => {
    const state = fullState();
    const { app, sid, datasetId } = await sessionWithDataset(state);
    const table = await app.inject({
      method: 'POST',
      url: `/api/sessions/${sid}/run`,
      payload: {
        skill_id: 'tableone',
        dataset_id: datasetId,
        args: { group: 'arm', continuous: ['y', 'x'], categorical: ['arm'] },
      },
    });
    expect(table.statusCode).toBe(200);
    expect(table.json().analysis.algorithm_id).toBe('tableone');

    const ttest = await app.inject({
      method: 'POST',
      url: `/api/sessions/${sid}/run`,
      payload: { skill_id: 'ttest', dataset_id: datasetId, args: { group: 'arm', testVar: 'y' } },
    });
    expect(ttest.statusCode).toBe(200);
    expect(ttest.json().analysis.algorithm_id).toBe('ttest');
    expect(ttest.json().payload.p_value).toBeGreaterThanOrEqual(0);
    await app.close();
  });

  it('missing required args → 422 SkillInvalidArgs (R12.4)', async () => {
    const state = fullState();
    const { app, sid, datasetId } = await sessionWithDataset(state);
    const res = await app.inject({
      method: 'POST',
      url: `/api/sessions/${sid}/run`,
      // omit predictors; the route supplies dataset_id from the top-level body.
      payload: { skill_id: 'model_linear', dataset_id: datasetId, args: { outcome: 'y' } },
    });
    expect(res.statusCode).toBe(422);
    expect(res.json().error_code).toBe('SkillInvalidArgs');
    await app.close();
  });

  it('timeout → 504 SkillTimeout (R12.5)', async () => {
    // Stub a runner that surfaces a structured timeout SkillRunError.
    const registry = SkillRegistry.withDefaults();
    const timeoutRunner = {
      run: () => Promise.reject(new SkillRunErrorException({ kind: 'timeout', wallSecs: 120 })),
    };
    const state = fullState({ skillRegistry: registry, skillRunner: timeoutRunner });
    const { app, sid, datasetId } = await sessionWithDataset(state);
    const res = await app.inject({
      method: 'POST',
      url: `/api/sessions/${sid}/run`,
      payload: { skill_id: 'model_linear', dataset_id: datasetId, args: { outcome: 'y', predictors: ['x'] } },
    });
    expect(res.statusCode).toBe(504);
    expect(res.json().error_code).toBe('SkillTimeout');
    await app.close();
  });

  it('archived session → 409 SessionArchived (R12.6)', async () => {
    const state = fullState();
    const { app, sid, datasetId } = await sessionWithDataset(state);
    const session = await state.sessionStore.get(sid);
    session.status = 'Archived';
    const res = await app.inject({
      method: 'POST',
      url: `/api/sessions/${sid}/run`,
      payload: { skill_id: 'model_linear', dataset_id: datasetId, args: { outcome: 'y', predictors: ['x'] } },
    });
    expect(res.statusCode).toBe(409);
    expect(res.json().error_code).toBe('SessionArchived');
    await app.close();
  });

  it('unknown session → 404 SessionNotFound', async () => {
    const state = fullState();
    const app = buildRouter({ state });
    const res = await app.inject({
      method: 'POST',
      url: '/api/sessions/00000000-0000-4000-8000-000000000000/run',
      payload: { skill_id: 'model_linear', dataset_id: 'x', args: {} },
    });
    expect(res.statusCode).toBe(404);
    expect(res.json().error_code).toBe('SessionNotFound');
    await app.close();
  });
});
