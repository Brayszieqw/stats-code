import { createHash } from 'node:crypto';
import { mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  buildRouter,
  createFileSessionStore,
  createResearchWorkflowService,
  SkillRegistry,
  SkillRunner,
  type AppState,
  type DatasetStore,
  type DatasetSummary,
  type FileSessionIntegrityWarning,
} from '@stats-code/server';

const CSV = 'participant_id,y,x\nP001,1,1\nP002,2,2\nP003,3,3.1\n';
const DATASET_ID = '33333333-3333-4333-8333-333333333333';
const RUN_SPEC = {
  skill_id: 'model_linear',
  dataset_id: DATASET_ID,
  args: { outcome: 'y', predictors: ['x'] },
};
const PROTOCOL = {
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

function summary(): DatasetSummary {
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
    uploaded_at: '2026-07-14T00:00:00.000Z',
    sha256: createHash('sha256').update(bytes).digest('hex'),
  };
}

function state(filePath: string, warnings: FileSessionIntegrityWarning[] = []) {
  const bytes = new TextEncoder().encode(CSV);
  const sessionStore = createFileSessionStore({
    filePath,
    now: () => new Date('2026-07-14T00:00:00.000Z'),
    onIntegrityWarning: (warning) => warnings.push(warning),
  });
  const datasetStore: DatasetStore = {
    saveAndParse: async () => summary(),
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
    now: () => new Date('2026-07-14T00:00:00.000Z'),
  });
  const appState: AppState = {
    sessionStore,
    datasetStore,
    skillRegistry: registry,
    skillRunner: runner,
    researchWorkflow,
  };
  return { appState, sessionStore, runnerSpy };
}

describe('production FileSessionStore router path', () => {
  let dir: string;
  let filePath: string;

  beforeEach(() => {
    dir = mkdtempSync(join(tmpdir(), 'stats-file-router-'));
    filePath = join(dir, 'sessions.json');
  });

  afterEach(() => {
    rmSync(dir, { recursive: true, force: true });
  });

  it('fails closed after persisted approval metadata is tampered and surfaces a quarantine warning', async () => {
    const initial = state(filePath);
    const firstApp = buildRouter({ state: initial.appState });
    const session = (await firstApp.inject({ method: 'POST', url: '/api/sessions' })).json();
    await initial.sessionStore.appendDataset(session.id, summary());

    const protocol = await firstApp.inject({
      method: 'PATCH',
      url: `/api/sessions/${session.id}/protocol`,
      payload: PROTOCOL,
    });
    expect(protocol.statusCode).toBe(200);

    const audit = await firstApp.inject({
      method: 'POST',
      url: `/api/sessions/${session.id}/datasets/${DATASET_ID}/audit`,
      payload: {
        skill_id: RUN_SPEC.skill_id,
        args: RUN_SPEC.args,
        expected_protocol_version: 1,
      },
    });
    expect(audit.statusCode).toBe(200);

    const approval = await firstApp.inject({
      method: 'POST',
      url: `/api/sessions/${session.id}/analysis-plans/approve`,
      payload: {
        ...RUN_SPEC,
        expected_protocol_version: 1,
        expected_audit_id: audit.json().audit_id,
        expected_audit_sha256: audit.json().audit_sha256,
        audit_roles: audit.json().roles,
      },
    });
    expect(approval.statusCode).toBe(201);
    await firstApp.close();

    const persisted = JSON.parse(readFileSync(filePath, 'utf8')) as {
      sessions: Array<{ id: string; research_protocol: { approval_id: string | null } }>;
    };
    persisted.sessions.find((candidate) => candidate.id === session.id)!.research_protocol.approval_id =
      '99999999-9999-4999-8999-999999999999';
    const tamperedRaw = JSON.stringify(persisted);
    writeFileSync(filePath, tamperedRaw, 'utf8');

    const warnings: FileSessionIntegrityWarning[] = [];
    const reloaded = state(filePath, warnings);
    const app = buildRouter({ state: reloaded.appState });
    const loaded = await app.inject({ method: 'GET', url: `/api/sessions/${session.id}` });
    expect(loaded.statusCode).toBe(200);
    expect(loaded.json()).toMatchObject({
      research_protocol: { status: 'Draft', approval_id: null },
      analysis_plan_approvals: [],
      integrity_warnings: expect.arrayContaining([
        expect.objectContaining({ record_type: 'research_protocol', reason: 'state_hash_mismatch' }),
        expect.objectContaining({ record_type: 'analysis_plan_approval', reason: 'protocol_not_approved' }),
      ]),
    });
    expect(warnings).toHaveLength(2);

    const run = await app.inject({
      method: 'POST',
      url: `/api/sessions/${session.id}/run`,
      payload: { ...RUN_SPEC, plan_id: approval.json().plan_id },
    });
    expect(run.statusCode).toBe(428);
    expect(run.json().error_code).toBe('ResearchProtocolRequired');
    expect(reloaded.runnerSpy).not.toHaveBeenCalled();

    const quarantineFiles = readdirSync(dir).filter((name) => name.startsWith('sessions.json.quarantine-'));
    expect(quarantineFiles).toHaveLength(1);
    expect(readFileSync(join(dir, quarantineFiles[0]!), 'utf8')).toBe(tamperedRaw);
    await app.close();
  });
});
