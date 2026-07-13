/**
 * Tests for the new dual-mode client methods listSessions / runSkill.
 *
 * Validates: Requirements 11.1, 12.1
 */

import { describe, it, expect, vi, afterEach } from 'vitest';
import {
  ApiError,
  approveAnalysisPlan,
  auditDataset,
  deleteSession,
  listSessions,
  postDataset,
  runSkill,
} from './client';
import type {
  AnalysisPlanApproval,
  DatasetAudit,
  DatasetSummary,
  SessionSummary,
  SkillResult,
} from './types';

function mockFetchOnce(status: number, body: unknown) {
  const fn = vi.fn().mockResolvedValue({
    ok: status >= 200 && status < 300,
    status,
    statusText: 'x',
    json: async () => body,
  } as Response);
  vi.stubGlobal('fetch', fn);
  return fn;
}

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('listSessions', () => {
  it('GETs /api/sessions and parses the summary array', async () => {
    const summaries: SessionSummary[] = [
      {
        id: 's1',
        status: 'Active',
        created_at: '2026-01-01T00:00:00.000Z',
        last_active_at: '2026-01-02T00:00:00.000Z',
        message_count: 3,
        title: '血压分析',
        dataset_count: 1,
      },
    ];
    const fetchFn = mockFetchOnce(200, summaries);
    const result = await listSessions();
    expect(fetchFn).toHaveBeenCalledWith('/api/sessions');
    expect(result).toEqual(summaries);
  });

  it('throws ApiError on non-2xx', async () => {
    mockFetchOnce(500, { error_code: 'SkillExecutionFailed', message: 'boom' });
    await expect(listSessions()).rejects.toBeInstanceOf(ApiError);
  });
});

describe('runSkill', () => {
  it('POSTs the run body to /api/sessions/:sid/run and parses SkillResult', async () => {
    const result: SkillResult = {
      schema_version: '1.0',
      payload: { r_squared: 0.9 },
      risk_signals: [],
    };
    const fetchFn = mockFetchOnce(200, result);
    const body = { skill_id: 'model_linear', dataset_id: 'ds-1', args: { outcome: 'y', predictors: ['x'] } };
    const out = await runSkill('sid-1', body);
    expect(fetchFn).toHaveBeenCalledWith(
      '/api/sessions/sid-1/run',
      expect.objectContaining({
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(body),
      }),
    );
    expect(out).toEqual(result);
  });

  it('throws ApiError on 422 invalid args', async () => {
    mockFetchOnce(422, { error_code: 'SkillInvalidArgs', message: 'missing' });
    await expect(
      runSkill('sid-1', { skill_id: 'model_linear', dataset_id: 'ds-1', args: {} }),
    ).rejects.toBeInstanceOf(ApiError);
  });
});

describe('research workflow gates', () => {
  const audit: DatasetAudit = {
    schema_version: '1.0',
    audit_rules_version: '1.1.0',
    audit_id: '22222222-2222-4222-8222-222222222222',
    dataset_id: 'ds-1',
    dataset_sha256: 'b'.repeat(64),
    protocol_version: 2,
    skill_id: 'model_linear',
    run_spec_sha256: 'c'.repeat(64),
    roles: { primary_key: ['participant_id'] },
    status: 'passed',
    findings: [],
    audit_sha256: 'd'.repeat(64),
    created_at: '2026-01-01T00:00:00Z',
  };

  it('requests a server dataset audit without client approval timestamps', async () => {
    const fetchFn = mockFetchOnce(200, audit);
    const body = {
      skill_id: 'model_linear',
      args: { outcome: 'y', predictors: ['x'] },
      expected_protocol_version: 2,
      audit_roles: { primary_key: ['participant_id'] },
    };

    await expect(auditDataset('sid-1', 'ds-1', body)).resolves.toEqual(audit);
    expect(fetchFn).toHaveBeenCalledWith(
      '/api/sessions/sid-1/datasets/ds-1/audit',
      expect.objectContaining({
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(body),
      }),
    );
    expect(JSON.parse(fetchFn.mock.calls[0]![1].body)).not.toHaveProperty('approved_at');
  });

  it('approves the exact audit the user reviewed', async () => {
    const approval: AnalysisPlanApproval = {
      schema_version: '1.0',
      plan_id: '33333333-3333-4333-8333-333333333333',
      approval_id: '44444444-4444-4444-8444-444444444444',
      status: 'Approved',
      protocol_version: 2,
      protocol_sha256: 'a'.repeat(64),
      protocol_approval_id: '11111111-1111-4111-8111-111111111111',
      dataset_id: 'ds-1',
      dataset_sha256: audit.dataset_sha256,
      skill_id: audit.skill_id,
      args: { outcome: 'y', predictors: ['x'] },
      run_spec_sha256: audit.run_spec_sha256,
      audit_id: audit.audit_id,
      audit_sha256: audit.audit_sha256,
      audit_roles: audit.roles,
      approved_at: '2026-01-01T00:00:01Z',
    };
    const fetchFn = mockFetchOnce(200, approval);
    const body = {
      dataset_id: 'ds-1',
      skill_id: audit.skill_id,
      args: approval.args,
      expected_protocol_version: 2,
      expected_audit_id: audit.audit_id,
      expected_audit_sha256: audit.audit_sha256,
      audit_roles: audit.roles,
    };

    await expect(approveAnalysisPlan('sid-1', body)).resolves.toEqual(approval);
    expect(fetchFn).toHaveBeenCalledWith(
      '/api/sessions/sid-1/analysis-plans/approve',
      expect.objectContaining({
        method: 'POST',
        body: JSON.stringify(body),
      }),
    );
    const sent = JSON.parse(fetchFn.mock.calls[0]![1].body);
    expect(sent).toMatchObject({
      expected_audit_id: audit.audit_id,
      expected_audit_sha256: audit.audit_sha256,
    });
    expect(sent).not.toHaveProperty('approved_at');
  });
});

describe('deleteSession', () => {
  it('DELETEs /api/sessions/:sid and accepts an empty 204 response', async () => {
    const fetchFn = vi.fn().mockResolvedValue({
      ok: true,
      status: 204,
      statusText: 'No Content',
    } as Response);
    vi.stubGlobal('fetch', fetchFn);

    await expect(deleteSession('sid-1')).resolves.toBeUndefined();
    expect(fetchFn).toHaveBeenCalledWith('/api/sessions/sid-1', { method: 'DELETE' });
  });

  it('throws ApiError on delete failure', async () => {
    mockFetchOnce(404, { error_code: 'SessionNotFound', message: 'missing' });
    await expect(deleteSession('sid-1')).rejects.toBeInstanceOf(ApiError);
  });
});

describe('postDataset', () => {
  it('POSTs the CSV file as backend-compatible JSON/base64', async () => {
    const summary: DatasetSummary = {
      dataset_id: '00000000-0000-4000-8000-000000000001',
      file_name: 'data.csv',
      size_bytes: 8,
      encoding: 'Utf8',
      row_count: 1,
      columns: [{ name: 'a', inferred_type: 'Numeric', missing_count: 0 }],
      uploaded_at: '2026-01-01T00:00:00.000Z',
      sha256: null,
    };
    const fetchFn = mockFetchOnce(201, summary);
    const file = new File(['a,b\n1,2\n'], 'data.csv', { type: 'text/csv' });

    const out = await postDataset('sid-1', file);

    expect(fetchFn).toHaveBeenCalledWith(
      '/api/sessions/sid-1/datasets',
      expect.objectContaining({
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
      }),
    );
    const [, init] = fetchFn.mock.calls[0]!;
    const body = JSON.parse((init as RequestInit).body as string) as { filename: string; data: string };
    expect(body.filename).toBe('data.csv');
    expect(atob(body.data)).toBe('a,b\n1,2\n');
    expect(out).toEqual(summary);
  });
});
