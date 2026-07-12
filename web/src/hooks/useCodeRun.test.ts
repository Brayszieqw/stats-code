/**
 * Tests for useCodeRun.
 *
 * Validates: Requirements 7.4, 12.7
 */

import { describe, it, expect, vi, afterEach } from 'vitest';
import { renderHook, act, waitFor } from '@testing-library/react';
import { useCodeRun } from './useCodeRun';

afterEach(() => {
  vi.unstubAllGlobals();
});

function stubFetch(impl: () => Promise<Response>) {
  vi.stubGlobal('fetch', vi.fn(impl));
}

const okResponse = (body: unknown) =>
  ({ ok: true, status: 200, statusText: 'ok', json: async () => body }) as Response;
const errResponse = (status: number, body: unknown) =>
  ({ ok: false, status, statusText: 'err', json: async () => body }) as Response;

const RUN_BODY = { skill_id: 'model_linear', dataset_id: 'ds-1', args: { outcome: 'y', predictors: ['x'] } };

describe('useCodeRun state machine (Requirements 7.4, 12.7)', () => {
  it('idle → running → success', async () => {
    const result = { schema_version: '1.0', payload: {}, risk_signals: [] };
    stubFetch(async () => okResponse(result));
    const { result: r } = renderHook(() => useCodeRun());
    expect(r.current.state.status).toBe('idle');
    await act(async () => {
      await r.current.run('sid', RUN_BODY);
    });
    expect(r.current.state.status).toBe('success');
    if (r.current.state.status === 'success') {
      expect(r.current.state.result).toEqual(result);
    }
  });

  it('error path surfaces the error code/message', async () => {
    stubFetch(async () => errResponse(422, { error_code: 'SkillInvalidArgs', message: '缺少参数' }));
    const { result: r } = renderHook(() => useCodeRun());
    await act(async () => {
      await r.current.run('sid', RUN_BODY);
    });
    expect(r.current.state.status).toBe('error');
    if (r.current.state.status === 'error') {
      expect(r.current.state.code).toBe('SkillInvalidArgs');
      expect(r.current.state.message).toBe('缺少参数');
    }
  });

  it('reset returns to idle', async () => {
    stubFetch(async () => okResponse({ schema_version: '1.0', payload: {}, risk_signals: [] }));
    const { result: r } = renderHook(() => useCodeRun());
    await act(async () => {
      await r.current.run('sid', RUN_BODY);
    });
    expect(r.current.state.status).toBe('success');
    act(() => r.current.reset());
    expect(r.current.state.status).toBe('idle');
  });

  it('stop aborts an in-flight run and leaves state idle', async () => {
    let resolveFetch: ((r: Response) => void) | null = null;
    stubFetch(() => new Promise<Response>((res) => { resolveFetch = res; }));
    const { result: r } = renderHook(() => useCodeRun());
    let pending: Promise<unknown> | null = null;
    act(() => {
      pending = r.current.run('sid', RUN_BODY);
    });
    await waitFor(() => expect(r.current.state.status).toBe('running'));
    act(() => r.current.stop());
    expect(r.current.state.status).toBe('idle');
    // Resolve the late fetch; aborted run must not flip back to success.
    await act(async () => {
      resolveFetch?.(okResponse({ schema_version: '1.0', payload: {}, risk_signals: [] }));
      await pending;
    });
    expect(r.current.state.status).toBe('idle');
  });
});
