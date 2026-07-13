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
  createResearchWorkflowService,
  MemSessionStore,
  SkillRegistry,
  SkillRunner,
  SkillRunErrorException,
  contract,
  type AppState,
  type DatasetStore,
  type DatasetSummary,
} from '@stats-code/server';

const CSV = 'participant_id,y,x,arm\nP001,1,1,A\nP002,2,2,A\nP003,3,3.1,A\nP004,4,3.9,B\nP005,5,5,B\nP006,6,6,B\n';
const BYTES = new TextEncoder().encode(CSV);

function fakeSummary(datasetId: string): DatasetSummary {
  return {
    dataset_id: datasetId,
    file_name: 'data.csv',
    size_bytes: BYTES.byteLength,
    encoding: 'Utf8',
    row_count: 6,
    columns: [
      { name: 'participant_id', inferred_type: 'String', missing_count: 0 },
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
  const sessionStore = overrides.sessionStore ?? new MemSessionStore();
  const datasetStore = overrides.datasetStore ?? memDatasetStore();
  const registry = overrides.skillRegistry ?? SkillRegistry.withDefaults();
  const runner = overrides.skillRunner ?? new SkillRunner(registry);
  const researchWorkflow = overrides.researchWorkflow ?? createResearchWorkflowService({
    sessionStore,
    datasetStore,
    registry,
    runner,
  });
  return {
    ...overrides,
    sessionStore,
    datasetStore,
    skillRegistry: registry,
    skillRunner: runner,
    researchWorkflow,
  };
}

const protocol = {
  status: 'Approved',
  research_question: 'x 是否与 y 相关？',
  study_design: 'cross_sectional',
  population: '演示成人数据',
  eligibility_criteria: '每人一行',
  exposure: 'x',
  comparator: '按 x 分组或每增加 1 单位',
  outcome: 'y',
  time_zero: '基线',
  follow_up: '横断面',
  analysis_unit: '参与者',
  estimand: '组间差或回归系数',
  confounders: '',
  missing_data_strategy: '完整案例',
  primary_analysis: '预先指定的统计模型',
  sensitivity_analysis: '',
} as const;

/** Create a session and attach a dataset summary, returning [app, sid, datasetId]. */
async function sessionWithDataset(state: AppState) {
  const app = buildRouter({ state });
  const created = (await app.inject({ method: 'POST', url: '/api/sessions' })).json();
  const datasetId = '33333333-3333-4333-8333-333333333333';
  await state.sessionStore.appendDataset(created.id, fakeSummary(datasetId));
  const protocolResponse = await app.inject({
    method: 'PATCH',
    url: `/api/sessions/${created.id}/protocol`,
    payload: protocol,
  });
  expect(protocolResponse.statusCode).toBe(200);
  return { app, sid: created.id as string, datasetId };
}

async function approve(
  app: ReturnType<typeof buildRouter>,
  sid: string,
  datasetId: string,
  skillId: string,
  args: Record<string, unknown>,
): Promise<string> {
  const auditResponse = await app.inject({
    method: 'POST',
    url: `/api/sessions/${sid}/datasets/${datasetId}/audit`,
    payload: {
      skill_id: skillId,
      args,
      expected_protocol_version: 1,
    },
  });
  expect(auditResponse.statusCode).toBe(200);
  const audit = auditResponse.json();
  const response = await app.inject({
    method: 'POST',
    url: `/api/sessions/${sid}/analysis-plans/approve`,
    payload: {
      skill_id: skillId,
      dataset_id: datasetId,
      args,
      expected_protocol_version: 1,
      expected_audit_id: audit.audit_id,
      expected_audit_sha256: audit.audit_sha256,
      audit_roles: audit.roles,
    },
  });
  expect(response.statusCode).toBe(201);
  return response.json().plan_id as string;
}

describe('POST /api/sessions/:sid/run (Requirements 12.3–12.6)', () => {
  it('returns a SkillResult on success (R12.3)', async () => {
    const state = fullState();
    const { app, sid, datasetId } = await sessionWithDataset(state);
    const args = { outcome: 'y', predictors: ['x'], dataset_id: datasetId };
    const planId = await approve(app, sid, datasetId, 'model_linear', args);
    const res = await app.inject({
      method: 'POST',
      url: `/api/sessions/${sid}/run`,
      payload: { skill_id: 'model_linear', dataset_id: datasetId, args, plan_id: planId },
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
    const args = { outcome: 'y', predictors: ['x'] };
    const planId = await approve(app, sid, datasetId, 'linear', args);
    const res = await app.inject({
      method: 'POST',
      url: `/api/sessions/${sid}/run`,
      payload: { skill_id: 'linear', dataset_id: datasetId, args, plan_id: planId },
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
    const tableArgs = { group: 'arm', continuous: ['y', 'x'], categorical: ['arm'] };
    const tablePlanId = await approve(app, sid, datasetId, 'tableone', tableArgs);
    const table = await app.inject({
      method: 'POST',
      url: `/api/sessions/${sid}/run`,
      payload: {
        skill_id: 'tableone',
        dataset_id: datasetId,
        args: tableArgs,
        plan_id: tablePlanId,
      },
    });
    expect(table.statusCode).toBe(200);
    expect(table.json().analysis.algorithm_id).toBe('tableone');

    const ttestArgs = { group: 'arm', testVar: 'y' };
    const ttestPlanId = await approve(app, sid, datasetId, 'ttest', ttestArgs);
    const ttest = await app.inject({
      method: 'POST',
      url: `/api/sessions/${sid}/run`,
      payload: { skill_id: 'ttest', dataset_id: datasetId, args: ttestArgs, plan_id: ttestPlanId },
    });
    expect(ttest.statusCode).toBe(200);
    expect(ttest.json().analysis.algorithm_id).toBe('ttest');
    expect(ttest.json().payload.p_value).toBeGreaterThanOrEqual(0);
    await app.close();
  });

  it('rejects missing required args before a plan can be approved (R12.4)', async () => {
    const state = fullState();
    const { app, sid, datasetId } = await sessionWithDataset(state);
    const args = { outcome: 'y' };
    const res = await app.inject({
      method: 'POST',
      url: `/api/sessions/${sid}/datasets/${datasetId}/audit`,
      payload: {
        skill_id: 'model_linear',
        args,
        expected_protocol_version: 1,
      },
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
    const args = { outcome: 'y', predictors: ['x'] };
    const planId = await approve(app, sid, datasetId, 'model_linear', args);
    const res = await app.inject({
      method: 'POST',
      url: `/api/sessions/${sid}/run`,
      payload: { skill_id: 'model_linear', dataset_id: datasetId, args, plan_id: planId },
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
