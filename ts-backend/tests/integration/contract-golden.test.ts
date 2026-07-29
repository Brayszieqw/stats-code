// tests/integration/contract-golden.test.ts — contract/golden diff (task 3.8).
//
// Diffs serialized request/response samples that mirror the running Rust
// backend against the TS backend for every non-SSE route, and asserts the same
// status codes. The golden fixtures encode the Rust serde wire contract (field
// names, enum tokens, status codes). Each live TS response is validated against
// (a) its zod contract schema and (b) the golden fixture shape, so a drift in
// either the schema or the handler surfaces here.
//
// _Requirements: 1.2, 1.3, 1.4_

import { describe, it, expect } from 'vitest';
import {
  buildRouter,
  MemSessionStore,
  contract,
  type AppState,
  type AnalysisPlanApproval,
  type DatasetAudit,
  type ResearchWorkflowService,
  type SnapshotProvider,
  type CoverageMatrixProvider,
} from '@stats-code/server';
import { coverage } from '@stats-code/engine';

const { ROUTE_CONTRACTS, domain } = contract;

function makeState(overrides: Partial<AppState> = {}): AppState {
  return { sessionStore: new MemSessionStore(), ...overrides };
}

// A coverage provider backed by the real loaded matrix.
const coverageProvider: CoverageMatrixProvider = { get: () => coverage.getLoadedMatrix() };

const snapshotProvider: SnapshotProvider = {
  export() {
    return { snapshot_path: 'out.zip', sha256: '0'.repeat(64) };
  },
};

const auditGolden: DatasetAudit = {
  schema_version: '1.0',
  audit_rules_version: '1.2.0',
  audit_id: '22222222-2222-4222-8222-222222222222',
  dataset_id: '33333333-3333-4333-8333-333333333333',
  dataset_sha256: 'a'.repeat(64),
  protocol_version: 1,
  skill_id: 'model_linear',
  run_spec_sha256: 'b'.repeat(64),
  roles: {},
  status: 'passed',
  findings: [],
  audit_sha256: 'c'.repeat(64),
  created_at: '2026-07-14T00:00:00.000Z',
};

const approvalGolden: AnalysisPlanApproval = {
  schema_version: '1.0',
  plan_id: '44444444-4444-4444-8444-444444444444',
  approval_id: '55555555-5555-4555-8555-555555555555',
  status: 'Approved',
  protocol_version: 1,
  protocol_sha256: 'd'.repeat(64),
  protocol_approval_id: '66666666-6666-4666-8666-666666666666',
  dataset_id: auditGolden.dataset_id,
  dataset_sha256: auditGolden.dataset_sha256,
  skill_id: 'model_linear',
  args: { outcome: 'y', predictors: ['x'] },
  run_spec_sha256: auditGolden.run_spec_sha256,
  audit_id: auditGolden.audit_id,
  audit_sha256: auditGolden.audit_sha256,
  audit_roles: auditGolden.roles,
  approved_at: '2026-07-14T00:00:01.000Z',
};

const researchWorkflow: ResearchWorkflowService = {
  now: () => new Date('2026-07-14T00:00:00.000Z'),
  auditDataset: async () => auditGolden,
  approveAnalysisPlan: async () => approvalGolden,
  execute: async () => ({ analysis: { run_id: 'run-1' } }),
};

describe('every route is registered in the contract harness', () => {
  it('exposes the 13 original API_Contract routes plus the dual-mode additions', () => {
    // 13 original routes + dual-mode/session/protocol + server research gates
    // + the LLM provider-cache activation endpoint.
    expect(ROUTE_CONTRACTS).toHaveLength(21);
    const ids = ROUTE_CONTRACTS.map((r) => r.id);
    expect(ids).toContain('list_sessions');
    expect(ids).toContain('run_skill');
    expect(ids).toContain('delete_session');
    expect(ids).toContain('patch_research_protocol');
    expect(ids).toContain('compile_research_protocol');
    expect(ids).toContain('audit_dataset');
    expect(ids).toContain('approve_analysis_plan');
    expect(ids).toContain('post_llm_config_activate');
  });

  it('each route id is unique', () => {
    const ids = ROUTE_CONTRACTS.map((r) => r.id);
    expect(new Set(ids).size).toBe(ids.length);
  });
});

describe('GET /api/health golden', () => {
  it('matches the Rust { "status": "ok" } body and 200 status', async () => {
    const app = buildRouter({ state: makeState() });
    const res = await app.inject({ method: 'GET', url: '/api/health' });
    expect(res.statusCode).toBe(200);
    const golden = { status: 'ok' };
    expect(res.json()).toEqual(golden);
    expect(contract.healthResponse.safeParse(res.json()).success).toBe(true);
    await app.close();
  });
});

describe('session lifecycle golden', () => {
  it('POST /api/sessions → 201 with a serde-shaped Session', async () => {
    const app = buildRouter({ state: makeState() });
    const res = await app.inject({ method: 'POST', url: '/api/sessions' });
    expect(res.statusCode).toBe(201);
    const body = res.json();
    // Golden serde field set + enum tokens.
    expect(Object.keys(body).sort()).toEqual(
      [
        'created_at',
        'analysis_plan_approvals',
        'dataset_audits',
        'datasets',
        'id',
        'last_active_at',
        'messages',
        'research_protocol',
        'settings',
        'skill_runs',
        'status',
        'uploaded_bytes',
      ].sort(),
    );
    expect(body.status).toBe('Active');
    expect(body.settings).toEqual({ decision_assistant: true });
    expect(body.research_protocol).toBeNull();
    // Validates against the zod contract schema (the runtime harness).
    expect(domain.session.safeParse(body).success).toBe(true);
    await app.close();
  });

  it('GET unknown session → 404 with serde ErrorPayload + correct error_code', async () => {
    const app = buildRouter({ state: makeState() });
    const res = await app.inject({
      method: 'GET',
      url: '/api/sessions/00000000-0000-4000-8000-000000000000',
    });
    expect(res.statusCode).toBe(404);
    const body = res.json();
    expect(body.error_code).toBe('SessionNotFound');
    expect(domain.errorPayload.safeParse(body).success).toBe(true);
    // Status matches the single-source ErrorCode → HTTP map.
    expect(domain.HTTP_STATUS_FOR.SessionNotFound).toBe(404);
    await app.close();
  });

  it('PATCH settings → 200 with updated Session', async () => {
    const app = buildRouter({ state: makeState() });
    const created = (await app.inject({ method: 'POST', url: '/api/sessions' })).json();
    const res = await app.inject({
      method: 'PATCH',
      url: `/api/sessions/${created.id}/settings`,
      payload: { decision_assistant: false },
    });
    expect(res.statusCode).toBe(200);
    expect(res.json().settings.decision_assistant).toBe(false);
    expect(domain.session.safeParse(res.json()).success).toBe(true);
    await app.close();
  });
});

describe('research workflow gate golden', () => {
  it('audit and approval routes preserve their contract response shapes and success statuses', async () => {
    const app = buildRouter({ state: makeState({ researchWorkflow }) });
    const audit = await app.inject({
      method: 'POST',
      url: `/api/sessions/session-1/datasets/${auditGolden.dataset_id}/audit`,
      payload: {
        skill_id: 'model_linear',
        args: { outcome: 'y', predictors: ['x'] },
        expected_protocol_version: 1,
        audit_roles: {},
      },
    });
    expect(audit.statusCode).toBe(200);
    expect(audit.json()).toEqual(auditGolden);
    expect(domain.datasetAudit.safeParse(audit.json()).success).toBe(true);

    const approval = await app.inject({
      method: 'POST',
      url: '/api/sessions/session-1/analysis-plans/approve',
      payload: {
        skill_id: 'model_linear',
        dataset_id: auditGolden.dataset_id,
        args: { outcome: 'y', predictors: ['x'] },
        expected_protocol_version: 1,
        expected_audit_id: auditGolden.audit_id,
        expected_audit_sha256: auditGolden.audit_sha256,
        audit_roles: {},
      },
    });
    expect(approval.statusCode).toBe(201);
    expect(approval.json()).toEqual(approvalGolden);
    expect(domain.analysisPlanApproval.safeParse(approval.json()).success).toBe(true);
    await app.close();
  });

  it('returns schema 422 before session 404 for all three gate routes', async () => {
    const app = buildRouter({ state: makeState({ researchWorkflow }) });
    const unknown = '00000000-0000-4000-8000-000000000000';
    const responses = await Promise.all([
      app.inject({
        method: 'POST',
        url: `/api/sessions/${unknown}/datasets/${auditGolden.dataset_id}/audit`,
        payload: {},
      }),
      app.inject({
        method: 'POST',
        url: `/api/sessions/${unknown}/analysis-plans/approve`,
        payload: {},
      }),
      app.inject({
        method: 'POST',
        url: `/api/sessions/${unknown}/run`,
        payload: {},
      }),
    ]);

    expect(responses.map((response) => response.statusCode)).toEqual([422, 422, 422]);
    expect(responses.map((response) => response.json().error_code)).toEqual([
      'SkillInvalidArgs',
      'SkillInvalidArgs',
      'SkillInvalidArgs',
    ]);
    await app.close();
  });
});

describe('llm-status golden', () => {
  it('unconfigured → { configured:false, provider:null, base_url:null, model:null }', async () => {
    const app = buildRouter({ state: makeState() });
    const res = await app.inject({ method: 'GET', url: '/api/llm-status' });
    expect(res.statusCode).toBe(200);
    const golden = { configured: false, provider: null, base_url: null, model: null, cached_providers: [] };
    expect(res.json()).toEqual(golden);
    expect(contract.llmStatusResponse.safeParse(res.json()).success).toBe(true);
    await app.close();
  });

  it('configured → exposes provider but never the api key', async () => {
    const app = buildRouter({
      state: makeState({
        llmConfigStore: {
          read: () => ({ provider: 'deepseek', api_key: 'sk-secret', base_url: null, model: null }),
          write: () => {},
          listCached: () => ['deepseek'],
          readProvider: () => null,
        },
      }),
    });
    const res = await app.inject({ method: 'GET', url: '/api/llm-status' });
    const body = res.json();
    expect(body).toEqual({
      configured: true,
      provider: 'deepseek',
      base_url: null,
      model: null,
      cached_providers: ['deepseek'],
    });
    expect(JSON.stringify(body)).not.toContain('sk-secret');
    await app.close();
  });
});

describe('coverage-matrix golden', () => {
  // The wire DTO (crates/api/src/sidecar.rs) uses callable/package/version per
  // reference cell. 13.7 maps the engine's internal matrix into this shape; the
  // golden here pins the wire contract a contract-shaped provider must satisfy.
  const goldenMatrix = {
    schema_version: 1,
    release_version: '0.5.0',
    algorithms: [
      {
        id: 'tableone',
        display_name: 'Table One',
        iterative: false,
        ui_runnable: true,
        coverage: { R: 'live', SAS: 'recorded', Python: 'sidecar_only', SPSS: 'none' },
        reference: {
          R: { callable: 'tableone', package: 'tableone', version: '0.13.2' },
          SAS: { callable: 'proc means', package: null, version: '9.4' },
          Python: { callable: 'tableone', package: 'tableone', version: '0.9.1' },
          SPSS: { callable: 'DESCRIPTIVES', package: null, version: '29' },
        },
      },
    ],
  };

  it('200 returns the verbatim provider matrix matching the wire contract schema', async () => {
    const provider: CoverageMatrixProvider = { get: () => goldenMatrix as never };
    const app = buildRouter({ state: makeState({ coverageMatrixProvider: provider }) });
    const res = await app.inject({ method: 'GET', url: '/api/coverage-matrix' });
    expect(res.statusCode).toBe(200);
    const body = res.json();
    expect(body).toEqual(goldenMatrix);
    expect(contract.sidecar.coverageMatrix.safeParse(body).success).toBe(true);
    await app.close();
  });

  it('the engine loaded matrix is structurally complete (4 softwares per cell)', () => {
    const matrix = coverageProvider.get();
    expect(matrix.algorithms.length).toBeGreaterThan(0);
    for (const entry of matrix.algorithms) {
      expect(Object.keys(entry.coverage).sort()).toEqual(['Python', 'R', 'SAS', 'SPSS']);
      expect(Object.keys(entry.reference).sort()).toEqual(['Python', 'R', 'SAS', 'SPSS']);
    }
  });
});

describe('snapshot/export golden', () => {
  it('200 returns { snapshot_path, sha256 } matching the contract schema', async () => {
    const app = buildRouter({ state: makeState({ snapshotProvider }) });
    const res = await app.inject({
      method: 'POST',
      url: '/api/snapshot/export',
      payload: { run_id: 'r', destination: 'out.zip' },
    });
    expect(res.statusCode).toBe(200);
    const body = res.json();
    expect(Object.keys(body).sort()).toEqual(['sha256', 'snapshot_path']);
    expect(contract.sidecar.snapshotExportResponse.safeParse(body).success).toBe(true);
    await app.close();
  });
});
