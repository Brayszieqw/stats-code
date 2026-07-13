import { afterEach, describe, expect, it } from 'vitest';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import {
  buildRouter,
  createFsDatasetStore,
  createSnapshotRunRegistry,
  createResearchWorkflowService,
  MemSessionStore,
  SkillRegistry,
  SkillRunner,
  type AppState,
} from '@stats-code/server';
import { snapshot } from '@stats-code/engine';

const tmpDirs: string[] = [];
afterEach(() => {
  for (const dir of tmpDirs.splice(0)) rmSync(dir, { recursive: true, force: true });
});

function freshDir(): string {
  const dir = mkdtempSync(join(tmpdir(), 'sc-research-snapshot-'));
  tmpDirs.push(dir);
  return dir;
}

const b64 = (text: string) => Buffer.from(text, 'utf8').toString('base64');

describe('research audit snapshot', () => {
  it('exports protocol + approval records and replay verifies their hashes', async () => {
    const root = freshDir();
    const datasetStore = createFsDatasetStore({ root: join(root, 'datasets') });
    const snapshotRuns = createSnapshotRunRegistry(datasetStore);
    const registry = SkillRegistry.withDefaults();
    const sessionStore = new MemSessionStore();
    const runner = new SkillRunner(registry);
    const researchWorkflow = createResearchWorkflowService({
      sessionStore,
      datasetStore,
      registry,
      runner,
      snapshotRunRecorder: snapshotRuns.recorder,
    });
    const state: AppState = {
      sessionStore,
      datasetStore,
      skillRegistry: registry,
      skillRunner: runner,
      researchWorkflow,
      snapshotProvider: snapshotRuns.provider,
      snapshotRunRecorder: snapshotRuns.recorder,
    };
    const app = buildRouter({ state });
    const sid = (await app.inject({ method: 'POST', url: '/api/sessions' })).json().id as string;
    const protocol = {
      status: 'Approved',
      research_question: '年龄是否与连续结局相关？',
      study_design: 'cross_sectional',
      population: '演示成人队列',
      eligibility_criteria: '有完整基线记录',
      exposure: 'x',
      comparator: '每增加 1 单位',
      outcome: 'y（连续结局）',
      time_zero: '基线',
      follow_up: '不适用',
      analysis_unit: '参与者',
      estimand: 'x 每增加 1 单位对应的平均 y 差',
      confounders: '',
      missing_data_strategy: '完整案例',
      primary_analysis: '多元线性回归',
      sensitivity_analysis: '',
    } as const;
    const savedProtocol = (await app.inject({
      method: 'PATCH',
      url: `/api/sessions/${sid}/protocol`,
      payload: protocol,
    })).json().research_protocol;

    const upload = await app.inject({
      method: 'POST',
      url: `/api/sessions/${sid}/datasets`,
      payload: { filename: 'cohort.csv', data: b64('participant_id,y,x\nP001,1,1\nP002,2,2\nP003,3,3\nP004,4,4\n') },
    });
    const datasetId = upload.json().dataset_id as string;
    const audit = await app.inject({
      method: 'POST',
      url: `/api/sessions/${sid}/datasets/${datasetId}/audit`,
      payload: {
        skill_id: 'model_linear',
        args: { outcome: 'y', predictors: ['x'] },
        expected_protocol_version: savedProtocol.version,
      },
    });
    expect(audit.statusCode).toBe(200);
    const auditResult = audit.json();
    const approval = await app.inject({
      method: 'POST',
      url: `/api/sessions/${sid}/analysis-plans/approve`,
      payload: {
        skill_id: 'model_linear',
        dataset_id: datasetId,
        args: { outcome: 'y', predictors: ['x'] },
        expected_protocol_version: savedProtocol.version,
        expected_audit_id: auditResult.audit_id,
        expected_audit_sha256: auditResult.audit_sha256,
        audit_roles: auditResult.roles,
      },
    });
    expect(approval.statusCode).toBe(201);
    const approvedPlan = approval.json();
    const run = await app.inject({
      method: 'POST',
      url: `/api/sessions/${sid}/run`,
      payload: {
        skill_id: 'model_linear',
        dataset_id: datasetId,
        args: { outcome: 'y', predictors: ['x'] },
        plan_id: approvedPlan.plan_id,
      },
    });
    expect(run.statusCode).toBe(200);
    const runId = run.json().analysis.run_id as string;

    const destination = join(root, 'audit.zip');
    const exported = await app.inject({
      method: 'POST',
      url: '/api/snapshot/export',
      payload: { run_id: runId, destination },
    });
    expect(exported.statusCode).toBe(200);

    const extractedDir = join(root, 'extracted');
    execFileSync('powershell', [
      '-NoProfile',
      '-Command',
      `Expand-Archive -LiteralPath '${destination}' -DestinationPath '${extractedDir}' -Force`,
    ]);
    expect(JSON.parse(readFileSync(join(extractedDir, 'protocol.json'), 'utf8'))).toMatchObject({
      status: 'Approved',
      outcome: 'y（连续结局）',
    });
    const approvalDocument = JSON.parse(readFileSync(join(extractedDir, 'approval.json'), 'utf8'));
    const auditDocument = JSON.parse(readFileSync(join(extractedDir, 'dataset-audit.json'), 'utf8'));
    expect(approvalDocument).toMatchObject({
      session_id: sid,
      run_id: runId,
      protocol_status: 'Approved',
      plan_id: approvedPlan.plan_id,
      plan_approved_at: approvedPlan.approved_at,
    });
    expect(approvalDocument.audit_id).toBe(auditDocument.audit_id);
    expect(approvalDocument.audit_sha256).toBe(auditDocument.audit_sha256);
    expect(readFileSync(join(extractedDir, 'workflow.yaml'), 'utf8')).toContain('path: protocol.json');
    expect(readFileSync(join(extractedDir, 'workflow.yaml'), 'utf8')).toContain('path: approval.json');
    expect(readFileSync(join(extractedDir, 'workflow.yaml'), 'utf8')).toContain('path: dataset-audit.json');
    expect(snapshot.executeReplay({ extractedDir, installedReferenceSoftware: [] })).toEqual({
      stepsReplayed: 1,
    });
    await app.close();
  });
});
