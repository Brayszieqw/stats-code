import { createHash } from 'node:crypto';
import { describe, expect, it, vi } from 'vitest';
import {
  buildRouter,
  createOrchestrator,
  createResearchWorkflowService,
  MemSessionStore,
  SkillRegistry,
  SkillRunner,
  type AppState,
  type DatasetStore,
  type DatasetSummary,
  type LlmProvider,
} from '@stats-code/server';

const CLEAN_CSV = 'participant_id,y,x\nP001,1,1\nP002,2,2\nP003,3,3.1\nP004,4,3.9\nP005,5,5\n';
const BLOCKED_CSV = 'participant_id,y,x\nP001,1,1\nP001,2,2\n';
const DATASET_ID = '33333333-3333-4333-8333-333333333333';

function summary(csv: string): DatasetSummary {
  const bytes = new TextEncoder().encode(csv);
  return {
    dataset_id: DATASET_ID,
    file_name: 'data.csv',
    size_bytes: bytes.byteLength,
    encoding: 'Utf8',
    row_count: csv.trim().split(/\r?\n/).length - 1,
    columns: [
      { name: 'participant_id', inferred_type: 'String', missing_count: 0 },
      { name: 'y', inferred_type: 'Numeric', missing_count: 0 },
      { name: 'x', inferred_type: 'Numeric', missing_count: 0 },
    ],
    uploaded_at: '2026-01-01T00:00:00Z',
    sha256: createHash('sha256').update(bytes).digest('hex'),
  };
}

function makeState(csv = CLEAN_CSV): AppState & { runnerSpy: ReturnType<typeof vi.spyOn> } {
  const bytes = new TextEncoder().encode(csv);
  const sessionStore = new MemSessionStore();
  const datasetStore: DatasetStore = {
    saveAndParse: async () => summary(csv),
    readRawById: async () => bytes,
  };
  const registry = SkillRegistry.withDefaults();
  const runner = new SkillRunner(registry);
  const runnerSpy = vi.spyOn(runner, 'run');
  const researchWorkflow = createResearchWorkflowService({
    sessionStore,
    datasetStore,
    registry,
    runner,
    now: () => new Date('2026-07-13T08:00:00.000Z'),
  });
  return { sessionStore, datasetStore, skillRegistry: registry, skillRunner: runner, researchWorkflow, runnerSpy };
}

const protocol = {
  status: 'Approved',
  research_question: 'x 与 y 是否相关？',
  study_design: 'cross_sectional',
  population: '演示成年人',
  eligibility_criteria: '每人一行且主键唯一',
  exposure: 'x',
  comparator: 'x 较低者',
  outcome: 'y',
  time_zero: '基线',
  follow_up: '横断面',
  analysis_unit: '参与者',
  estimand: 'x 每增加 1 单位时 y 的均值差',
  confounders: '无',
  missing_data_strategy: '完整案例',
  primary_analysis: '多元线性回归',
  sensitivity_analysis: '稳健性检查',
} as const;

const runSpec = {
  skill_id: 'model_linear',
  dataset_id: DATASET_ID,
  args: { outcome: 'y', predictors: ['x'] },
};

function linearIntentLlm(): LlmProvider {
  return {
    providerId: 'openai',
    redactedConfig: () => ({ provider: 'openai', baseUrl: 'x', model: 'm' }),
    // eslint-disable-next-line @typescript-eslint/require-await
    async *chatStream() {
      yield {
        type: 'text_delta',
        text: '{"skill_ids":["model_linear"],"resolved_args":{"outcome":"y","predictors":["x"]},"has_query_intent":true,"text_response":null}',
      };
      yield { type: 'done' };
    },
  };
}

async function seed(state: AppState) {
  const app = buildRouter({ state });
  const sid = (await app.inject({ method: 'POST', url: '/api/sessions' })).json().id as string;
  await state.sessionStore.appendDataset(sid, summary(CLEAN_CSV));
  return { app, sid };
}

async function auditPlan(
  app: ReturnType<typeof buildRouter>,
  sid: string,
  spec: typeof runSpec,
  auditRoles?: Record<string, unknown>,
) {
  const response = await app.inject({
    method: 'POST',
    url: `/api/sessions/${sid}/datasets/${spec.dataset_id}/audit`,
    payload: {
      skill_id: spec.skill_id,
      args: spec.args,
      expected_protocol_version: 1,
      ...(auditRoles ? { audit_roles: auditRoles } : {}),
    },
  });
  expect(response.statusCode).toBe(200);
  return response.json();
}

function approvalPayload(audit: Record<string, unknown>) {
  return {
    ...runSpec,
    expected_protocol_version: 1,
    expected_audit_id: audit.audit_id,
    expected_audit_sha256: audit.audit_sha256,
    audit_roles: audit.roles,
  };
}

async function approvePlan(
  app: ReturnType<typeof buildRouter>,
  sid: string,
) {
  const audit = await auditPlan(app, sid, runSpec);
  const response = await app.inject({
    method: 'POST',
    url: `/api/sessions/${sid}/analysis-plans/approve`,
    payload: approvalPayload(audit),
  });
  expect(response.statusCode).toBe(201);
  return { audit, approval: response.json() };
}

describe('server-enforced research workflow gate', () => {
  it('maps a real conversation gate rejection to a structured SSE error without running the skill', async () => {
    const state = makeState();
    state.messageHandler = createOrchestrator({
      sessionStore: state.sessionStore,
      registry: SkillRegistry.withDefaults(),
      researchWorkflow: state.researchWorkflow!,
      llmProviderFactory: () => linearIntentLlm(),
    });
    const { app, sid } = await seed(state);
    const protocolResponse = await app.inject({
      method: 'PATCH',
      url: `/api/sessions/${sid}/protocol`,
      payload: protocol,
    });
    expect(protocolResponse.statusCode).toBe(200);

    const response = await app.inject({
      method: 'POST',
      url: `/api/sessions/${sid}/messages`,
      payload: { text: '对 y 做线性回归' },
    });
    const frames = response.body.trim().split('\n\n').map((frame) => {
      const [eventLine, dataLine] = frame.split('\n');
      return {
        event: eventLine!.slice('event: '.length),
        data: JSON.parse(dataLine!.slice('data: '.length)) as Record<string, unknown>,
      };
    });

    expect(response.statusCode).toBe(200);
    expect(response.headers['content-type']).toContain('text/event-stream');
    expect(frames.map((frame) => frame.event)).toEqual(['skill_call', 'error', 'done']);
    expect(frames[1]!.data).toEqual({
      error_code: 'ResearchApprovalRequired',
      message: expect.any(String),
    });
    expect(state.runnerSpy).not.toHaveBeenCalled();
    await app.close();
  });

  it('rejects unapproved runs and client-reported approval timestamps before invoking the runner', async () => {
    const state = makeState();
    const { app, sid } = await seed(state);

    const noProtocol = await app.inject({ method: 'POST', url: `/api/sessions/${sid}/run`, payload: runSpec });
    expect(noProtocol.statusCode).toBe(428);
    expect(noProtocol.json().error_code).toBe('ResearchProtocolRequired');

    await app.inject({ method: 'PATCH', url: `/api/sessions/${sid}/protocol`, payload: protocol });
    const noPlan = await app.inject({ method: 'POST', url: `/api/sessions/${sid}/run`, payload: runSpec });
    expect(noPlan.statusCode).toBe(428);
    expect(noPlan.json().error_code).toBe('ResearchApprovalRequired');

    const forged = await app.inject({
      method: 'POST',
      url: `/api/sessions/${sid}/run`,
      payload: {
        ...runSpec,
        plan_id: '44444444-4444-4444-8444-444444444444',
        args: { ...runSpec.args, workflow_approval: { plan_approved_at: '2099-01-01T00:00:00Z' } },
      },
    });
    expect(forged.statusCode).toBe(422);
    expect(forged.json().error_code).toBe('SkillInvalidArgs');
    expect(state.runnerSpy).not.toHaveBeenCalled();
    await app.close();
  });

  it('audits, creates a server approval, and executes only the exact approved spec', async () => {
    const state = makeState();
    const { app, sid } = await seed(state);

    const protocolResponse = await app.inject({
      method: 'PATCH',
      url: `/api/sessions/${sid}/protocol`,
      payload: protocol,
    });
    expect(protocolResponse.statusCode).toBe(200);
    expect(protocolResponse.json().research_protocol).toMatchObject({
      status: 'Approved',
      version: 1,
      approved_at: '2026-07-13T08:00:00.000Z',
    });

    const audit = await auditPlan(app, sid, runSpec);
    const approval = await app.inject({
      method: 'POST',
      url: `/api/sessions/${sid}/analysis-plans/approve`,
      payload: {
        ...runSpec,
        expected_protocol_version: 1,
        expected_audit_id: audit.audit_id,
        expected_audit_sha256: audit.audit_sha256,
        audit_roles: audit.roles,
      },
    });
    expect(approval.statusCode).toBe(201);
    expect(approval.json()).toMatchObject({
      status: 'Approved',
      protocol_version: 1,
      dataset_id: DATASET_ID,
      approved_at: '2026-07-13T08:00:00.000Z',
    });
    expect(approval.json().plan_id).toMatch(/[0-9a-f-]{36}/);
    expect(approval.json().run_spec_sha256).toMatch(/^[0-9a-f]{64}$/);

    const run = await app.inject({
      method: 'POST',
      url: `/api/sessions/${sid}/run`,
      payload: { ...runSpec, plan_id: approval.json().plan_id },
    });
    expect(run.statusCode).toBe(200);
    expect(run.json().analysis).toMatchObject({
      plan_id: approval.json().plan_id,
      run_status: 'completed',
    });
    expect(state.runnerSpy).toHaveBeenCalledTimes(1);

    const changedArgs = await app.inject({
      method: 'POST',
      url: `/api/sessions/${sid}/run`,
      payload: {
        ...runSpec,
        plan_id: approval.json().plan_id,
        args: { outcome: 'y', predictors: ['x'], alpha: 0.01 },
      },
    });
    expect(changedArgs.statusCode).toBe(409);
    expect(changedArgs.json().error_code).toBe('ResearchApprovalStale');
    expect(state.runnerSpy).toHaveBeenCalledTimes(1);
    await app.close();
  });

  it('invalidates an old plan when the protocol version changes', async () => {
    const state = makeState();
    const { app, sid } = await seed(state);
    await app.inject({ method: 'PATCH', url: `/api/sessions/${sid}/protocol`, payload: protocol });
    const audit = await auditPlan(app, sid, runSpec);
    const approval = await app.inject({
      method: 'POST',
      url: `/api/sessions/${sid}/analysis-plans/approve`,
      payload: {
        ...runSpec,
        expected_protocol_version: 1,
        expected_audit_id: audit.audit_id,
        expected_audit_sha256: audit.audit_sha256,
        audit_roles: audit.roles,
      },
    });

    const updated = await app.inject({
      method: 'PATCH',
      url: `/api/sessions/${sid}/protocol`,
      payload: { ...protocol, expected_version: 1, outcome: '修订后的 y' },
    });
    expect(updated.json().research_protocol.version).toBe(2);

    const run = await app.inject({
      method: 'POST',
      url: `/api/sessions/${sid}/run`,
      payload: { ...runSpec, plan_id: approval.json().plan_id },
    });
    expect(run.statusCode).toBe(409);
    expect(run.json().error_code).toBe('ResearchApprovalStale');
    expect(state.runnerSpy).not.toHaveBeenCalled();
    await app.close();
  });

  it('atomically refuses an approval when the protocol changes during audit recomputation', async () => {
    const bytes = new TextEncoder().encode(CLEAN_CSV);
    const sessionStore = new MemSessionStore();
    let readCount = 0;
    let signalApprovalRead!: () => void;
    let releaseApprovalRead!: () => void;
    const approvalReadStarted = new Promise<void>((resolve) => { signalApprovalRead = resolve; });
    const approvalReadGate = new Promise<void>((resolve) => { releaseApprovalRead = resolve; });
    const datasetStore: DatasetStore = {
      saveAndParse: async () => summary(CLEAN_CSV),
      readRawById: async () => {
        readCount += 1;
        if (readCount === 2) {
          signalApprovalRead();
          await approvalReadGate;
        }
        return bytes;
      },
    };
    const registry = SkillRegistry.withDefaults();
    const runner = new SkillRunner(registry);
    const researchWorkflow = createResearchWorkflowService({ sessionStore, datasetStore, registry, runner });
    const state: AppState = { sessionStore, datasetStore, skillRegistry: registry, skillRunner: runner, researchWorkflow };
    const { app, sid } = await seed(state);
    await app.inject({ method: 'PATCH', url: `/api/sessions/${sid}/protocol`, payload: protocol });
    const audit = await auditPlan(app, sid, runSpec);

    const pendingApproval = app.inject({
      method: 'POST',
      url: `/api/sessions/${sid}/analysis-plans/approve`,
      payload: {
        ...runSpec,
        expected_protocol_version: 1,
        expected_audit_id: audit.audit_id,
        expected_audit_sha256: audit.audit_sha256,
        audit_roles: audit.roles,
      },
    });
    await approvalReadStarted;
    const changed = await app.inject({
      method: 'PATCH',
      url: `/api/sessions/${sid}/protocol`,
      payload: { ...protocol, expected_version: 1, outcome: 'changed during approval' },
    });
    expect(changed.statusCode).toBe(200);
    expect(changed.json().research_protocol.version).toBe(2);
    releaseApprovalRead();

    const approval = await pendingApproval;
    expect(approval.statusCode).toBe(409);
    expect(approval.json().error_code).toBe('ResearchApprovalStale');
    expect((await sessionStore.get(sid)).analysis_plan_approvals).toEqual([]);
    await app.close();
  });

  it('persists audit blockers and refuses to create an approval', async () => {
    const state = makeState(BLOCKED_CSV);
    const app = buildRouter({ state });
    const sid = (await app.inject({ method: 'POST', url: '/api/sessions' })).json().id as string;
    await state.sessionStore.appendDataset(sid, summary(BLOCKED_CSV));
    await app.inject({ method: 'PATCH', url: `/api/sessions/${sid}/protocol`, payload: protocol });

    const maliciousRoles = { primary_key: ['x'] };
    const audit = await auditPlan(app, sid, runSpec, maliciousRoles);
    const approval = await app.inject({
      method: 'POST',
      url: `/api/sessions/${sid}/analysis-plans/approve`,
      payload: {
        ...runSpec,
        expected_protocol_version: 1,
        expected_audit_id: audit.audit_id,
        expected_audit_sha256: audit.audit_sha256,
        audit_roles: maliciousRoles,
      },
    });
    expect(approval.statusCode).toBe(409);
    expect(approval.json().error_code).toBe('ResearchAuditBlocked');
    expect(approval.json().details.findings).toEqual(expect.arrayContaining([
      expect.objectContaining({ code: 'DUPLICATE_PRIMARY_KEY', severity: 'blocker' }),
      expect.objectContaining({ code: 'AUDIT_ROLE_OVERRIDE_REJECTED', severity: 'blocker' }),
    ]));
    expect(state.runnerSpy).not.toHaveBeenCalled();
    expect((await state.sessionStore.get(sid)).dataset_audits).toHaveLength(1);
    await app.close();
  });

  it('rejects execution when the persisted dataset bytes no longer match the approved sha', async () => {
    let bytes = new TextEncoder().encode(CLEAN_CSV);
    const sessionStore = new MemSessionStore();
    const datasetStore: DatasetStore = {
      saveAndParse: async () => summary(CLEAN_CSV),
      readRawById: async () => bytes,
    };
    const registry = SkillRegistry.withDefaults();
    const runner = new SkillRunner(registry);
    const runnerSpy = vi.spyOn(runner, 'run');
    const researchWorkflow = createResearchWorkflowService({ sessionStore, datasetStore, registry, runner });
    const state: AppState = { sessionStore, datasetStore, skillRegistry: registry, skillRunner: runner, researchWorkflow };
    const { app, sid } = await seed(state);
    await app.inject({ method: 'PATCH', url: `/api/sessions/${sid}/protocol`, payload: protocol });
    const { approval } = await approvePlan(app, sid);

    bytes = new TextEncoder().encode(`${CLEAN_CSV}P006,6,6\n`);
    const run = await app.inject({
      method: 'POST',
      url: `/api/sessions/${sid}/run`,
      payload: { ...runSpec, plan_id: approval.plan_id },
    });

    expect(run.statusCode).toBe(409);
    expect(run.json()).toMatchObject({
      error_code: 'ResearchApprovalStale',
      details: { dataset_id: DATASET_ID },
    });
    expect(runnerSpy).not.toHaveBeenCalled();
    await app.close();
  });

  it('rejects execution when the fresh audit becomes blocked', async () => {
    const state = makeState();
    const { app, sid } = await seed(state);
    await app.inject({ method: 'PATCH', url: `/api/sessions/${sid}/protocol`, payload: protocol });
    const { approval } = await approvePlan(app, sid);
    const session = await state.sessionStore.get(sid);
    session.analysis_plan_approvals[0]!.audit_roles = { primary_key: ['x'] };

    const run = await app.inject({
      method: 'POST',
      url: `/api/sessions/${sid}/run`,
      payload: { ...runSpec, plan_id: approval.plan_id },
    });

    expect(run.statusCode).toBe(409);
    expect(run.json().error_code).toBe('ResearchAuditBlocked');
    expect(run.json().details.findings).toEqual(expect.arrayContaining([
      expect.objectContaining({ code: 'AUDIT_ROLE_OVERRIDE_REJECTED', severity: 'blocker' }),
    ]));
    expect((await state.sessionStore.get(sid)).dataset_audits).toHaveLength(2);
    expect(state.runnerSpy).not.toHaveBeenCalled();
    await app.close();
  });

  it('rejects execution when the fresh audit hash drifts from the approval', async () => {
    const state = makeState();
    const { app, sid } = await seed(state);
    await app.inject({ method: 'PATCH', url: `/api/sessions/${sid}/protocol`, payload: protocol });
    const { approval } = await approvePlan(app, sid);
    const session = await state.sessionStore.get(sid);
    session.analysis_plan_approvals[0]!.audit_sha256 = 'f'.repeat(64);

    const run = await app.inject({
      method: 'POST',
      url: `/api/sessions/${sid}/run`,
      payload: { ...runSpec, plan_id: approval.plan_id },
    });

    expect(run.statusCode).toBe(409);
    expect(run.json().error_code).toBe('ResearchApprovalStale');
    expect((await state.sessionStore.get(sid)).dataset_audits).toHaveLength(2);
    expect(state.runnerSpy).not.toHaveBeenCalled();
    await app.close();
  });

  it('rejects execution when the approval references a missing server audit record', async () => {
    const state = makeState();
    const { app, sid } = await seed(state);
    await app.inject({ method: 'PATCH', url: `/api/sessions/${sid}/protocol`, payload: protocol });
    const { approval } = await approvePlan(app, sid);
    (await state.sessionStore.get(sid)).dataset_audits = [];

    const run = await app.inject({
      method: 'POST',
      url: `/api/sessions/${sid}/run`,
      payload: { ...runSpec, plan_id: approval.plan_id },
    });

    expect(run.statusCode).toBe(409);
    expect(run.json().error_code).toBe('ResearchApprovalStale');
    expect(run.json().message).toContain('审计记录不存在');
    expect(state.runnerSpy).not.toHaveBeenCalled();
    await app.close();
  });

  it('fails the final linearization check when the protocol changes during execute recomputation', async () => {
    const bytes = new TextEncoder().encode(CLEAN_CSV);
    const sessionStore = new MemSessionStore();
    let readCount = 0;
    let signalExecuteRead!: () => void;
    let releaseExecuteRead!: () => void;
    const executeReadStarted = new Promise<void>((resolve) => { signalExecuteRead = resolve; });
    const executeReadGate = new Promise<void>((resolve) => { releaseExecuteRead = resolve; });
    const datasetStore: DatasetStore = {
      saveAndParse: async () => summary(CLEAN_CSV),
      readRawById: async () => {
        readCount += 1;
        if (readCount === 3) {
          signalExecuteRead();
          await executeReadGate;
        }
        return bytes;
      },
    };
    const registry = SkillRegistry.withDefaults();
    const runner = new SkillRunner(registry);
    const runnerSpy = vi.spyOn(runner, 'run');
    const researchWorkflow = createResearchWorkflowService({ sessionStore, datasetStore, registry, runner });
    const state: AppState = { sessionStore, datasetStore, skillRegistry: registry, skillRunner: runner, researchWorkflow };
    const { app, sid } = await seed(state);
    await app.inject({ method: 'PATCH', url: `/api/sessions/${sid}/protocol`, payload: protocol });
    const { approval } = await approvePlan(app, sid);

    const pendingRun = app.inject({
      method: 'POST',
      url: `/api/sessions/${sid}/run`,
      payload: { ...runSpec, plan_id: approval.plan_id },
    });
    await executeReadStarted;
    const changed = await app.inject({
      method: 'PATCH',
      url: `/api/sessions/${sid}/protocol`,
      payload: { ...protocol, expected_version: 1, outcome: 'changed during execute' },
    });
    expect(changed.statusCode).toBe(200);
    releaseExecuteRead();

    const run = await pendingRun;
    expect(run.statusCode).toBe(409);
    expect(run.json().error_code).toBe('ResearchApprovalStale');
    expect(run.json().message).toContain('运行复核期间');
    expect(runnerSpy).not.toHaveBeenCalled();
    await app.close();
  });

  it('creates independent approvals for repeated approval of the same spec', async () => {
    const state = makeState();
    const { app, sid } = await seed(state);
    await app.inject({ method: 'PATCH', url: `/api/sessions/${sid}/protocol`, payload: protocol });
    const audit = await auditPlan(app, sid, runSpec);

    const first = await app.inject({
      method: 'POST',
      url: `/api/sessions/${sid}/analysis-plans/approve`,
      payload: approvalPayload(audit),
    });
    const second = await app.inject({
      method: 'POST',
      url: `/api/sessions/${sid}/analysis-plans/approve`,
      payload: approvalPayload(audit),
    });

    expect([first.statusCode, second.statusCode]).toEqual([201, 201]);
    expect(first.json().plan_id).not.toBe(second.json().plan_id);
    expect(first.json().run_spec_sha256).toBe(second.json().run_spec_sha256);
    expect((await state.sessionStore.get(sid)).analysis_plan_approvals).toHaveLength(2);
    await app.close();
  });

  it('atomically persists both independent approvals when approve requests race', async () => {
    const state = makeState();
    const { app, sid } = await seed(state);
    await app.inject({ method: 'PATCH', url: `/api/sessions/${sid}/protocol`, payload: protocol });
    const audit = await auditPlan(app, sid, runSpec);

    const [left, right] = await Promise.all([
      app.inject({
        method: 'POST',
        url: `/api/sessions/${sid}/analysis-plans/approve`,
        payload: approvalPayload(audit),
      }),
      app.inject({
        method: 'POST',
        url: `/api/sessions/${sid}/analysis-plans/approve`,
        payload: approvalPayload(audit),
      }),
    ]);

    expect([left.statusCode, right.statusCode]).toEqual([201, 201]);
    expect(left.json().approval_id).not.toBe(right.json().approval_id);
    expect((await state.sessionStore.get(sid)).analysis_plan_approvals).toHaveLength(2);
    await app.close();
  });

  it('returns 404 for approval against an unknown session', async () => {
    const state = makeState();
    const app = buildRouter({ state });
    const response = await app.inject({
      method: 'POST',
      url: '/api/sessions/00000000-0000-4000-8000-000000000000/analysis-plans/approve',
      payload: {
        ...runSpec,
        expected_protocol_version: 1,
        expected_audit_id: '11111111-1111-4111-8111-111111111111',
        expected_audit_sha256: '0'.repeat(64),
      },
    });

    expect(response.statusCode).toBe(404);
    expect(response.json().error_code).toBe('SessionNotFound');
    await app.close();
  });

  it('rejects direct approval while the research protocol is Draft', async () => {
    const state = makeState();
    const { app, sid } = await seed(state);
    await app.inject({
      method: 'PATCH',
      url: `/api/sessions/${sid}/protocol`,
      payload: { ...protocol, status: 'Draft' },
    });

    const response = await app.inject({
      method: 'POST',
      url: `/api/sessions/${sid}/analysis-plans/approve`,
      payload: {
        ...runSpec,
        expected_protocol_version: 1,
        expected_audit_id: '11111111-1111-4111-8111-111111111111',
        expected_audit_sha256: '0'.repeat(64),
      },
    });

    expect(response.statusCode).toBe(428);
    expect(response.json().error_code).toBe('ResearchProtocolRequired');
    expect((await state.sessionStore.get(sid)).analysis_plan_approvals).toEqual([]);
    await app.close();
  });

  it('allows concurrent executions of the same approved plan and records both runs', async () => {
    const state = makeState();
    const { app, sid } = await seed(state);
    await app.inject({ method: 'PATCH', url: `/api/sessions/${sid}/protocol`, payload: protocol });
    const { approval } = await approvePlan(app, sid);
    const payload = { ...runSpec, plan_id: approval.plan_id };

    const [left, right] = await Promise.all([
      app.inject({ method: 'POST', url: `/api/sessions/${sid}/run`, payload }),
      app.inject({ method: 'POST', url: `/api/sessions/${sid}/run`, payload }),
    ]);

    expect([left.statusCode, right.statusCode]).toEqual([200, 200]);
    expect(left.json().analysis.plan_id).toBe(approval.plan_id);
    expect(right.json().analysis.plan_id).toBe(approval.plan_id);
    expect(state.runnerSpy).toHaveBeenCalledTimes(2);
    expect((await state.sessionStore.get(sid)).skill_runs).toHaveLength(2);
    await app.close();
  });
});
