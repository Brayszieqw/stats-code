/**
 * Smoke tests for `useLlmStatus`.
 *
 * Covers the Requirement 10.1 / 11.1 contract:
 *   1. Mount → fetch `/api/llm-status` → seed `configured` / `provider`.
 *   2. Backend says unconfigured → hook stays unconfigured.
 *   3. `setConfigured(provider)` flips the state without a re-fetch.
 *   4. `requireReconfigure()` forces `configured = false` again.
 *   5. `setRuntimeError` / `clearRuntimeError` round-trip.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { renderHook, act, waitFor } from '@testing-library/react';
import { useLlmStatus } from '../useLlmStatus';

function mockFetchJson(status: number, body: unknown): void {
  vi.stubGlobal(
    'fetch',
    vi.fn(async (input: RequestInfo | URL) => {
      const url = typeof input === 'string' ? input : input.toString();
      if (!url.includes('/api/llm-status')) {
        throw new Error(`unexpected fetch: ${url}`);
      }
      return new Response(JSON.stringify(body), {
        status,
        headers: { 'Content-Type': 'application/json' },
      });
    }),
  );
}

describe('useLlmStatus', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('marks the hook as configured once the backend confirms it', async () => {
    mockFetchJson(200, {
      configured: true,
      provider: 'deepseek',
      base_url: 'https://api.deepseek.com/v1',
      model: 'deepseek-chat',
      cached_providers: ['deepseek', 'qwen'],
    });

    const { result } = renderHook(() => useLlmStatus());

    expect(result.current.fetchState).toBe('loading');

    await waitFor(() => {
      expect(result.current.fetchState).toBe('ready');
    });
    expect(result.current.configured).toBe(true);
    expect(result.current.provider).toBe('deepseek');
    expect(result.current.base_url).toBe('https://api.deepseek.com/v1');
    expect(result.current.model).toBe('deepseek-chat');
    expect(result.current.cached_providers).toEqual(['deepseek', 'qwen']);
    expect(result.current.runtime_error).toBeNull();
  });

  it('stays unconfigured when /api/llm-status returns configured=false', async () => {
    mockFetchJson(200, { configured: false, provider: null });

    const { result } = renderHook(() => useLlmStatus());

    await waitFor(() => {
      expect(result.current.fetchState).toBe('ready');
    });
    expect(result.current.configured).toBe(false);
    expect(result.current.provider).toBeNull();
    expect(result.current.model).toBeNull();
  });

  it('defaults cached_providers to [] when the field is absent', async () => {
    mockFetchJson(200, { configured: true, provider: 'deepseek' });

    const { result } = renderHook(() => useLlmStatus());

    await waitFor(() => {
      expect(result.current.fetchState).toBe('ready');
    });
    expect(result.current.cached_providers).toEqual([]);
  });

  it('exposes mutators that flip configured state without re-fetching', async () => {
    mockFetchJson(200, { configured: false, provider: null });

    const { result } = renderHook(() => useLlmStatus());
    await waitFor(() => {
      expect(result.current.fetchState).toBe('ready');
    });

    act(() => {
      result.current.setConfigured('qwen', 'https://dashscope.aliyuncs.com/compatible-mode/v1', 'qwen-max');
    });
    expect(result.current.configured).toBe(true);
    expect(result.current.provider).toBe('qwen');
    expect(result.current.base_url).toBe('https://dashscope.aliyuncs.com/compatible-mode/v1');
    expect(result.current.model).toBe('qwen-max');

    act(() => {
      result.current.requireReconfigure();
    });
    expect(result.current.configured).toBe(false);
    // Provider is left intact so the card can pre-select the previous value.
    expect(result.current.provider).toBe('qwen');
    expect(result.current.model).toBe('qwen-max');
  });

  it('records and clears runtime LLM errors', async () => {
    mockFetchJson(200, { configured: true, provider: 'qwen' });

    const { result } = renderHook(() => useLlmStatus());
    await waitFor(() => {
      expect(result.current.fetchState).toBe('ready');
    });

    act(() => {
      result.current.setRuntimeError({
        provider: 'qwen',
        summary: 'rate limited',
        last_message_id: 'msg-1',
      });
    });
    expect(result.current.runtime_error?.summary).toBe('rate limited');

    act(() => {
      result.current.clearRuntimeError();
    });
    expect(result.current.runtime_error).toBeNull();
  });

  it('falls back to unconfigured when the fetch errors out', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(async () =>
        new Response(JSON.stringify({ error_code: 'LlmUnavailable', message: 'boom' }), {
          status: 500,
          headers: { 'Content-Type': 'application/json' },
        }),
      ),
    );

    const { result } = renderHook(() => useLlmStatus());

    await waitFor(() => {
      expect(result.current.fetchState).toBe('error');
    });
    expect(result.current.configured).toBe(false);
    expect(result.current.provider).toBeNull();
    expect(result.current.model).toBeNull();
    expect(result.current.fetchError).toBe('boom');
  });
});
