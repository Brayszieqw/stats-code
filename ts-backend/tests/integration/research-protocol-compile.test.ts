import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  buildRouter,
  MemSessionStore,
  type ProtocolCompileResult,
} from '@stats-code/server';

const RESULT: ProtocolCompileResult = {
  schema_version: '1.0',
  compiler_version: '1.0.0',
  proposal: {
    research_question: '吸烟与疾病结局是否相关？',
    study_design: 'cohort',
    population: '成人队列',
    eligibility_criteria: '',
    exposure: '吸烟',
    comparator: '未吸烟',
    outcome: '疾病结局',
    time_zero: '基线',
    follow_up: '一年',
    analysis_unit: '参与者',
    estimand: '调整后风险比',
    confounders: '年龄、性别',
    missing_data_strategy: '报告缺失率',
    primary_analysis: '多变量回归',
    sensitivity_analysis: '',
  },
  missing_required_fields: [],
  warnings: [],
  brief_sha256: 'a'.repeat(64),
  approval_required: true,
};

describe('research protocol compile route', () => {
  const sessionStore = new MemSessionStore();
  const compile = vi.fn(async (): Promise<ProtocolCompileResult> => RESULT);
  let app: ReturnType<typeof buildRouter>;

  beforeEach(() => {
    compile.mockClear();
    app = buildRouter({ state: { sessionStore, protocolCompiler: { compile } } });
  });

  afterEach(async () => {
    await app.close();
  });

  it('returns a proposal without persisting or approving it', async () => {
    const session = await sessionStore.create();
    const brief = '请研究成人队列中吸烟与一年疾病结局的关联，并先生成研究协议草稿。';
    const response = await app.inject({
      method: 'POST',
      url: `/api/sessions/${session.id}/protocol/compile`,
      payload: { brief },
    });

    expect(response.statusCode).toBe(200);
    expect(response.json()).toEqual(RESULT);
    expect(compile).toHaveBeenCalledWith({ brief });
    expect((await sessionStore.get(session.id)).research_protocol).toBeNull();
  });

  it('rejects short/extra-field requests before calling the compiler', async () => {
    const session = await sessionStore.create();
    const response = await app.inject({
      method: 'POST',
      url: `/api/sessions/${session.id}/protocol/compile`,
      payload: { brief: '太短', approved_at: '2099-01-01T00:00:00Z' },
    });

    expect(response.statusCode).toBe(422);
    expect(response.json()).toMatchObject({ error_code: 'SkillInvalidArgs' });
    expect(compile).not.toHaveBeenCalled();
  });

  it('blocks archived sessions and never invokes the compiler', async () => {
    const session = await sessionStore.create();
    session.status = 'Archived';
    const response = await app.inject({
      method: 'POST',
      url: `/api/sessions/${session.id}/protocol/compile`,
      payload: { brief: '请研究成人队列中吸烟与一年疾病结局的关联，并先生成研究协议草稿。' },
    });

    expect(response.statusCode).toBe(409);
    expect(response.json()).toMatchObject({ error_code: 'SessionArchived' });
    expect(compile).not.toHaveBeenCalled();
  });

  it('returns a safe LLM error when the compiler is not configured', async () => {
    const isolated = buildRouter({ state: { sessionStore: new MemSessionStore() } });
    try {
      const created = await isolated.inject({ method: 'POST', url: '/api/sessions' });
      const response = await isolated.inject({
        method: 'POST',
        url: `/api/sessions/${created.json().id}/protocol/compile`,
        payload: { brief: '请研究成人队列中吸烟与一年疾病结局的关联，并先生成研究协议草稿。' },
      });
      expect(response.statusCode).toBe(502);
      expect(response.json()).toMatchObject({ error_code: 'LlmUnavailable' });
    } finally {
      await isolated.close();
    }
  });
});
