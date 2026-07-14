import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { createHash } from 'node:crypto';
import { mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import {
  buildRouter,
  createFileSessionStore,
  createResearchWorkflowService,
  SkillRegistry,
  SkillRunner,
  type DatasetStore,
  type DatasetSummary,
  type FileSessionIntegrityWarning,
} from '@stats-code/server';

const CSV = 'participant_id,y,x\nP001,1,1\nP002,2,2\nP003,3,3.1\n';
const DATASET_ID = '33333333-3333-4333-8333-333333333333';

function datasetSummary(): DatasetSummary {
  const bytes = new TextEncoder().encode(CSV);
  return {
    dataset_id: DATASET_ID,
    file_name: 'data.csv',
    size_bytes: bytes.byteLength,
    encoding: 'Utf8',
    row_count: 3,
    columns: [
      { name: 'participant_id', inferred_type: 'String', missing_count: 0 },
      { name: 'y', inferred_type: 'Numeric', missing_count: 0 },
      { name: 'x', inferred_type: 'Numeric', missing_count: 0 },
    ],
    uploaded_at: '2026-01-01T00:00:00Z',
    sha256: createHash('sha256').update(bytes).digest('hex'),
  };
}

const approvedProtocol = {
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

    const warnings: FileSessionIntegrityWarning[] = [];
    const reloaded = createFileSessionStore({ filePath, onIntegrityWarning: (warning) => warnings.push(warning) });
    expect((await reloaded.get(session.id)).research_protocol).toMatchObject({
      status: 'Draft',
      outcome: 'outcome',
      version: 1,
      approval_id: null,
      approved_at: null,
    });
    expect(warnings).toContainEqual({
      event: 'file_session_integrity_warning',
      action: 'downgraded',
      record_type: 'research_protocol',
      session_id: session.id,
      reason: 'legacy_approved_without_server_trust',
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
    const tamperedRaw = JSON.stringify(persisted);
    writeFileSync(filePath, tamperedRaw, 'utf8');

    const warnings: FileSessionIntegrityWarning[] = [];
    const reloaded = createFileSessionStore({ filePath, onIntegrityWarning: (warning) => warnings.push(warning) });
    const reloadedSession = await reloaded.get(session.id);
    const protocol = reloadedSession.research_protocol!;
    expect(protocol).toMatchObject({ status: 'Draft', version: 999, approval_id: null, approved_at: null });
    expect(protocol.state_sha256).not.toBe(originalStateHash);
    expect(warnings).toContainEqual(expect.objectContaining({
      action: 'downgraded',
      record_type: 'research_protocol',
      session_id: session.id,
      reason: 'state_hash_mismatch',
    }));
    expect(reloadedSession.integrity_warnings).toContainEqual(expect.objectContaining({
      action: 'downgraded',
      record_type: 'research_protocol',
      session_id: session.id,
      reason: 'state_hash_mismatch',
    }));

    const quarantineFiles = readdirSync(dir).filter((name) => name.startsWith('sessions.json.quarantine-'));
    expect(quarantineFiles).toHaveLength(1);
    const quarantinePath = join(dir, quarantineFiles[0]!);
    expect(readFileSync(quarantinePath, 'utf8')).toBe(tamperedRaw);

    const resilient = createFileSessionStore({
      filePath,
      onIntegrityWarning: () => { throw new Error('logging unavailable'); },
    });
    await expect(resilient.get(session.id)).resolves.toMatchObject({
      research_protocol: { status: 'Draft', approval_id: null },
    });

    await reloaded.updateSettings(session.id, { decision_assistant: false });
    expect(readFileSync(quarantinePath, 'utf8')).toBe(tamperedRaw);
    const sanitized = JSON.parse(readFileSync(filePath, 'utf8')) as {
      sessions: Array<{ id: string; research_protocol: { status: string }; integrity_warnings?: unknown[] }>;
    };
    expect(sanitized.sessions.find((candidate) => candidate.id === session.id)).toMatchObject({
      research_protocol: { status: 'Draft' },
      integrity_warnings: [expect.objectContaining({ reason: 'state_hash_mismatch' })],
    });
  });

  it('filters a stale persisted approval and reports the failed binding check', async () => {
    const bytes = new TextEncoder().encode(CSV);
    const sessionStore = createFileSessionStore({ filePath });
    const datasetStore: DatasetStore = {
      saveAndParse: async () => datasetSummary(),
      readRawById: async () => bytes,
    };
    const registry = SkillRegistry.withDefaults();
    const runner = new SkillRunner(registry);
    const researchWorkflow = createResearchWorkflowService({ sessionStore, datasetStore, registry, runner });
    const app = buildRouter({ state: { sessionStore, datasetStore, researchWorkflow } });
    const session = (await app.inject({ method: 'POST', url: '/api/sessions' })).json();
    await sessionStore.appendDataset(session.id, datasetSummary());
    await app.inject({ method: 'PATCH', url: `/api/sessions/${session.id}/protocol`, payload: approvedProtocol });
    const audit = await app.inject({
      method: 'POST',
      url: `/api/sessions/${session.id}/datasets/${DATASET_ID}/audit`,
      payload: {
        skill_id: 'model_linear',
        args: { outcome: 'y', predictors: ['x'] },
        expected_protocol_version: 1,
      },
    });
    expect(audit.statusCode).toBe(200);
    const approval = await app.inject({
      method: 'POST',
      url: `/api/sessions/${session.id}/analysis-plans/approve`,
      payload: {
        skill_id: 'model_linear',
        dataset_id: DATASET_ID,
        args: { outcome: 'y', predictors: ['x'] },
        expected_protocol_version: 1,
        expected_audit_id: audit.json().audit_id,
        expected_audit_sha256: audit.json().audit_sha256,
        audit_roles: audit.json().roles,
      },
    });
    expect(approval.statusCode).toBe(201);
    await app.close();

    const persisted = JSON.parse(readFileSync(filePath, 'utf8')) as {
      sessions: Array<{
        id: string;
        analysis_plan_approvals: Array<{ args: Record<string, unknown> }>;
      }>;
    };
    const target = persisted.sessions.find((candidate) => candidate.id === session.id)!;
    target.analysis_plan_approvals[0]!.args = { outcome: 'y', predictors: ['x'], alpha: 0.01 };
    writeFileSync(filePath, JSON.stringify(persisted), 'utf8');

    const warnings: FileSessionIntegrityWarning[] = [];
    const reloaded = createFileSessionStore({ filePath, onIntegrityWarning: (warning) => warnings.push(warning) });
    const reloadedSession = await reloaded.get(session.id);
    expect(reloadedSession.analysis_plan_approvals).toEqual([]);
    expect(warnings).toContainEqual({
      event: 'file_session_integrity_warning',
      action: 'discarded',
      record_type: 'analysis_plan_approval',
      session_id: session.id,
      reason: 'binding_mismatch',
    });
    expect(reloadedSession.integrity_warnings).toContainEqual(warnings[0]);
  });
});
