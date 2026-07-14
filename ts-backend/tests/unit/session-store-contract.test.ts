import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import {
  createFileSessionStore,
  MemSessionStore,
  protocolContentSha256,
  protocolStateSha256,
  runSpecSha256,
  type AnalysisPlanApproval,
  type ResearchProtocol,
  type SessionStore,
} from '@stats-code/server';

const PROTOCOL_APPROVAL_ID = '11111111-1111-4111-8111-111111111111';
const DATASET_ID = '33333333-3333-4333-8333-333333333333';
const ARGS = { outcome: 'y', predictors: ['x'] };

function protocol(status: 'Draft' | 'Approved'): ResearchProtocol {
  const fields = {
    status,
    research_question: 'x 与 y 是否相关？',
    study_design: 'cross_sectional' as const,
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
    version: 1,
    content_sha256: '',
    approval_id: status === 'Approved' ? PROTOCOL_APPROVAL_ID : null,
    approved_at: status === 'Approved' ? '2026-07-14T00:00:00.000Z' : null,
    updated_at: '2026-07-14T00:00:00.000Z',
  };
  const withContentHash = { ...fields, content_sha256: protocolContentSha256(fields) };
  return { ...withContentHash, state_sha256: protocolStateSha256(withContentHash) };
}

function approval(boundProtocol: ResearchProtocol): AnalysisPlanApproval {
  return {
    schema_version: '1.0',
    plan_id: '44444444-4444-4444-8444-444444444444',
    approval_id: '55555555-5555-4555-8555-555555555555',
    status: 'Approved',
    protocol_version: boundProtocol.version,
    protocol_sha256: boundProtocol.content_sha256,
    protocol_approval_id: boundProtocol.approval_id!,
    dataset_id: DATASET_ID,
    dataset_sha256: 'a'.repeat(64),
    skill_id: 'model_linear',
    args: ARGS,
    run_spec_sha256: runSpecSha256('model_linear', DATASET_ID, ARGS),
    audit_id: '66666666-6666-4666-8666-666666666666',
    audit_sha256: 'b'.repeat(64),
    audit_roles: {},
    approved_at: '2026-07-14T00:00:00.000Z',
  };
}

interface StoreHarness {
  store: SessionStore;
  archive(sessionId: string): Promise<SessionStore>;
  cleanup(): void;
}

function memoryHarness(): StoreHarness {
  const store = new MemSessionStore();
  return {
    store,
    async archive(sessionId) {
      (await store.get(sessionId)).status = 'Archived';
      return store;
    },
    cleanup() {},
  };
}

function fileHarness(): StoreHarness {
  const dir = mkdtempSync(join(tmpdir(), 'stats-session-contract-'));
  const filePath = join(dir, 'sessions.json');
  const store = createFileSessionStore({ filePath });
  return {
    store,
    async archive(sessionId) {
      const persisted = JSON.parse(readFileSync(filePath, 'utf8')) as {
        sessions: Array<{ id: string; status: 'Active' | 'Archived' }>;
      };
      persisted.sessions.find((session) => session.id === sessionId)!.status = 'Archived';
      writeFileSync(filePath, JSON.stringify(persisted), 'utf8');
      return createFileSessionStore({ filePath });
    },
    cleanup() {
      rmSync(dir, { recursive: true, force: true });
    },
  };
}

describe.each([
  ['MemSessionStore', memoryHarness],
  ['FileSessionStore', fileHarness],
] as const)('%s approval append contract', (_name, createHarness) => {
  let harness: StoreHarness;

  beforeEach(() => {
    harness = createHarness();
  });

  afterEach(() => {
    harness.cleanup();
  });

  it('accepts an approval only when it matches the current approved protocol', async () => {
    const session = await harness.store.create();
    const approved = protocol('Approved');
    expect(await harness.store.updateResearchProtocol(session.id, approved)).toBe(true);

    expect(await harness.store.appendAnalysisPlanApproval(session.id, approval(approved))).toBe(true);
    expect((await harness.store.get(session.id)).analysis_plan_approvals).toHaveLength(1);
  });

  it('rejects absent, draft, and stale protocol bindings without appending', async () => {
    const session = await harness.store.create();
    const approved = protocol('Approved');
    const boundApproval = approval(approved);

    expect(await harness.store.appendAnalysisPlanApproval(session.id, boundApproval)).toBe(false);
    expect(await harness.store.updateResearchProtocol(session.id, protocol('Draft'))).toBe(true);
    expect(await harness.store.appendAnalysisPlanApproval(session.id, boundApproval)).toBe(false);
    expect(await harness.store.updateResearchProtocol(session.id, approved, 1)).toBe(true);

    const staleBindings: AnalysisPlanApproval[] = [
      { ...boundApproval, protocol_version: 2 },
      { ...boundApproval, protocol_sha256: 'f'.repeat(64) },
      { ...boundApproval, protocol_approval_id: '77777777-7777-4777-8777-777777777777' },
    ];
    for (const stale of staleBindings) {
      expect(await harness.store.appendAnalysisPlanApproval(session.id, stale)).toBe(false);
    }
    expect((await harness.store.get(session.id)).analysis_plan_approvals).toEqual([]);
  });

  it('rejects an otherwise matching approval after archival', async () => {
    const session = await harness.store.create();
    const approved = protocol('Approved');
    expect(await harness.store.updateResearchProtocol(session.id, approved)).toBe(true);

    const archivedStore = await harness.archive(session.id);
    expect(await archivedStore.appendAnalysisPlanApproval(session.id, approval(approved))).toBe(false);
    expect((await archivedStore.get(session.id)).analysis_plan_approvals).toEqual([]);
  });
});
