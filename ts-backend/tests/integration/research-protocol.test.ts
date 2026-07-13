import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { buildRouter, MemSessionStore } from '@stats-code/server';

describe('research protocol route', () => {
  let app: ReturnType<typeof buildRouter>;

  beforeEach(() => {
    app = buildRouter({ state: { sessionStore: new MemSessionStore() } });
  });

  afterEach(async () => {
    await app.close();
  });

  async function createSession(): Promise<{ id: string; research_protocol: null }> {
    const response = await app.inject({ method: 'POST', url: '/api/sessions' });
    expect(response.statusCode).toBe(201);
    return response.json();
  }

  const completeProtocol = {
    status: 'Approved',
    research_question: '吸烟与疾病结局是否相关？',
    study_design: 'cross_sectional',
    population: '演示成人观察性队列',
    eligibility_criteria: '纳入基线信息完整的成人；排除重复记录',
    exposure: 'smoke',
    comparator: '未吸烟',
    outcome: 'disease',
    time_zero: '基线调查时点',
    follow_up: '横断面分析，不涉及随访',
    analysis_unit: '参与者',
    estimand: '吸烟与疾病患病几率的调整后 OR',
    confounders: 'age、sex、bmi',
    missing_data_strategy: '报告缺失并以完整案例为主分析',
    primary_analysis: 'Table One + 多变量 Logistic 回归',
    sensitivity_analysis: '改变协变量集并比较估计稳定性',
  } as const;

  it('creates sessions with an explicit empty protocol slot', async () => {
    const session = await createSession();
    expect(session.research_protocol).toBeNull();
  });

  it('saves a draft and returns it from the session endpoint', async () => {
    const session = await createSession();
    const response = await app.inject({
      method: 'PATCH',
      url: `/api/sessions/${session.id}/protocol`,
      payload: {
        ...completeProtocol,
        status: 'Draft',
        research_question: '待完善的问题',
      },
    });

    expect(response.statusCode).toBe(200);
    expect(response.json().research_protocol).toMatchObject({
      status: 'Draft',
      research_question: '待完善的问题',
      approved_at: null,
    });

    const reloaded = await app.inject({ method: 'GET', url: `/api/sessions/${session.id}` });
    expect(reloaded.json().research_protocol.status).toBe('Draft');
  });

  it('rejects approval when essential protocol fields are blank', async () => {
    const session = await createSession();
    const response = await app.inject({
      method: 'PATCH',
      url: `/api/sessions/${session.id}/protocol`,
      payload: { ...completeProtocol, outcome: '' },
    });

    expect(response.statusCode).toBe(422);
    expect(response.json()).toMatchObject({ error_code: 'SkillInvalidArgs' });
  });

  it('rejects client-supplied server approval metadata instead of silently trusting it', async () => {
    const session = await createSession();
    const response = await app.inject({
      method: 'PATCH',
      url: `/api/sessions/${session.id}/protocol`,
      payload: {
        ...completeProtocol,
        approved_at: '2099-01-01T00:00:00Z',
        approval_id: '11111111-1111-4111-8111-111111111111',
        content_sha256: 'f'.repeat(64),
      },
    });

    expect(response.statusCode).toBe(422);
    expect(response.json()).toMatchObject({ error_code: 'SkillInvalidArgs' });
    expect((await app.inject({ method: 'GET', url: `/api/sessions/${session.id}` })).json().research_protocol).toBeNull();
  });

  it('approves a complete protocol with audit timestamps', async () => {
    const session = await createSession();
    const response = await app.inject({
      method: 'PATCH',
      url: `/api/sessions/${session.id}/protocol`,
      payload: completeProtocol,
    });

    expect(response.statusCode).toBe(200);
    const protocol = response.json().research_protocol;
    expect(protocol).toMatchObject({ status: 'Approved', outcome: 'disease', version: 1 });
    expect(protocol.approval_id).toMatch(/[0-9a-f-]{36}/);
    expect(protocol.content_sha256).toMatch(/^[0-9a-f]{64}$/);
    expect(protocol.state_sha256).toMatch(/^[0-9a-f]{64}$/);
    expect(new Date(protocol.updated_at).toString()).not.toBe('Invalid Date');
    expect(new Date(protocol.approved_at).toString()).not.toBe('Invalid Date');
  });

  it('atomically rejects one of two concurrent updates based on the same version', async () => {
    const session = await createSession();
    await app.inject({ method: 'PATCH', url: `/api/sessions/${session.id}/protocol`, payload: completeProtocol });

    const [left, right] = await Promise.all([
      app.inject({
        method: 'PATCH',
        url: `/api/sessions/${session.id}/protocol`,
        payload: { ...completeProtocol, expected_version: 1, outcome: 'left outcome' },
      }),
      app.inject({
        method: 'PATCH',
        url: `/api/sessions/${session.id}/protocol`,
        payload: { ...completeProtocol, expected_version: 1, outcome: 'right outcome' },
      }),
    ]);

    expect([left.statusCode, right.statusCode].sort()).toEqual([200, 409]);
    const latest = (await app.inject({ method: 'GET', url: `/api/sessions/${session.id}` })).json().research_protocol;
    expect(latest.version).toBe(2);
    expect(['left outcome', 'right outcome']).toContain(latest.outcome);
  });

  it('increments the CAS version on approval-state changes and rejects stale resurrection', async () => {
    const session = await createSession();
    await app.inject({ method: 'PATCH', url: `/api/sessions/${session.id}/protocol`, payload: completeProtocol });

    const revoked = await app.inject({
      method: 'PATCH',
      url: `/api/sessions/${session.id}/protocol`,
      payload: { ...completeProtocol, status: 'Draft', expected_version: 1 },
    });
    expect(revoked.statusCode).toBe(200);
    expect(revoked.json().research_protocol).toMatchObject({ status: 'Draft', version: 2, approval_id: null });

    const staleReapproval = await app.inject({
      method: 'PATCH',
      url: `/api/sessions/${session.id}/protocol`,
      payload: { ...completeProtocol, expected_version: 1 },
    });
    expect(staleReapproval.statusCode).toBe(409);
    expect(staleReapproval.json()).toMatchObject({ error_code: 'ResearchVersionConflict' });
    expect((await app.inject({ method: 'GET', url: `/api/sessions/${session.id}` })).json().research_protocol).toMatchObject({
      status: 'Draft',
      version: 2,
      approval_id: null,
    });
  });
});
