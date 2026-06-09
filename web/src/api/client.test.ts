/**
 * Tests for the new dual-mode client methods listSessions / runSkill.
 *
 * Validates: Requirements 11.1, 12.1
 */

import { describe, it, expect, vi, afterEach } from 'vitest';
import { listSessions, runSkill, ApiError } from './client';
import type { SessionSummary, SkillResult } from './types';

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
