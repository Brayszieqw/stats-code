import { describe, it, expect } from 'vitest';
import { contract } from '@stats-code/server';

const { ROUTE_CONTRACTS, allRouteJsonSchemas, HTTP_STATUS_FOR, domain, sidecar } = contract;

describe('route contracts', () => {
  it('declares the 13 API_Contract routes plus the dual-mode additions', () => {
    expect(ROUTE_CONTRACTS).toHaveLength(20);
    const ids = ROUTE_CONTRACTS.map((r) => r.id).sort();
    expect(ids).toEqual(
      [
        'create_session',
        'approve_analysis_plan',
        'audit_dataset',
        'compile_research_protocol',
        'delete_session',
        'get_coverage_matrix',
        'get_dataset',
        'get_llm_status',
        'get_session',
        'health',
        'list_sessions',
        'patch_settings',
        'patch_research_protocol',
        'post_audio',
        'post_dataset',
        'post_llm_config',
        'post_message',
        'post_sidecar',
        'post_snapshot_export',
        'run_skill',
      ].sort(),
    );
  });

  it('preserves per-route body limits from the Rust router', () => {
    const audio = ROUTE_CONTRACTS.find((r) => r.id === 'post_audio');
    const dataset = ROUTE_CONTRACTS.find((r) => r.id === 'post_dataset');
    expect(audio?.bodyLimitBytes).toBe(10 * 1024 * 1024);
    expect(dataset?.bodyLimitBytes).toBe(70 * 1024 * 1024);
  });

  it('preserves the Rust success status codes (201 for creates)', () => {
    expect(ROUTE_CONTRACTS.find((r) => r.id === 'create_session')?.successStatus).toBe(201);
    expect(ROUTE_CONTRACTS.find((r) => r.id === 'post_dataset')?.successStatus).toBe(201);
    expect(ROUTE_CONTRACTS.find((r) => r.id === 'get_session')?.successStatus).toBe(200);
  });

  it('generates JSON Schema for every route with a body or response', () => {
    const schemas = allRouteJsonSchemas();
    expect(Object.keys(schemas)).toHaveLength(20);
    // health has a response schema
    expect(schemas['health']?.response).toBeDefined();
    // patch_settings has both a request and a response
    expect(schemas['patch_settings']?.body).toBeDefined();
    expect(schemas['patch_settings']?.response).toBeDefined();
    expect(schemas['patch_research_protocol']?.body).toBeDefined();
    expect(schemas['patch_research_protocol']?.response).toBeDefined();
    expect(schemas['compile_research_protocol']?.body).toBeDefined();
    expect(schemas['compile_research_protocol']?.response).toBeDefined();
    expect(schemas['audit_dataset']?.body).toBeDefined();
    expect(schemas['approve_analysis_plan']?.body).toBeDefined();
    expect(schemas['approve_analysis_plan']?.response).toBeDefined();
  });

  it('accepts contract-shaped audit and approval requests', () => {
    const auditRequest = {
      skill_id: 'model_linear',
      args: { outcome: 'y', predictors: ['x'] },
      expected_protocol_version: 1,
      audit_roles: {},
    };
    const approvalRequest = {
      ...auditRequest,
      dataset_id: '33333333-3333-4333-8333-333333333333',
      expected_audit_id: '22222222-2222-4222-8222-222222222222',
      expected_audit_sha256: 'a'.repeat(64),
    };

    expect(domain.datasetAuditRequest.safeParse(auditRequest).success).toBe(true);
    expect(domain.analysisPlanApprovalRequest.safeParse(approvalRequest).success).toBe(true);
    expect(domain.analysisPlanApprovalRequest.safeParse({
      ...approvalRequest,
      approved_at: '2099-01-01T00:00:00.000Z',
    }).success).toBe(false);
  });
});

describe('error-code → status mapping', () => {
  it('matches the Rust http_status_for table', () => {
    expect(HTTP_STATUS_FOR.SessionNotFound).toBe(404);
    expect(HTTP_STATUS_FOR.SessionArchived).toBe(409);
    expect(HTTP_STATUS_FOR.DatasetTooLarge).toBe(413);
    expect(HTTP_STATUS_FOR.DatasetEmpty).toBe(422);
    expect(HTTP_STATUS_FOR.SkillTimeout).toBe(504);
    expect(HTTP_STATUS_FOR.SkillOom).toBe(507);
    expect(HTTP_STATUS_FOR.LlmUnavailable).toBe(502);
    expect(HTTP_STATUS_FOR.ResearchProtocolRequired).toBe(428);
    expect(HTTP_STATUS_FOR.ResearchApprovalRequired).toBe(428);
    expect(HTTP_STATUS_FOR.ResearchAuditBlocked).toBe(409);
    expect(HTTP_STATUS_FOR.ResearchVersionConflict).toBe(409);
    expect(Object.keys(HTTP_STATUS_FOR)).toHaveLength(18);
  });
});

describe('domain schema validation', () => {
  it('accepts a well-formed Session and the externally-tagged Message enum', () => {
    const sample = {
      id: '00000000-0000-4000-8000-000000000000',
      status: 'Active',
      created_at: '2025-01-01T00:00:00Z',
      last_active_at: '2025-01-01T00:00:00Z',
      settings: { decision_assistant: true },
      messages: [{ User: { id: '00000000-0000-4000-8000-000000000001', created_at: '2025-01-01T00:00:00Z', content: { Text: 'hi' } } }],
      datasets: [],
      skill_runs: [],
      uploaded_bytes: 0,
    };
    expect(domain.session.safeParse(sample).success).toBe(true);
  });

  it('rejects an unknown session status', () => {
    const bad = { status: 'Frozen' };
    expect(domain.sessionStatus.safeParse(bad.status).success).toBe(false);
  });

  it('LlmProvider serializes to lowercase tokens', () => {
    expect(domain.llmProvider.safeParse('deepseek').success).toBe(true);
    expect(domain.llmProvider.safeParse('openai').success).toBe(true);
    expect(domain.llmProvider.safeParse('DeepSeek').success).toBe(false);
  });
});

describe('sidecar/coverage schema validation', () => {
  it('accepts a coverage matrix keyed by the four software tokens', () => {
    const matrix = {
      schema_version: 1,
      release_version: '0.5.0',
      algorithms: [
        {
          id: 'tableone',
          display_name: 'Table One',
          iterative: false,
          ui_runnable: true,
          coverage: { R: 'live', SAS: 'recorded', Python: 'live', SPSS: 'none' },
          reference: { R: { callable: 'tableone::CreateTableOne', package: 'tableone', version: '0.13.2' } },
        },
      ],
    };
    expect(sidecar.coverageMatrix.safeParse(matrix).success).toBe(true);
  });

  it('rejects an unknown coverage token', () => {
    expect(sidecar.coverageValue.safeParse('partial').success).toBe(false);
  });

  it('a none-state sidecar snippet may omit text', () => {
    const snip = {
      algorithm_id: 'tableone',
      software: 'SPSS',
      coverage_value: 'none',
      sha256_of_dataset: '0'.repeat(64),
      release_version: '0.5.0',
    };
    expect(sidecar.sidecarSnippet.safeParse(snip).success).toBe(true);
  });
});
