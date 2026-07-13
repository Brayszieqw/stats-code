import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { buildRouter, createFileSessionStore } from '@stats-code/server';

describe('file session store empty-shell reuse', () => {
  let dir: string;
  let filePath: string;

  beforeEach(() => {
    dir = mkdtempSync(join(tmpdir(), 'stats-sess-'));
    filePath = join(dir, 'sessions.json');
  });

  afterEach(() => {
    rmSync(dir, { recursive: true, force: true });
  });

  it('reuses the newest empty shell and purges extra empties', async () => {
    const store = createFileSessionStore({ filePath });
    const a = await store.create();
    // Force a second empty by temporarily marking a as non-empty then clearing is hard;
    // instead create, append a message, create again, then create empty after delete path.
    await store.appendMessages(a.id, [
      {
        User: {
          id: '11111111-1111-4111-8111-111111111111',
          created_at: new Date().toISOString(),
          content: { Text: 'hello' },
        },
      },
    ]);
    const empty1 = await store.create();
    expect(empty1.id).not.toBe(a.id);
    const empty2 = await store.create();
    // Second create on empty should reuse empty1
    expect(empty2.id).toBe(empty1.id);
    const list = await store.list();
    const empties = list.filter((s) => s.message_count === 0 && s.dataset_count === 0);
    expect(empties).toHaveLength(1);
    expect(list.some((s) => s.id === a.id)).toBe(true);
  });

  it('downgrades a legacy approved protocol to Draft and does not mint server approval trust', async () => {
    const store = createFileSessionStore({ filePath });
    const session = await store.create();
    const timestamp = new Date().toISOString();
    await store.updateResearchProtocol(session.id, {
      status: 'Approved',
      research_question: '暴露与结局是否相关？',
      study_design: 'cohort',
      population: '成人队列',
      eligibility_criteria: '有基线记录',
      exposure: 'exposure',
      comparator: '未暴露',
      outcome: 'outcome',
      time_zero: '基线',
      follow_up: '一年',
      analysis_unit: '参与者',
      estimand: '调整后风险比',
      confounders: 'age',
      missing_data_strategy: '完整案例',
      primary_analysis: '回归模型',
      sensitivity_analysis: '改变协变量集',
      approved_at: timestamp,
      updated_at: timestamp,
    });

    const next = await store.create();
    expect(next.id).not.toBe(session.id);

    const reloaded = createFileSessionStore({ filePath });
    expect((await reloaded.get(session.id)).research_protocol).toMatchObject({
      status: 'Draft',
      outcome: 'outcome',
      version: 1,
      approval_id: null,
      approved_at: null,
    });
  });

  it('downgrades persisted approval metadata when the server envelope hash no longer matches', async () => {
    const store = createFileSessionStore({ filePath });
    const app = buildRouter({ state: { sessionStore: store } });
    const session = (await app.inject({ method: 'POST', url: '/api/sessions' })).json();
    const saved = await app.inject({
      method: 'PATCH',
      url: `/api/sessions/${session.id}/protocol`,
      payload: {
        status: 'Draft',
        research_question: '暴露与结局是否相关？',
        study_design: 'cohort',
        population: '成人队列',
        eligibility_criteria: '有基线记录',
        exposure: 'exposure',
        comparator: '未暴露',
        outcome: 'outcome',
        time_zero: '基线',
        follow_up: '一年',
        analysis_unit: '参与者',
        estimand: '调整后风险比',
        confounders: 'age',
        missing_data_strategy: '完整案例',
        primary_analysis: '回归模型',
        sensitivity_analysis: '改变协变量集',
      },
    });
    expect(saved.statusCode).toBe(200);
    const originalStateHash = saved.json().research_protocol.state_sha256 as string;
    await app.close();

    const persisted = JSON.parse(readFileSync(filePath, 'utf8')) as {
      sessions: Array<{ id: string; research_protocol: Record<string, unknown> }>;
    };
    const target = persisted.sessions.find((candidate) => candidate.id === session.id)!;
    target.research_protocol.status = 'Approved';
    target.research_protocol.version = 999;
    target.research_protocol.approval_id = '11111111-1111-4111-8111-111111111111';
    target.research_protocol.approved_at = '2099-01-01T00:00:00.000Z';
    writeFileSync(filePath, JSON.stringify(persisted), 'utf8');

    const reloaded = createFileSessionStore({ filePath });
    const protocol = (await reloaded.get(session.id)).research_protocol!;
    expect(protocol).toMatchObject({ status: 'Draft', version: 999, approval_id: null, approved_at: null });
    expect(protocol.state_sha256).not.toBe(originalStateHash);
  });
});
